//! Exact rational numbers (`num / den`, `den > 0`), because the engine is
//! float-free (`#![deny(clippy::float_arithmetic)]`) and Safe Mode decisions
//! sit exactly on zone boundaries such as CR = 1.35 / 1.20 / 1.00.
//!
//! Ratios are stored unnormalized: `1.35` may arrive as `(135, 100)` or
//! `(27, 20)`; equality and ordering are cross-multiplied with checked
//! arithmetic, so no normalization precision is ever lost or invented.

use crate::error::{EligibilityError, EligibilityResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Ratio {
    pub num: u128,
    pub den: u128,
}

impl Ratio {
    pub const fn new(num: u128, den: u128) -> Self {
        Self { num, den }
    }

    pub const ZERO: Ratio = Ratio { num: 0, den: 1 };
    pub const ONE: Ratio = Ratio { num: 1, den: 1 };

    /// `den == 0` is not a number; callers fail closed on it.
    pub const fn is_valid(&self) -> bool {
        self.den != 0
    }

    fn cmul(a: u128, b: u128) -> EligibilityResult<u128> {
        a.checked_mul(b).ok_or(EligibilityError::MathOverflow)
    }

    /// Exact comparison without normalization: `self < other` iff
    /// `self.num * other.den < other.num * self.den`.
    pub fn lt(&self, other: &Ratio) -> EligibilityResult<bool> {
        debug_assert!(self.is_valid() && other.is_valid());
        Ok(Self::cmul(self.num, other.den)? < Self::cmul(other.num, self.den)?)
    }

    pub fn le(&self, other: &Ratio) -> EligibilityResult<bool> {
        Ok(Self::cmul(self.num, other.den)? <= Self::cmul(other.num, self.den)?)
    }

    pub fn gt(&self, other: &Ratio) -> EligibilityResult<bool> {
        other.lt(self)
    }

    pub fn ge(&self, other: &Ratio) -> EligibilityResult<bool> {
        other.le(self)
    }

    /// The smaller of two ratios.
    pub fn min(&self, other: &Ratio) -> EligibilityResult<Ratio> {
        if self.le(other)? {
            Ok(*self)
        } else {
            Ok(*other)
        }
    }

    /// `self - 1`, used by `leverage = CR / (CR - 1)`; `None` when `self <= 1`
    /// (the subtraction would be zero or negative: CR = 1 is singular and
    /// CR < 1 has no valid leverage, section 9 / test 38).
    pub fn minus_one(&self) -> Option<Ratio> {
        if self.den == 0 || self.num <= self.den {
            return None;
        }
        Some(Ratio {
            num: self.num - self.den,
            den: self.den,
        })
    }

    /// `self / other`, `None` when `other` is zero.
    pub fn div(&self, other: &Ratio) -> EligibilityResult<Option<Ratio>> {
        if other.num == 0 || other.den == 0 || self.den == 0 {
            return Ok(None);
        }
        let num = Self::cmul(self.num, other.den)?;
        let den = Self::cmul(self.den, other.num)?;
        Ok(Some(Ratio { num, den }))
    }

    /// `leverage = CR / (CR - 1)` — `Some` only for CR > 1; never a finite
    /// value at CR = 1 or below (test 38).
    pub fn leverage(&self) -> EligibilityResult<Option<Ratio>> {
        match self.minus_one() {
            None => Ok(None),
            Some(d) => self.div(&d),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_boundaries_compare_without_normalization() {
        let a = Ratio::new(135, 100);
        let b = Ratio::new(27, 20); // same value, unnormalized
        assert!(!a.lt(&b).unwrap() && !b.lt(&a).unwrap());
        let just_below = Ratio::new(1_349, 1_000);
        assert!(just_below.lt(&a).unwrap());
        assert!(a.ge(&Ratio::ONE).unwrap());
    }

    #[test]
    fn leverage_is_none_at_and_below_one() {
        assert!(Ratio::ONE.leverage().unwrap().is_none());
        assert!(Ratio::new(99, 100).leverage().unwrap().is_none());
        assert!(Ratio::ZERO.leverage().unwrap().is_none());
        let l = Ratio::new(27, 20).leverage().unwrap().unwrap(); // CR 1.35
        assert!(l.le(&Ratio::new(27, 7)).unwrap() && Ratio::new(27, 7).le(&l).unwrap());
    }

    #[test]
    fn overflow_fails_closed() {
        let huge = Ratio::new(u128::MAX, 1);
        let other = Ratio::new(u128::MAX - 1, u128::MAX);
        assert_eq!(huge.lt(&other), Err(EligibilityError::MathOverflow));
    }
}
