//! [`Amount`] and [`Denom`] — fixed-point native-asset values.

use std::fmt;

/// An amount in the smallest unit (`aalcry`, 10^-18 ALCRY).
///
/// All arithmetic is checked; silent overflow is a consensus bug, not an
/// inconvenience. `u128` is intentional: balances must be exact integers.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct Amount(u128);

impl Amount {
    /// Zero.
    pub const ZERO: Amount = Amount(0);

    /// One smallest unit.
    pub const ONE: Amount = Amount(1);

    /// Largest representable amount.
    pub const MAX: Amount = Amount(u128::MAX);

    /// Construct from a number of smallest units.
    pub const fn from_units(units: u128) -> Self {
        Amount(units)
    }

    /// Read the raw smallest-unit value.
    pub const fn units(self) -> u128 {
        self.0
    }

    /// Construct from whole coins, checking the `10^18` multiplication.
    pub fn from_coins(coins: u128) -> Option<Self> {
        coins.checked_mul(crate::units_per_coin()).map(Amount)
    }

    /// Checked addition.
    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        self.0.checked_add(rhs.0).map(Amount)
    }

    /// Checked subtraction.
    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        self.0.checked_sub(rhs.0).map(Amount)
    }

    /// Checked multiplication by a scalar.
    pub fn checked_mul(self, factor: u128) -> Option<Self> {
        self.0.checked_mul(factor).map(Amount)
    }

    /// Checked division by a scalar; zero divisor returns `None`.
    pub fn checked_div(self, divisor: u128) -> Option<Self> {
        self.0.checked_div(divisor).map(Amount)
    }
}

impl fmt::Display for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.0, crate::SMALLEST_UNIT)
    }
}

impl fmt::Debug for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Amount({})", self.0)
    }
}

/// A denomination name. Only the two native names are canonical at C1.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Denom(&'static str);

impl Denom {
    /// The display denomination, `ALCRY`.
    pub const NATIVE: Denom = Denom(crate::DENOM);

    /// The smallest-unit denomination, `aalcry`.
    pub const SMALLEST: Denom = Denom(crate::SMALLEST_UNIT);

    /// The denomination as a string.
    pub const fn as_str(&self) -> &'static str {
        self.0
    }

    /// Parse a known denomination, rejecting anything else.
    pub fn parse(text: &str) -> Option<Self> {
        if text == crate::DENOM {
            Some(Self::NATIVE)
        } else if text == crate::SMALLEST_UNIT {
            Some(Self::SMALLEST)
        } else {
            None
        }
    }
}

impl fmt::Display for Denom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl fmt::Debug for Denom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Denom({})", self.0)
    }
}
