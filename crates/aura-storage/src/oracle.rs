//! E5 — Oracle: decentralized price feed network.
//!
//! A feed network of registered providers publishes signed price reports.
//! A round aggregates them deterministically:
//!
//! 1. **admission** — every report must carry a valid ed25519 signature from a
//!    registered provider, target this feed, be fresh (within
//!    [`E5_MAX_STALENESS_SECS`] of `now`, never from the future) and appear at
//!    most once per provider per round;
//! 2. **quorum** — at least [`E5_MIN_QUORUM`] (N/2+1) admitted reports;
//! 3. **aggregation** — median of admitted prices, then an outlier band
//!    (±[`E5_DEVIATION_BPS`] bps around that median) drops manipulation
//!    attempts, and the median of the survivors is published.
//!
//! The design follows the decentralized oracle network (DON) shape used by
//! Chainlink-style feeds: N independent providers, median aggregation, a
//! quorum floor, a heartbeat/staleness bound and outlier rejection. With
//! fewer than half of the providers malicious the published median is the
//! honest price: two liars among five cannot move it (the E5 property gate).
//!
//! Determinism: `submit` is a pure function of `(now, reports)` given the
//! network configuration; the lower median (element `n/2` of the sorted
//! slice) is used everywhere, including for the aggregate timestamp.
//!
//! Strict admission: any invalid report aborts the whole round with a typed
//! error (named gate message). Production systems would quarantine bad
//! reports and continue; the strict policy is chosen here so that every
//! failure mode maps to exactly one red gate message under mutation testing.

use aura_crypto::{sign_raw, verify_raw};

/// Fixed-point price, 18 decimals (same scale as [`crate::econ::Amount`]).
pub type Price = u128;

/// Heartbeat / staleness bound in seconds (Chainlink-like 1-hour heartbeat).
pub const E5_MAX_STALENESS_SECS: u64 = 3600;

/// Minimum number of admitted reports per round (N/2+1 for the 5-provider
/// reference network; a round cannot finalize below this).
pub const E5_MIN_QUORUM: usize = 3;

/// Outlier band around the provisional median, in basis points (5%).
pub const E5_DEVIATION_BPS: u64 = 500;

/// Fixed-point multiplier matching the 18-decimal price scale.
pub const E5_ONE: Price = 1_000_000_000_000_000_000;

/// Every way a round can fail. Each variant maps to exactly one named gate
/// message, used verbatim by the negative controls in `tools/nc_e5.py`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum E5Error {
    /// The round received no reports at all.
    NoReports,
    /// A report came from a provider that is not registered.
    UnknownProvider {
        /// Offending provider id.
        provider: u32,
    },
    /// A provider submitted more than one report in the same round.
    DuplicateProvider {
        /// Offending provider id.
        provider: u32,
    },
    /// A report targets a different feed than the network serves.
    FeedMismatch {
        /// Offending provider id.
        provider: u32,
    },
    /// A report's ed25519 signature does not verify against the provider key.
    BadSignature {
        /// Offending provider id.
        provider: u32,
    },
    /// A report is older than the staleness bound or comes from the future.
    Stale {
        /// Offending provider id.
        provider: u32,
        /// Report timestamp.
        timestamp: u64,
        /// Round logical time.
        now: u64,
    },
    /// Fewer than `quorum` reports were admitted.
    Quorum {
        /// Admitted report count.
        valid: usize,
        /// Quorum floor in force.
        required: usize,
    },
}

impl std::fmt::Display for E5Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoReports => write!(f, "E5: round requires at least one report"),
            Self::UnknownProvider { provider } => {
                write!(f, "E5: report from unregistered provider {provider}")
            }
            Self::DuplicateProvider { provider } => {
                write!(
                    f,
                    "E5: one report per provider per round (provider {provider})"
                )
            }
            Self::FeedMismatch { provider } => {
                write!(
                    f,
                    "E5: all reports must target this feed (provider {provider})"
                )
            }
            Self::BadSignature { provider } => {
                write!(
                    f,
                    "E5: provider report signature must verify (provider {provider})"
                )
            }
            Self::Stale {
                provider,
                timestamp,
                now,
            } => write!(
                f,
                "E5: provider report must be fresh (provider {provider}, ts {timestamp}, now {now})"
            ),
            Self::Quorum { valid, required } => write!(
                f,
                "E5: oracle requires a quorum of provider reports ({valid}/{required})"
            ),
        }
    }
}

/// A registered feed provider with an ed25519 verification key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    /// Stable provider identifier used in reports.
    pub id: u32,
    /// Ed25519 verification key of the provider.
    pub pubkey: [u8; 32],
}

/// One signed price observation from a provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceReport {
    /// Authoring provider id.
    pub provider: u32,
    /// Feed identifier (e.g. `b"ALCRY/USD......."`).
    pub feed: [u8; 16],
    /// Quoted price, 18 decimals.
    pub price: Price,
    /// Observation time (logical seconds).
    pub timestamp: u64,
    /// Ed25519 signature over the canonical signing message.
    pub signature: [u8; 64],
}

impl PriceReport {
    /// Canonical signing message: `feed (16) || price (16, BE) || timestamp (8, BE)`.
    pub fn signing_message(feed: &[u8; 16], price: Price, timestamp: u64) -> Vec<u8> {
        let mut out = Vec::with_capacity(40);
        out.extend_from_slice(feed);
        out.extend_from_slice(&price.to_be_bytes());
        out.extend_from_slice(&timestamp.to_be_bytes());
        out
    }

    /// Build a report signed with the provider's ed25519 seed.
    pub fn sign(
        seed: &[u8; 32],
        provider: u32,
        feed: [u8; 16],
        price: Price,
        timestamp: u64,
    ) -> Self {
        let message = Self::signing_message(&feed, price, timestamp);
        let signature = sign_raw(seed, &message);
        Self {
            provider,
            feed,
            price,
            timestamp,
            signature,
        }
    }

    /// Verify the report signature against a provider public key.
    pub fn verify(&self, pubkey: &[u8; 32]) -> bool {
        let message = Self::signing_message(&self.feed, self.price, self.timestamp);
        verify_raw(pubkey, &message, &self.signature).is_ok()
    }
}

/// The published result of one round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregatedPrice {
    /// Feed this aggregate belongs to.
    pub feed: [u8; 16],
    /// Final price: median of the reports that survived the outlier band.
    pub price: Price,
    /// Median timestamp of the surviving reports (lower median).
    pub timestamp: u64,
    /// Reports admitted by the admission stage.
    pub admitted: usize,
    /// Reports dropped by the outlier band.
    pub rejected_outliers: usize,
}

/// A price feed network: registered providers + aggregation policy.
#[derive(Debug, Clone)]
pub struct OracleNetwork {
    feed: [u8; 16],
    providers: Vec<Provider>,
    quorum: usize,
    max_staleness: u64,
    deviation_bps: u64,
    latest: Option<AggregatedPrice>,
}

impl OracleNetwork {
    /// Build a network with the default policy: quorum = N/2+1 (never below
    /// [`E5_MIN_QUORUM`]), staleness [`E5_MAX_STALENESS_SECS`], outlier band
    /// [`E5_DEVIATION_BPS`].
    pub fn new(feed: [u8; 16], providers: Vec<Provider>) -> Self {
        let n = providers.len();
        let quorum = std::cmp::max(n / 2 + 1, E5_MIN_QUORUM.min(n.max(1)));
        Self {
            feed,
            providers,
            quorum,
            max_staleness: E5_MAX_STALENESS_SECS,
            deviation_bps: E5_DEVIATION_BPS,
            latest: None,
        }
    }

    /// Override the quorum floor (used by tests and negative controls).
    pub fn with_quorum(mut self, quorum: usize) -> Self {
        self.quorum = quorum;
        self
    }

    /// Override the staleness bound in seconds.
    pub fn with_staleness(mut self, secs: u64) -> Self {
        self.max_staleness = secs;
        self
    }

    /// Override the outlier band in basis points.
    pub fn with_deviation(mut self, bps: u64) -> Self {
        self.deviation_bps = bps;
        self
    }

    /// Quorum floor currently in force.
    pub fn quorum(&self) -> usize {
        self.quorum
    }

    /// The feed this network serves.
    pub fn feed(&self) -> [u8; 16] {
        self.feed
    }

    /// Registered provider count.
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    /// Latest published aggregate, if any round finalized.
    pub fn latest(&self) -> Option<&AggregatedPrice> {
        self.latest.as_ref()
    }

    /// Aggregate one round of reports at logical time `now`.
    ///
    /// Admission is strict: any single invalid report aborts the round with
    /// its own error variant so that negative controls can pin exactly one
    /// named message per mutation.
    pub fn submit(
        &mut self,
        now: u64,
        reports: Vec<PriceReport>,
    ) -> Result<AggregatedPrice, E5Error> {
        if reports.is_empty() {
            return Err(E5Error::NoReports);
        }

        let mut seen = Vec::with_capacity(reports.len());
        let mut admitted: Vec<PriceReport> = Vec::with_capacity(reports.len());

        for report in &reports {
            let provider = self
                .providers
                .iter()
                .find(|p| p.id == report.provider)
                .ok_or(E5Error::UnknownProvider {
                    provider: report.provider,
                })?;

            if seen.contains(&report.provider) {
                return Err(E5Error::DuplicateProvider {
                    provider: report.provider,
                });
            }
            seen.push(report.provider);

            if report.feed != self.feed {
                return Err(E5Error::FeedMismatch {
                    provider: report.provider,
                });
            }

            if !report.verify(&provider.pubkey) {
                return Err(E5Error::BadSignature {
                    provider: report.provider,
                });
            }

            let fresh = report.timestamp <= now
                && now.saturating_sub(report.timestamp) <= self.max_staleness;
            if !fresh {
                return Err(E5Error::Stale {
                    provider: report.provider,
                    timestamp: report.timestamp,
                    now,
                });
            }

            admitted.push(report.clone());
        }

        if admitted.len() < self.quorum {
            return Err(E5Error::Quorum {
                valid: admitted.len(),
                required: self.quorum,
            });
        }

        // Provisional median over all admitted prices.
        let provisional = lower_median(admitted.iter().map(|r| r.price));

        // Outlier band around the provisional median.
        let admitted_count = admitted.len();
        let (survivors, rejected_outliers): (Vec<PriceReport>, Vec<PriceReport>) = admitted
            .into_iter()
            .partition(|r| within_band(r.price, provisional, self.deviation_bps));

        // The survivors are never empty: the provisional median is always
        // within a zero-width band of itself.
        let final_price = lower_median(survivors.iter().map(|r| r.price));
        let final_ts = lower_median(survivors.iter().map(|r| r.timestamp));

        let agg = AggregatedPrice {
            feed: self.feed,
            price: final_price,
            timestamp: final_ts,
            admitted: admitted_count,
            rejected_outliers: rejected_outliers.len(),
        };
        self.latest = Some(agg.clone());
        Ok(agg)
    }
}

/// Lower median: element `n/2` of the ascending slice (deterministic for
/// both odd and even lengths).
fn lower_median<T: Ord + Copy>(values: impl Iterator<Item = T>) -> T {
    let mut v: Vec<T> = values.collect();
    v.sort_unstable();
    v[v.len() / 2]
}

/// `true` when `price` is within `±band_bps` of `center` (0 bps = exact).
fn within_band(price: Price, center: Price, band_bps: u64) -> bool {
    let diff = price.abs_diff(center);
    // diff * 10_000 <= band_bps * center, computed without overflow for the
    // 18-decimal scale by dividing the right side first when needed.
    diff.saturating_mul(10_000) <= (band_bps as u128).saturating_mul(center)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_for(id: u32) -> [u8; 32] {
        let mut seed = [0u8; 32];
        seed[0] = id as u8;
        seed[31] = 0xE5;
        seed
    }

    fn provider(id: u32) -> Provider {
        Provider {
            id,
            pubkey: aura_crypto::ed25519_public_key(&seed_for(id)),
        }
    }

    #[test]
    fn signing_message_round_trip() {
        let feed = *b"ALCRY/USD.......";
        let msg = PriceReport::signing_message(&feed, 3 * E5_ONE, 1_000);
        assert_eq!(msg.len(), 40);
    }

    #[test]
    fn report_verifies() {
        let feed = *b"ALCRY/USD.......";
        let p = provider(1);
        let report = PriceReport::sign(&seed_for(1), 1, feed, E5_ONE, 500);
        assert!(report.verify(&p.pubkey));
    }
}
