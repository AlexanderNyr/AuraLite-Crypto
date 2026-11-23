//! E7 lending market engine (ECOSYSTEM_PLAN E7).
//!
//! Deterministic over-collateralized lending (Aave/Compound-style): one
//! market with a supplied collateral asset (oracle-priced) and debt in
//! USD-pegged stable units. Borrowing is capped by the collateral
//! factor (LTV 75%); liquidation triggers when the health factor falls
//! below the liquidation threshold (LTV > 80% ⇔ health < 125%); the
//! liquidator repays up to the close factor (50% of debt) and seizes
//! collateral worth `repaid · (1 + bonus)` (5% bonus). Interest accrues
//! daily at a kinked utilization-based borrow rate
//! (Compound/Aave model): `base + slope1 · U / kink` below the kink
//! (80%), steep `slope2` above.
//!
//! The phase gate is a 30-day simulation in which the invariant
//! **`collateral_value >= debt` never breaks** per account (liquidations
//! restore positions before the invariant fails; without liquidations
//! the same path does break it — the "no liquidation → red" control).
//!
//! Amounts are `u128` with 18 decimals (D-T1); prices are USD per whole
//! collateral unit, also 18 decimals. All products go through the
//! 256-bit [`crate::amm::mul_div`] (floor rounding at every division).

use crate::amm::{Amount, mul_div};

/// USD price per whole collateral unit, 18 decimals (D-T1).
pub type Price = u128;

/// 18-decimal unit (1.0).
pub const E7_ONE: Amount = 1_000_000_000_000_000_000;
/// Basis points denominator (100% = 10000).
pub const E7_BPS: u64 = 10_000;
/// Borrow cap: 75% of collateral value (Aave-style LTV).
pub const E7_LTV_BPS: u64 = 7_500;
/// Liquidation health threshold: liquidatable below 12500 bps health
/// (LTV above 80% — the gap to LTV is the price buffer).
pub const E7_LIQ_HEALTH_BPS: u64 = 12_500;
/// Liquidation bonus for the liquidator (5%, Aave ETH precedent).
pub const E7_LIQ_BONUS_BPS: u64 = 500;
/// Close factor: one liquidation repays at most 50% of the debt.
pub const E7_CLOSE_FACTOR_BPS: u64 = 5_000;
/// Interest rate model: base 2% APR.
pub const E7_BASE_BPS: u64 = 200;
/// Interest rate model: slope 10% APR below the kink.
pub const E7_SLOPE1_BPS: u64 = 1_000;
/// Interest rate model: kink at 80% utilization.
pub const E7_KINK_BPS: u64 = 8_000;
/// Interest rate model: steep slope 30% APR above the kink.
pub const E7_SLOPE2_BPS: u64 = 3_000;

/// Named E7 error messages (exact strings are asserted by tests and
/// negative controls).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum E7Error {
    /// No account with this identifier.
    AccountNotFound {
        /// Requested account id.
        id: u64,
    },
    /// Supply amount was zero.
    ZeroSupply,
    /// Withdraw amount was zero.
    ZeroWithdraw,
    /// Withdraw exceeds the supplied collateral.
    WithdrawExceeds {
        /// Requested withdraw amount.
        amount: Amount,
        /// Supplied collateral.
        supplied: Amount,
    },
    /// Withdraw would drop the account below borrow capacity.
    WithdrawCapacity {
        /// Requested withdraw amount.
        amount: Amount,
        /// Maximum withdrawable amount.
        capacity: Amount,
    },
    /// Borrow amount was zero.
    ZeroBorrow,
    /// Borrow exceeds the collateral capacity (LTV).
    BorrowCapacity {
        /// Requested borrow amount.
        amount: Amount,
        /// Maximum additional borrow amount.
        capacity: Amount,
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
    /// Liquidation refused: the account is still healthy.
    HealthyAccount {
        /// Current health in bps.
        health_bps: u64,
        /// Liquidation threshold in bps.
        min_bps: u64,
    },
    /// Oracle price was zero.
    ZeroPrice,
    /// Intermediate math left the representable range.
    Overflow,
}

impl core::fmt::Display for E7Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::AccountNotFound { id } => write!(f, "E7: account not found ({id})"),
            Self::ZeroSupply => write!(f, "E7: supply must be positive"),
            Self::ZeroWithdraw => write!(f, "E7: withdraw must be positive"),
            Self::WithdrawExceeds { amount, supplied } => {
                write!(f, "E7: withdraw exceeds supplied ({amount}/{supplied})")
            }
            Self::WithdrawCapacity { amount, capacity } => write!(
                f,
                "E7: withdraw would breach collateral capacity ({amount}/{capacity})"
            ),
            Self::ZeroBorrow => write!(f, "E7: borrow must be positive"),
            Self::BorrowCapacity { amount, capacity } => write!(
                f,
                "E7: borrow exceeds collateral capacity ({amount}/{capacity})"
            ),
            Self::ZeroRepay => write!(f, "E7: repay must be positive"),
            Self::RepayExceeds { repay, debt } => {
                write!(f, "E7: repay exceeds debt ({repay}/{debt})")
            }
            Self::HealthyAccount {
                health_bps,
                min_bps,
            } => write!(
                f,
                "E7: account is healthy, liquidation refused ({health_bps} >= {min_bps})"
            ),
            Self::ZeroPrice => write!(f, "E7: price must be positive"),
            Self::Overflow => write!(f, "E7: amount out of representable range"),
        }
    }
}

impl std::error::Error for E7Error {}

/// Fallible E7 operation.
pub type Result<T> = core::result::Result<T, E7Error>;

/// One lending position: supplied collateral and outstanding debt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Account {
    /// Account identifier (issue order from [`LendingMarket::open_account`]).
    pub id: u64,
    /// Collateral supplied, 18 decimals.
    pub collateral: Amount,
    /// Debt outstanding (accrues interest), 18 decimals.
    pub debt: Amount,
}

/// Outcome of one liquidation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Liquidation {
    /// Debt repaid by the liquidator (at most the close factor).
    pub repaid: Amount,
    /// Collateral seized by the liquidator (repaid · (1 + bonus)).
    pub seized: Amount,
}

/// Borrow rate in bps for utilization `util_bps` (kinked two-slope
/// Compound/Aave model, floor rounding).
pub fn rate_bps_for(util_bps: u64) -> u64 {
    let util = u128::from(util_bps.min(E7_BPS));
    if util_bps <= E7_KINK_BPS {
        let slope = u128::from(E7_SLOPE1_BPS) * util / u128::from(E7_KINK_BPS);
        E7_BASE_BPS + slope as u64
    } else {
        let over = util - u128::from(E7_KINK_BPS);
        let slope = u128::from(E7_SLOPE2_BPS) * over / u128::from(E7_BPS - E7_KINK_BPS);
        E7_BASE_BPS + E7_SLOPE1_BPS + slope as u64
    }
}

/// Lending market: account ledger, oracle price, daily accrual.
#[derive(Debug, Clone)]
pub struct LendingMarket {
    ltv_bps: u64,
    liq_health_bps: u64,
    bonus_bps: u64,
    close_bps: u64,
    price: Price,
    accounts: Vec<Account>,
    days: u64,
    total_interest: Amount,
}

impl LendingMarket {
    /// New market at the documented constants (LTV 75%, liquidation
    /// health 125%, bonus 5%, close factor 50%, kinked rate model),
    /// price 1.0 USD.
    pub fn new() -> Self {
        Self {
            ltv_bps: E7_LTV_BPS,
            liq_health_bps: E7_LIQ_HEALTH_BPS,
            bonus_bps: E7_LIQ_BONUS_BPS,
            close_bps: E7_CLOSE_FACTOR_BPS,
            price: E7_ONE,
            accounts: Vec::new(),
            days: 0,
            total_interest: 0,
        }
    }

    /// Current collateral price (USD, 18 decimals).
    pub fn price(&self) -> Price {
        self.price
    }

    /// Simulated days elapsed.
    pub fn days(&self) -> u64 {
        self.days
    }

    /// Cumulative interest accrued across all accounts.
    pub fn total_interest(&self) -> Amount {
        self.total_interest
    }

    /// The account record.
    pub fn account(&self, id: u64) -> Option<&Account> {
        self.accounts.iter().find(|a| a.id == id)
    }

    /// Collateral value of the account (USD, 18 decimals, floored).
    pub fn value_of(&self, id: u64) -> Option<Amount> {
        let account = self.account(id)?;
        mul_div(account.collateral, self.price, E7_ONE)
    }

    /// Health factor in bps (`value · 10000 / debt`); `None` while the
    /// account carries no debt.
    pub fn health_bps(&self, id: u64) -> Option<u64> {
        let account = self.account(id)?;
        health_bps_of(account.collateral, account.debt, self.price)
    }

    /// Total debt across all accounts.
    pub fn total_debt(&self) -> Amount {
        self.accounts
            .iter()
            .fold(0, |acc, a| acc.saturating_add(a.debt))
    }

    /// Total collateral value across all accounts (USD, 18 dec).
    pub fn total_value(&self) -> Amount {
        self.accounts.iter().fold(0, |acc, a| {
            acc.saturating_add(mul_div(a.collateral, self.price, E7_ONE).unwrap_or(u128::MAX))
        })
    }

    /// Utilization in bps: `total_debt · 10000 / total_value`, capped.
    pub fn utilization_bps(&self) -> u64 {
        let value = self.total_value();
        if value == 0 {
            return 0;
        }
        let util = mul_div(self.total_debt(), u128::from(E7_BPS), value).unwrap_or(u128::MAX);
        u64::try_from(util).unwrap_or(u64::MAX).min(E7_BPS)
    }

    /// Current borrow rate in bps (kinked model on the utilization).
    pub fn rate_bps(&self) -> u64 {
        rate_bps_for(self.utilization_bps())
    }

    /// Borrow capacity: maximum additional borrow (0 when capped).
    pub fn borrow_capacity(&self, id: u64) -> Option<Amount> {
        let account = self.account(id)?;
        let value = mul_div(account.collateral, self.price, E7_ONE)?;
        let cap_total = mul_div(value, u128::from(self.ltv_bps), u128::from(E7_BPS))?;
        Some(cap_total.saturating_sub(account.debt))
    }

    /// Peg-style invariant: every account holds
    /// `collateral_value >= debt` (the phase gate invariant).
    pub fn invariant_holds(&self) -> bool {
        self.accounts.iter().all(|a| {
            let value = mul_div(a.collateral, self.price, E7_ONE).unwrap_or(0);
            value >= a.debt
        })
    }

    fn account_mut(&mut self, id: u64) -> Result<&mut Account> {
        let pos = self
            .accounts
            .iter()
            .position(|a| a.id == id)
            .ok_or(E7Error::AccountNotFound { id })?;
        Ok(&mut self.accounts[pos])
    }

    /// Open an empty position; returns its id.
    pub fn open_account(&mut self) -> u64 {
        let id = self.accounts.len() as u64;
        self.accounts.push(Account {
            id,
            collateral: 0,
            debt: 0,
        });
        id
    }

    /// Supply `amount` collateral to the account.
    pub fn supply(&mut self, id: u64, amount: Amount) -> Result<()> {
        if amount == 0 {
            return Err(E7Error::ZeroSupply);
        }
        let account = self.account_mut(id)?;
        account.collateral += amount;
        Ok(())
    }

    /// Withdraw `amount` collateral; refused when it exceeds the
    /// supply or would breach the collateral capacity.
    pub fn withdraw(&mut self, id: u64, amount: Amount) -> Result<()> {
        if amount == 0 {
            return Err(E7Error::ZeroWithdraw);
        }
        let price = self.price;
        let ltv_bps = self.ltv_bps;
        let account = self.account_mut(id)?;
        if amount > account.collateral {
            return Err(E7Error::WithdrawExceeds {
                amount,
                supplied: account.collateral,
            });
        }
        if account.debt > 0 {
            let capacity = max_withdraw(account.collateral, account.debt, price, ltv_bps)
                .ok_or(E7Error::Overflow)?;
            if amount > capacity {
                return Err(E7Error::WithdrawCapacity { amount, capacity });
            }
        }
        account.collateral -= amount;
        Ok(())
    }

    /// Borrow `amount` stable units against the collateral (LTV cap).
    pub fn borrow(&mut self, id: u64, amount: Amount) -> Result<()> {
        if amount == 0 {
            return Err(E7Error::ZeroBorrow);
        }
        let price = self.price;
        let ltv_bps = self.ltv_bps;
        let account = self.account_mut(id)?;
        let value = mul_div(account.collateral, price, E7_ONE).ok_or(E7Error::Overflow)?;
        let cap_total =
            mul_div(value, u128::from(ltv_bps), u128::from(E7_BPS)).ok_or(E7Error::Overflow)?;
        let capacity = cap_total.saturating_sub(account.debt);
        if amount > capacity {
            return Err(E7Error::BorrowCapacity { amount, capacity });
        }
        account.debt += amount;
        Ok(())
    }

    /// Repay `amount` of the debt.
    pub fn repay(&mut self, id: u64, amount: Amount) -> Result<()> {
        if amount == 0 {
            return Err(E7Error::ZeroRepay);
        }
        let account = self.account_mut(id)?;
        if amount > account.debt {
            return Err(E7Error::RepayExceeds {
                repay: amount,
                debt: account.debt,
            });
        }
        account.debt -= amount;
        Ok(())
    }

    /// Peg stress driver: update the collateral oracle price.
    pub fn set_price(&mut self, price: Price) -> Result<()> {
        if price == 0 {
            return Err(E7Error::ZeroPrice);
        }
        self.price = price;
        Ok(())
    }

    /// Accrue one day of interest on every debt at the current kinked
    /// borrow rate (floor per account per day). Returns the interest
    /// added today.
    pub fn advance_day(&mut self) -> Result<Amount> {
        let rate = u128::from(self.rate_bps());
        let mut added: Amount = 0;
        for account in &mut self.accounts {
            let interest =
                mul_div(account.debt, rate, 365 * u128::from(E7_BPS)).ok_or(E7Error::Overflow)?;
            account.debt += interest;
            added = added.saturating_add(interest);
        }
        self.days += 1;
        self.total_interest = self.total_interest.saturating_add(added);
        Ok(added)
    }

    /// Liquidate an unhealthy account (health below
    /// [`E7_LIQ_HEALTH_BPS`]): the liquidator repays up to the close
    /// factor and seizes collateral worth `repaid · (1 + bonus)` at the
    /// current price. Refuses healthy accounts.
    pub fn liquidate(&mut self, id: u64) -> Result<Liquidation> {
        let price = self.price;
        let bonus_bps = self.bonus_bps;
        let close_bps = self.close_bps;
        let min_bps = self.liq_health_bps;
        let account = self.account_mut(id)?;
        let health = health_bps_of(account.collateral, account.debt, price);
        if account.debt == 0 || health.unwrap_or(u64::MAX) >= min_bps {
            return Err(E7Error::HealthyAccount {
                health_bps: health.unwrap_or(u64::MAX),
                min_bps,
            });
        }
        let repaid = mul_div(account.debt, u128::from(close_bps), u128::from(E7_BPS))
            .ok_or(E7Error::Overflow)?;
        // Single floor: seized = repaid · (BPS + bonus) · ONE / (price · BPS).
        let seized = mul_div(
            repaid,
            u128::from(E7_BPS + bonus_bps) * E7_ONE,
            price * u128::from(E7_BPS),
        )
        .ok_or(E7Error::Overflow)?;
        let seized = seized.min(account.collateral);
        account.debt -= repaid;
        account.collateral -= seized;
        Ok(Liquidation { repaid, seized })
    }
}

impl Default for LendingMarket {
    fn default() -> Self {
        Self::new()
    }
}

/// Floor health in bps: `collateral · price · 10000 / (ONE · debt)`.
fn health_bps_of(collateral: Amount, debt: Amount, price: Price) -> Option<u64> {
    if debt == 0 {
        return None;
    }
    let value = mul_div(collateral, price, E7_ONE)?;
    let health = mul_div(value, u128::from(E7_BPS), debt)?;
    u64::try_from(health).ok()
}

/// Maximum withdrawable collateral keeping `floor(floor((collateral −
/// w) · price / ONE) · LTV / BPS) >= debt`: `collateral − xmin` with
/// `vmin = ceil(debt · BPS / LTV)` and `xmin = ceil(vmin · ONE / price)`
/// (256-bit intermediates).
fn max_withdraw(collateral: Amount, debt: Amount, price: Price, ltv_bps: u64) -> Option<Amount> {
    let vmin = mul_div_ceil(debt, u128::from(E7_BPS), u128::from(ltv_bps))?;
    let xmin = mul_div_ceil(vmin, E7_ONE, price)?;
    Some(collateral.saturating_sub(xmin.min(collateral)))
}

/// `floor(a * b / d)` plus one when the division has a remainder
/// (256-bit intermediate, checked).
fn mul_div_ceil(a: Amount, b: Amount, d: Amount) -> Option<Amount> {
    if d == 0 {
        return None;
    }
    let q = mul_div(a, b, d)?;
    // Remainder exists iff the product differs from q * d exactly.
    if crate::amm::wide_mul(a, b) == crate::amm::wide_mul(q, d) {
        Some(q)
    } else {
        q.checked_add(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_model_boundaries() {
        assert_eq!(rate_bps_for(0), 200);
        assert_eq!(rate_bps_for(6_666), 1_033);
        assert_eq!(rate_bps_for(8_000), 1_200);
        assert_eq!(rate_bps_for(9_000), 2_700);
    }

    #[test]
    fn borrow_capacity_golden() {
        let mut m = LendingMarket::new();
        let a = m.open_account();
        m.supply(a, 1_000 * E7_ONE).unwrap();
        m.borrow(a, 700 * E7_ONE).unwrap();
        assert_eq!(m.borrow_capacity(a), Some(50 * E7_ONE));
        let err = m.borrow(a, 50 * E7_ONE + 1).unwrap_err();
        assert_eq!(
            err.to_string(),
            format!(
                "E7: borrow exceeds collateral capacity ({}/{})",
                50 * E7_ONE + 1,
                50 * E7_ONE
            )
        );
    }

    #[test]
    fn withdraw_capacity_golden() {
        let mut m = LendingMarket::new();
        let a = m.open_account();
        m.supply(a, 1_000 * E7_ONE).unwrap();
        m.borrow(a, 700 * E7_ONE).unwrap();
        // ceil(700e18*10000/7500) = 933333333333333333334 -> max_wd 66666666666666666666
        let err = m.withdraw(a, 66_666_666_666_666_666_667).unwrap_err();
        assert_eq!(
            err.to_string(),
            format!(
                "E7: withdraw would breach collateral capacity ({}/{})",
                66_666_666_666_666_666_667u128, 66_666_666_666_666_666_666u128
            )
        );
        m.withdraw(a, 66_666_666_666_666_666_666).unwrap();
        assert_eq!(m.health_bps(a), Some(13_333));
    }

    #[test]
    fn healthy_liquidation_refused() {
        let mut m = LendingMarket::new();
        let a = m.open_account();
        m.supply(a, 1_000 * E7_ONE).unwrap();
        m.borrow(a, 700 * E7_ONE).unwrap();
        let err = m.liquidate(a).unwrap_err();
        assert_eq!(
            err.to_string(),
            "E7: account is healthy, liquidation refused (14285 >= 12500)"
        );
    }
}
