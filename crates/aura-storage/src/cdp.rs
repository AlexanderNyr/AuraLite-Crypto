//! E6 stablecoin CDP engine (ECOSYSTEM_PLAN E6).
//!
//! Deterministic over-collateralized debt positions (MakerDAO-style
//! vaults): lock collateral, mint a USD-pegged stablecoin against it,
//! repay, withdraw — every state change keeps the vault above the
//! liquidation ratio (150%, MakerDAO ETH-A standard) or the position is
//! liquidatable. Liquidation seizes collateral worth `debt · (1 +
//! penalty)` (5% penalty, USDC-A/Raft precedent) at the current oracle
//! price, burns the repaid stablecoin and returns the leftover
//! collateral to the vault owner. The peg invariant — every unit of
//! stablecoin in circulation is backed by at least $1 of collateral held
//! by the engine (`bad_debt == 0`, `backed_value >= stable_supply`) —
//! survives the documented stress scenarios.
//!
//! Amounts are `u128` with 18 decimals (D-T1); prices are USD per whole
//! collateral unit, also 18 decimals. All products go through the
//! 256-bit [`crate::amm::mul_div`] (floor rounding at every division).

use core::cmp::min;

use crate::amm::{Amount, mul_div};

/// USD price per whole collateral unit, 18 decimals (D-T1).
pub type Price = u128;

/// Floor collateral ratio in bps: `value · 10000 / debt` with value
/// `collateral · price / 1e18`. `None` while `debt == 0` or on overflow.
fn ratio_bps_of(collateral: Amount, debt: Amount, price: Price) -> Option<u64> {
    if debt == 0 {
        return None;
    }
    let value = mul_div(collateral, price, E6_ONE)?;
    let ratio = mul_div(value, u128::from(E6_BPS), debt)?;
    u64::try_from(ratio).ok()
}

/// 18-decimal unit (1.0).
pub const E6_ONE: Amount = 1_000_000_000_000_000_000;
/// Basis points denominator (100% = 10000).
pub const E6_BPS: u64 = 10_000;
/// Minimum collateral ratio 150% (MakerDAO ETH-A liquidation ratio).
pub const E6_MIN_RATIO_BPS: u64 = 15_000;
/// Liquidation penalty 5% (MakerDAO USDC-A / Raft precedent).
pub const E6_LIQ_PENALTY_BPS: u64 = 500;

/// Named E6 error messages (exact strings are asserted by tests and
/// negative controls).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum E6Error {
    /// No vault with this identifier.
    VaultNotFound {
        /// Requested vault id.
        id: u64,
    },
    /// Deposit amount was zero.
    ZeroDeposit,
    /// Mint amount was zero.
    ZeroMint,
    /// Mint would drop the collateral ratio below the minimum.
    MintRatio {
        /// Post-mint ratio in bps.
        ratio_bps: u64,
        /// Configured minimum ratio in bps.
        min_bps: u64,
    },
    /// Repay amount was zero.
    ZeroRepay,
    /// Repay exceeds the outstanding debt.
    RepayExceeds {
        /// Requested repay amount.
        repay: Amount,
        /// Outstanding debt.
        debt: Amount,
    },
    /// Withdraw amount was zero.
    ZeroWithdraw,
    /// Withdraw would drop the collateral ratio below the minimum.
    WithdrawRatio {
        /// Post-withdraw ratio in bps.
        ratio_bps: u64,
        /// Configured minimum ratio in bps.
        min_bps: u64,
    },
    /// Withdraw exceeds the locked collateral.
    WithdrawExceeds {
        /// Requested withdraw amount.
        amount: Amount,
        /// Locked collateral.
        collateral: Amount,
    },
    /// Liquidation refused: the vault is still healthy.
    HealthyVault {
        /// Current ratio in bps.
        ratio_bps: u64,
        /// Configured minimum ratio in bps.
        min_bps: u64,
    },
    /// Oracle price was zero.
    ZeroPrice,
    /// Intermediate math left the representable range.
    Overflow,
}

impl core::fmt::Display for E6Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::VaultNotFound { id } => write!(f, "E6: vault not found ({id})"),
            Self::ZeroDeposit => write!(f, "E6: deposit must be positive"),
            Self::ZeroMint => write!(f, "E6: mint must be positive"),
            Self::MintRatio { ratio_bps, min_bps } => {
                write!(
                    f,
                    "E6: mint exceeds collateral ratio ({ratio_bps}/{min_bps})"
                )
            }
            Self::ZeroRepay => write!(f, "E6: repay must be positive"),
            Self::RepayExceeds { repay, debt } => {
                write!(f, "E6: repay exceeds debt ({repay}/{debt})")
            }
            Self::ZeroWithdraw => write!(f, "E6: withdraw must be positive"),
            Self::WithdrawRatio { ratio_bps, min_bps } => write!(
                f,
                "E6: withdraw would breach collateral ratio ({ratio_bps}/{min_bps})"
            ),
            Self::WithdrawExceeds { amount, collateral } => {
                write!(f, "E6: withdraw exceeds collateral ({amount}/{collateral})")
            }
            Self::HealthyVault { ratio_bps, min_bps } => write!(
                f,
                "E6: vault is healthy, liquidation refused ({ratio_bps} >= {min_bps})"
            ),
            Self::ZeroPrice => write!(f, "E6: price must be positive"),
            Self::Overflow => write!(f, "E6: amount out of representable range"),
        }
    }
}

impl std::error::Error for E6Error {}

/// Fallible E6 operation.
pub type Result<T> = core::result::Result<T, E6Error>;

/// One over-collateralized debt position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vault {
    /// Vault identifier (issue order from [`CdpEngine::open_vault`]).
    pub id: u64,
    /// Collateral locked, 18 decimals.
    pub collateral: Amount,
    /// Stablecoin debt outstanding, 18 decimals.
    pub debt: Amount,
}

/// Outcome of one liquidation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Liquidation {
    /// Collateral seized for the liquidator (debt + penalty value).
    pub seized: Amount,
    /// Debt repaid; stablecoin burned from supply.
    pub repaid: Amount,
    /// Unbacked shortfall — recorded as bad debt (must stay zero).
    pub shortfall: Amount,
    /// Collateral returned to the vault owner.
    pub returned: Amount,
}

/// CDP engine: vault ledger, oracle price and peg accounting.
#[derive(Debug, Clone)]
pub struct CdpEngine {
    min_ratio_bps: u64,
    penalty_bps: u64,
    price: Price,
    vaults: Vec<Vault>,
    stable_supply: Amount,
    bad_debt: Amount,
    liquidations: u64,
}

impl CdpEngine {
    /// New engine at [`E6_MIN_RATIO_BPS`] / [`E6_LIQ_PENALTY_BPS`],
    /// price 1.0 USD.
    pub fn new() -> Self {
        Self {
            min_ratio_bps: E6_MIN_RATIO_BPS,
            penalty_bps: E6_LIQ_PENALTY_BPS,
            price: E6_ONE,
            vaults: Vec::new(),
            stable_supply: 0,
            bad_debt: 0,
            liquidations: 0,
        }
    }

    /// Builder: risk parameters (min ratio and penalty in bps).
    pub fn with_params(mut self, min_ratio_bps: u64, penalty_bps: u64) -> Self {
        self.min_ratio_bps = min_ratio_bps;
        self.penalty_bps = penalty_bps;
        self
    }

    /// Current collateral price (USD, 18 decimals).
    pub fn price(&self) -> Price {
        self.price
    }

    /// Total stablecoin in circulation (minted − repaid − liquidation burns).
    pub fn stable_supply(&self) -> Amount {
        self.stable_supply
    }

    /// Cumulative unbacked debt (peg stress metric; must stay zero).
    pub fn bad_debt(&self) -> Amount {
        self.bad_debt
    }

    /// Number of liquidations performed.
    pub fn liquidations(&self) -> u64 {
        self.liquidations
    }

    /// Configured minimum ratio in bps.
    pub fn min_ratio_bps(&self) -> u64 {
        self.min_ratio_bps
    }

    /// Configured liquidation penalty in bps.
    pub fn penalty_bps(&self) -> u64 {
        self.penalty_bps
    }

    /// The vault record.
    pub fn vault(&self, id: u64) -> Option<&Vault> {
        self.vaults.iter().find(|v| v.id == id)
    }

    /// Sum of all collateral valued at the current price (USD, 18 dec).
    pub fn backed_value(&self) -> Amount {
        self.vaults.iter().fold(0u128, |acc, v| {
            acc.saturating_add(mul_div(v.collateral, self.price, E6_ONE).unwrap_or(u128::MAX))
        })
    }

    /// Collateral ratio in bps (`value · 10000 / debt`); `None` while
    /// the vault carries no debt.
    pub fn ratio_bps(&self, id: u64) -> Option<u64> {
        let vault = self.vault(id)?;
        ratio_bps_of(vault.collateral, vault.debt, self.price)
    }

    fn vault_mut(&mut self, id: u64) -> Result<&mut Vault> {
        let pos = self
            .vaults
            .iter()
            .position(|v| v.id == id)
            .ok_or(E6Error::VaultNotFound { id })?;
        Ok(&mut self.vaults[pos])
    }

    /// Open an empty vault; returns its id.
    pub fn open_vault(&mut self) -> u64 {
        let id = self.vaults.len() as u64;
        self.vaults.push(Vault {
            id,
            collateral: 0,
            debt: 0,
        });
        id
    }

    /// Lock `amount` collateral into the vault.
    pub fn deposit(&mut self, id: u64, amount: Amount) -> Result<()> {
        if amount == 0 {
            return Err(E6Error::ZeroDeposit);
        }
        let vault = self.vault_mut(id)?;
        vault.collateral += amount;
        Ok(())
    }

    /// Mint `amount` stablecoin against the vault; refused unless the
    /// collateral ratio stays `>= min_ratio_bps`.
    pub fn mint(&mut self, id: u64, amount: Amount) -> Result<()> {
        if amount == 0 {
            return Err(E6Error::ZeroMint);
        }
        let price = self.price;
        let min_bps = self.min_ratio_bps;
        let vault = self.vault_mut(id)?;
        let debt_after = vault.debt + amount;
        let value = mul_div(vault.collateral, price, E6_ONE).ok_or(E6Error::Overflow)?;
        let ratio_after =
            u64::try_from(mul_div(value, u128::from(E6_BPS), debt_after).ok_or(E6Error::Overflow)?)
                .map_err(|_| E6Error::Overflow)?;
        if ratio_after < min_bps {
            return Err(E6Error::MintRatio {
                ratio_bps: ratio_after,
                min_bps,
            });
        }
        vault.debt = debt_after;
        self.stable_supply += amount;
        Ok(())
    }

    /// Burn `amount` stablecoin against the vault's debt.
    pub fn repay(&mut self, id: u64, amount: Amount) -> Result<()> {
        if amount == 0 {
            return Err(E6Error::ZeroRepay);
        }
        let vault = self.vault_mut(id)?;
        if amount > vault.debt {
            return Err(E6Error::RepayExceeds {
                repay: amount,
                debt: vault.debt,
            });
        }
        vault.debt -= amount;
        self.stable_supply -= amount;
        Ok(())
    }

    /// Unlock `amount` collateral; refused unless the collateral ratio
    /// stays `>= min_ratio_bps` and the vault holds the collateral.
    pub fn withdraw(&mut self, id: u64, amount: Amount) -> Result<()> {
        if amount == 0 {
            return Err(E6Error::ZeroWithdraw);
        }
        let price = self.price;
        let min_bps = self.min_ratio_bps;
        let vault = self.vault_mut(id)?;
        if amount > vault.collateral {
            return Err(E6Error::WithdrawExceeds {
                amount,
                collateral: vault.collateral,
            });
        }
        if vault.debt > 0 {
            let value_after =
                mul_div(vault.collateral - amount, price, E6_ONE).ok_or(E6Error::Overflow)?;
            let ratio_after = u64::try_from(
                mul_div(value_after, u128::from(E6_BPS), vault.debt).ok_or(E6Error::Overflow)?,
            )
            .map_err(|_| E6Error::Overflow)?;
            if ratio_after < min_bps {
                return Err(E6Error::WithdrawRatio {
                    ratio_bps: ratio_after,
                    min_bps,
                });
            }
        }
        vault.collateral -= amount;
        Ok(())
    }

    /// Peg stress driver: update the collateral oracle price.
    pub fn set_price(&mut self, price: Price) -> Result<()> {
        if price == 0 {
            return Err(E6Error::ZeroPrice);
        }
        self.price = price;
        Ok(())
    }

    /// Liquidate an under-collateralized vault: seize collateral worth
    /// `debt · (1 + penalty)` at the current price (capped at the
    /// vault's collateral), burn the repaid stablecoin, return the rest
    /// to the owner. Refuses healthy vaults.
    pub fn liquidate(&mut self, id: u64) -> Result<Liquidation> {
        let price = self.price;
        let penalty_bps = self.penalty_bps;
        let min_bps = self.min_ratio_bps;
        {
            let vault = self.vault(id).ok_or(E6Error::VaultNotFound { id })?;
            let ratio = ratio_bps_of(vault.collateral, vault.debt, price);
            if vault.debt == 0 || ratio.unwrap_or(u64::MAX) >= min_bps {
                return Err(E6Error::HealthyVault {
                    ratio_bps: ratio.unwrap_or(u64::MAX),
                    min_bps,
                });
            }
        }
        let vault = self.vault_mut(id)?;
        let pen_debt = mul_div(
            vault.debt,
            u128::from(E6_BPS + penalty_bps),
            u128::from(E6_BPS),
        )
        .ok_or(E6Error::Overflow)?;
        let wanted = mul_div(pen_debt, E6_ONE, price).ok_or(E6Error::Overflow)?;
        let seized = min(wanted, vault.collateral);
        let repaid = min(
            vault.debt,
            mul_div(seized, price, E6_ONE).ok_or(E6Error::Overflow)?,
        );
        let shortfall = vault.debt - repaid;
        let returned = vault.collateral - seized;
        vault.debt = 0;
        vault.collateral = returned;
        self.stable_supply -= repaid;
        self.bad_debt += shortfall;
        self.liquidations += 1;
        Ok(Liquidation {
            seized,
            repaid,
            shortfall,
            returned,
        })
    }

    /// Peg invariant: every unit of stablecoin is backed by at least
    /// $1 of collateral held by the engine, and bad debt is zero.
    pub fn peg_holds(&self) -> bool {
        self.bad_debt == 0 && self.backed_value() >= self.stable_supply
    }
}

impl Default for CdpEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratio_boundaries() {
        let mut e = CdpEngine::new();
        let id = e.open_vault();
        e.deposit(id, 200 * E6_ONE).unwrap();
        assert_eq!(e.ratio_bps(id), None);
        e.mint(id, 100 * E6_ONE).unwrap();
        assert_eq!(e.ratio_bps(id), Some(20_000));
        e.set_price(6 * E6_ONE / 10).unwrap();
        assert_eq!(e.ratio_bps(id), Some(12_000));
    }

    #[test]
    fn mint_boundary_exact() {
        let mut e = CdpEngine::new();
        let id = e.open_vault();
        e.deposit(id, 200 * E6_ONE).unwrap();
        let dmax = 133_333_333_333_333_333_333u128;
        e.mint(id, dmax).unwrap();
        assert_eq!(e.ratio_bps(id), Some(15_000));
        let err = e.mint(id, 1).unwrap_err();
        assert_eq!(
            err.to_string(),
            "E6: mint exceeds collateral ratio (14999/15000)"
        );
    }

    #[test]
    fn withdraw_boundary_exact() {
        let mut e = CdpEngine::new();
        let id = e.open_vault();
        e.deposit(id, 200 * E6_ONE).unwrap();
        e.mint(id, 100 * E6_ONE).unwrap();
        e.withdraw(id, 50 * E6_ONE).unwrap();
        assert_eq!(e.ratio_bps(id), Some(15_000));
        let err = e.withdraw(id, 1).unwrap_err();
        assert_eq!(
            err.to_string(),
            "E6: withdraw would breach collateral ratio (14999/15000)"
        );
    }

    #[test]
    fn liquidation_golden_200_minus40() {
        let mut e = CdpEngine::new();
        let id = e.open_vault();
        e.deposit(id, 200 * E6_ONE).unwrap();
        e.mint(id, 100 * E6_ONE).unwrap();
        e.set_price(6 * E6_ONE / 10).unwrap();
        let liq = e.liquidate(id).unwrap();
        assert_eq!(liq.seized, 175 * E6_ONE);
        assert_eq!(liq.repaid, 100 * E6_ONE);
        assert_eq!(liq.shortfall, 0);
        assert_eq!(liq.returned, 25 * E6_ONE);
        assert_eq!(e.stable_supply(), 0);
        assert_eq!(e.bad_debt(), 0);
    }

    #[test]
    fn liquidation_golden_quarter_shock() {
        // Vault 300/100 at price 0.45: seized floor(105e18/0.45).
        let mut e = CdpEngine::new();
        let id = e.open_vault();
        e.deposit(id, 300 * E6_ONE).unwrap();
        e.mint(id, 100 * E6_ONE).unwrap();
        e.set_price(45 * E6_ONE / 100).unwrap();
        let liq = e.liquidate(id).unwrap();
        assert_eq!(liq.seized, 233_333_333_333_333_333_333);
        assert_eq!(liq.repaid, 100 * E6_ONE);
        assert_eq!(liq.shortfall, 0);
        assert_eq!(liq.returned, 66_666_666_666_666_666_667);
    }

    #[test]
    fn healthy_liquidation_refused() {
        let mut e = CdpEngine::new();
        let id = e.open_vault();
        e.deposit(id, 200 * E6_ONE).unwrap();
        e.mint(id, 100 * E6_ONE).unwrap();
        let err = e.liquidate(id).unwrap_err();
        assert_eq!(
            err.to_string(),
            "E6: vault is healthy, liquidation refused (20000 >= 15000)"
        );
    }

    #[test]
    fn peg_holds_across_stress() {
        let mut e = CdpEngine::new();
        let a = e.open_vault();
        let b = e.open_vault();
        let c = e.open_vault();
        for (id, col) in [(a, 200 * E6_ONE), (b, 220 * E6_ONE), (c, 300 * E6_ONE)] {
            e.deposit(id, col).unwrap();
            e.mint(id, 100 * E6_ONE).unwrap();
        }
        e.set_price(6 * E6_ONE / 10).unwrap();
        for id in [a, b] {
            let liq = e.liquidate(id).unwrap();
            assert_eq!(liq.shortfall, 0);
        }
        assert!(e.peg_holds());
        e.set_price(45 * E6_ONE / 100).unwrap();
        let liq = e.liquidate(c).unwrap();
        assert_eq!(liq.seized, 233_333_333_333_333_333_333);
        assert_eq!(liq.shortfall, 0);
        assert_eq!(e.bad_debt(), 0);
        assert!(e.peg_holds());
    }
}
