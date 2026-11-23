//! E4 AMM DEX — constant-product pools (ECOSYSTEM_PLAN E4).
//!
//! Uniswap-V2-style automated market maker over [`Amount`] (u128, 18
//! decimals — decision D-T1): constant-product pools `x * y = k` with LP
//! shares, fee-on-input swaps (30 bps), an exact-output slippage guard
//! and multi-hop routing. The gate property is the swap invariant:
//! **`k` never decreases over a swap** (fee-on-input plus `floor`
//! rounding). All integer math is deterministic; `mul_div` keeps every
//! product in range via a 256-bit intermediate.
//!
//! Explained deviations from Uniswap V2 (recorded in the E4 receipt):
//! the initial LP supply is `amount_a + amount_b` instead of
//! `sqrt(amount_a * amount_b)` (the integer sqrt would need 256-bit
//! math), and LP shares are accounted pool-side without per-user
//! balances. Routing charges the pool fee per hop and enforces the
//! caller's `min_out` on the final hop only.

use core::cmp::min;
use std::fmt;

/// Token identifier (same 16-byte handle style as oracle feeds).
pub type Token = [u8; 16];

/// Token amounts: u128 with 18 decimals (decision D-T1).
pub type Amount = u128;

/// 256-bit product as little-endian `u64` limbs (pool invariant `k`).
pub type Wide = [u64; 4];

/// One whole token unit (18 decimals).
pub const E4_ONE: Amount = 1_000_000_000_000_000_000;
/// Basis points denominator.
pub const E4_BPS: u64 = 10_000;
/// Swap fee, 30 bps (0.30% — Uniswap V2 style, fee-on-input).
pub const E4_FEE_BPS: u64 = 30;

/// Errors with named messages (anchored one-per-message by `tools/nc_e4.py`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum E4Error {
    /// A swap was asked to move zero input.
    ZeroInput,
    /// A swap was attempted against a pool with an empty side.
    NoLiquidity,
    /// The output fell below the caller's `min_out` slippage guard.
    Slippage {
        /// Computed output.
        out: Amount,
        /// Caller's minimum acceptable output.
        min_out: Amount,
    },
    /// An initial mint asked to mint zero on either side.
    ZeroMint,
    /// A proportional mint was not proportional to pool reserves.
    MintRatio,
    /// A burn asked for more LP shares than exist.
    BurnExceeds {
        /// Requested LP amount.
        lp: Amount,
        /// Current LP supply.
        supply: Amount,
    },
    /// A route had no hops or undersized pools for its path.
    ShortRoute,
    /// A `mul_div` left the representable range.
    Overflow,
    /// A swap or route referenced a token the pool does not hold.
    TokenNotInPool {
        /// Offending token.
        token: Token,
    },
}

impl fmt::Display for E4Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroInput => write!(f, "E4: swap input must be positive"),
            Self::NoLiquidity => write!(f, "E4: pool has no liquidity"),
            Self::Slippage { out, min_out } => write!(
                f,
                "E4: slippage guard: output {out} below minimum {min_out}"
            ),
            Self::ZeroMint => {
                write!(f, "E4: initial mint amounts must both be positive")
            }
            Self::MintRatio => {
                write!(f, "E4: mint amounts must be proportional to reserves")
            }
            Self::BurnExceeds { lp, supply } => {
                write!(f, "E4: burn exceeds LP supply ({lp}/{supply})")
            }
            Self::ShortRoute => {
                write!(f, "E4: route must contain at least one hop")
            }
            Self::Overflow => {
                write!(f, "E4: amount out of representable range")
            }
            Self::TokenNotInPool { token } => {
                write!(f, "E4: swap token not in pool ({token:?})")
            }
        }
    }
}

impl std::error::Error for E4Error {}

/// Fallible E4 operation.
pub type Result<T> = core::result::Result<T, E4Error>;

/// Product `a * b` as 256-bit limbs (little-endian).
pub fn wide_mul(a: Amount, b: Amount) -> Wide {
    let words_a = [a as u64, (a >> 64) as u64];
    let words_b = [b as u64, (b >> 64) as u64];
    let mut r = [0u64; 4];
    for i in 0..2 {
        let mut carry: u128 = 0;
        for j in 0..2 {
            let cur = (words_a[i] as u128) * (words_b[j] as u128) + (r[i + j] as u128) + carry;
            r[i + j] = cur as u64;
            carry = cur >> 64;
        }
        let mut k = i + 2;
        while carry != 0 && k < 4 {
            let cur = (r[k] as u128) + carry;
            r[k] = cur as u64;
            carry = cur >> 64;
            k += 1;
        }
    }
    r
}

/// `x >= y` for 256-bit limb values (little-endian).
pub fn wide_ge(x: &Wide, y: &Wide) -> bool {
    (x[3], x[2], x[1], x[0]) >= (y[3], y[2], y[1], y[0])
}

/// `floor(a * b / d)` through a 256-bit intermediate. `None` when
/// `d == 0`, `d >= 2^127` (running remainder must fit `u128 << 1`) or
/// the quotient exceeds `u128`.
pub fn mul_div(a: Amount, b: Amount, d: Amount) -> Option<Amount> {
    if d == 0 || d >> 127 != 0 {
        return None;
    }
    let product = wide_mul(a, b);
    let mut q = [0u64; 4];
    let mut r: u128 = 0;
    for bit in (0..256).rev() {
        let bit_val = (product[bit / 64] >> (bit % 64)) & 1;
        r = (r << 1) | u128::from(bit_val);
        if r >= d {
            r -= d;
            q[bit / 64] |= 1 << (bit % 64);
        }
    }
    if q[2] != 0 || q[3] != 0 {
        return None;
    }
    Some(q[0] as u128 | ((q[1] as u128) << 64))
}

/// A constant-product pool over an unordered token pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pool {
    token_a: Token,
    token_b: Token,
    reserve_a: Amount,
    reserve_b: Amount,
    lp_supply: Amount,
    fee_bps: u64,
}

impl Pool {
    /// New empty pool for `(token_a, token_b)` at [`E4_FEE_BPS`].
    pub fn new(token_a: Token, token_b: Token) -> Self {
        Self {
            token_a,
            token_b,
            reserve_a: 0,
            reserve_b: 0,
            lp_supply: 0,
            fee_bps: E4_FEE_BPS,
        }
    }

    /// Builder: swap fee in basis points (must be `< E4_BPS`).
    pub fn with_fee(mut self, fee_bps: u64) -> Self {
        assert!(fee_bps < E4_BPS, "E4: fee must be below 100%");
        self.fee_bps = fee_bps;
        self
    }

    /// First token of the pair.
    pub fn token_a(&self) -> Token {
        self.token_a
    }

    /// Second token of the pair.
    pub fn token_b(&self) -> Token {
        self.token_b
    }

    /// Reserve of [`Self::token_a`].
    pub fn reserve_a(&self) -> Amount {
        self.reserve_a
    }

    /// Reserve of [`Self::token_b`].
    pub fn reserve_b(&self) -> Amount {
        self.reserve_b
    }

    /// Total LP share supply.
    pub fn lp_supply(&self) -> Amount {
        self.lp_supply
    }

    /// Swap fee in basis points.
    pub fn fee_bps(&self) -> u64 {
        self.fee_bps
    }

    /// The pool holds `token` on one of its sides.
    pub fn holds(&self, token: Token) -> bool {
        token == self.token_a || token == self.token_b
    }

    /// Constant-product invariant `k = reserve_a * reserve_b` in
    /// 256-bit limbs (256-bit on purpose: `k` overflows `u128` for
    /// ordinary reserve sizes).
    pub fn invariant_k(&self) -> Wide {
        wide_mul(self.reserve_a, self.reserve_b)
    }

    /// The reserve of the side `token` sits on.
    fn reserve_of(&self, token: Token) -> Option<Amount> {
        if token == self.token_a {
            Some(self.reserve_a)
        } else if token == self.token_b {
            Some(self.reserve_b)
        } else {
            None
        }
    }

    /// Deposit proportional liquidity; returns the LP shares minted.
    ///
    /// The first deposit mints `amount_a + amount_b` shares (explained
    /// deviation); later deposits must be proportional to the reserves
    /// and mint `min(a * S / ra, b * S / rb)`.
    pub fn add_liquidity(&mut self, amount_a: Amount, amount_b: Amount) -> Result<Amount> {
        if self.lp_supply == 0 {
            if amount_a == 0 || amount_b == 0 {
                return Err(E4Error::ZeroMint);
            }
            let lp = amount_a + amount_b;
            self.reserve_a += amount_a;
            self.reserve_b += amount_b;
            self.lp_supply = lp;
            return Ok(lp);
        }
        if amount_a == 0 || amount_b == 0 {
            return Err(E4Error::ZeroMint);
        }
        if wide_mul(amount_a, self.reserve_b) != wide_mul(amount_b, self.reserve_a) {
            return Err(E4Error::MintRatio);
        }
        let share_a = mul_div(amount_a, self.lp_supply, self.reserve_a).ok_or(E4Error::Overflow)?;
        let share_b = mul_div(amount_b, self.lp_supply, self.reserve_b).ok_or(E4Error::Overflow)?;
        let lp = min(share_a, share_b);
        self.reserve_a += amount_a;
        self.reserve_b += amount_b;
        self.lp_supply += lp;
        Ok(lp)
    }

    /// Burn LP shares; returns the pro-rata `(amount_a, amount_b)`.
    pub fn remove_liquidity(&mut self, lp: Amount) -> Result<(Amount, Amount)> {
        if lp == 0 || lp > self.lp_supply {
            return Err(E4Error::BurnExceeds {
                lp,
                supply: self.lp_supply,
            });
        }
        let out_a = mul_div(lp, self.reserve_a, self.lp_supply).ok_or(E4Error::Overflow)?;
        let out_b = mul_div(lp, self.reserve_b, self.lp_supply).ok_or(E4Error::Overflow)?;
        self.reserve_a -= out_a;
        self.reserve_b -= out_b;
        self.lp_supply -= lp;
        Ok((out_a, out_b))
    }

    /// Quote: output of `amount_in` of `token_in` at the current
    /// reserves (fee-on-input, `floor` rounding), without trading.
    pub fn get_amount_out(&self, token_in: Token, amount_in: Amount) -> Result<Amount> {
        if amount_in == 0 {
            return Err(E4Error::ZeroInput);
        }
        let reserve_in = self
            .reserve_of(token_in)
            .ok_or(E4Error::TokenNotInPool { token: token_in })?;
        let reserve_out = if token_in == self.token_a {
            self.reserve_b
        } else {
            self.reserve_a
        };
        if reserve_in == 0 || reserve_out == 0 {
            return Err(E4Error::NoLiquidity);
        }
        let in_fee = mul_div(
            amount_in,
            u128::from(E4_BPS - self.fee_bps),
            u128::from(E4_BPS),
        )
        .ok_or(E4Error::Overflow)?;
        let out = mul_div(in_fee, reserve_out, reserve_in + in_fee).ok_or(E4Error::Overflow)?;
        Ok(out)
    }

    /// Swap exactly `amount_in` of `token_in`, refusing outputs below
    /// `min_out` (the slippage guard). Returns the output amount.
    pub fn swap_exact_in(
        &mut self,
        token_in: Token,
        amount_in: Amount,
        min_out: Amount,
    ) -> Result<Amount> {
        let out = self.get_amount_out(token_in, amount_in)?;
        if out < min_out {
            return Err(E4Error::Slippage { out, min_out });
        }
        if token_in == self.token_a {
            self.reserve_a += amount_in;
            self.reserve_b -= out;
        } else {
            self.reserve_b += amount_in;
            self.reserve_a -= out;
        }
        Ok(out)
    }
}

/// Multi-hop swap along `path` (`pools[i]` serves hop `path[i] ->
/// path[i + 1]`), fee per hop, `min_out` enforced on the final hop.
/// Returns the final output amount.
pub fn swap_route(
    pools: &mut [Pool],
    path: &[Token],
    amount_in: Amount,
    min_out: Amount,
) -> Result<Amount> {
    if path.len() < 2 || pools.len() + 1 != path.len() {
        return Err(E4Error::ShortRoute);
    }
    let mut current = amount_in;
    let last_hop = pools.len() - 1;
    for (i, pool) in pools.iter_mut().enumerate() {
        if !pool.holds(path[i]) {
            return Err(E4Error::TokenNotInPool { token: path[i] });
        }
        if !pool.holds(path[i + 1]) {
            return Err(E4Error::TokenNotInPool { token: path[i + 1] });
        }
        let hop_min = if i == last_hop { min_out } else { 0 };
        current = pool.swap_exact_in(path[i], current, hop_min)?;
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_mul_known_values() {
        assert_eq!(wide_mul(0, u128::MAX), [0, 0, 0, 0]);
        assert_eq!(wide_mul(1, 1), [1, 0, 0, 0]);
        // (2^64-1)^2 = 2^128 - 2^65 + 1.
        let m = u64::MAX as u128;
        assert_eq!(wide_mul(m, m), [1, 0xFFFF_FFFF_FFFF_FFFE, 0, 0]);
        // u128::MAX^2 = 2^256 - 2^129 + 1 -> limbs [1, 0, max-1, max].
        let max = u128::MAX;
        assert_eq!(wide_mul(max, max), [1, 0, u64::MAX - 1, u64::MAX]);
    }

    #[test]
    fn wide_ge_orders_by_most_significant_limb() {
        assert!(wide_ge(&[0, 0, 0, 1], &[u64::MAX, u64::MAX, u64::MAX, 0]));
        assert!(wide_ge(&[5, 0, 0, 0], &[5, 0, 0, 0]));
        assert!(!wide_ge(&[0, 0, 0, 0], &[1, 0, 0, 0]));
    }

    #[test]
    fn mul_div_floor_and_range() {
        assert_eq!(mul_div(10, 10, 3), Some(33));
        assert_eq!(mul_div(1, 1, 2), Some(0));
        assert_eq!(mul_div(1, 1, 0), None);
        // (u128::MAX * 2) / 3 still fits in u128; u128::MAX^2 does not.
        assert_eq!(
            mul_div(u128::MAX, 2, 3),
            Some(226_854_911_280_625_642_308_916_404_954_512_140_970)
        );
        assert_eq!(mul_div(u128::MAX, u128::MAX, 1), None);
        // big product that would overflow u128 intermediates
        let big = 10u128.pow(30);
        assert_eq!(mul_div(big, big, big), Some(big));
        assert_eq!(
            mul_div(1974316068794122597, 1, 1),
            Some(1974316068794122597)
        );
    }

    #[test]
    fn swap_quote_matches_golden() {
        let mut pool = Pool::new(*b"ALCRY...........", *b"AUSD............");
        pool.add_liquidity(100 * E4_ONE, 200 * E4_ONE).unwrap();
        let out = pool.get_amount_out(*b"ALCRY...........", E4_ONE).unwrap();
        assert_eq!(out, 1_974_316_068_794_122_597);
    }
}
