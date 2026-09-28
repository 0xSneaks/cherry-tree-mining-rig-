//! Reference-price commitment (architecture section 5, tests 30–32).
//!
//! Each admitted asset's epoch reference price is committed **before** the
//! epoch starts, from a manipulation-resistant source: an oracle with a
//! confidence bound, or a time-weighted average over a window. A single spot
//! sample is prohibited. The validation thresholds (minimum distinct blocks,
//! minimum window span, maximum spread / confidence) are **calibrated per
//! adapter and supplied as configuration** — none of the values in this
//! crate's tests are production values (gate 14.2-D); every constant below is
//! a labelled fixture.
//!
//! In-epoch additions are valued at `min(reference, verified price at the
//! addition event)`; the lot bookkeeping that applies that rule lives in
//! [`crate::lots`], and the observations it consumes enter the timeline as
//! `EventKind::PriceObserved` events that already passed this interface.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{EligibilityError, EligibilityResult};
use crate::rational::Ratio;
use crate::valuation::{PriceBasis, PriceKey, UnitPrice};

/// One candidate sample of a manipulation-resistant source at one block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PriceSample {
    pub slot: u64,
    pub unix_timestamp: i64,
    /// Mid price as `lamports_num / units_den` lamports per base unit.
    pub lamports_num: u128,
    pub units_den: u128,
    /// Oracle confidence half-width around the mid, `(num, den)` in the same
    /// units; `None` when the source publishes no confidence bound.
    pub confidence: Option<(u128, u128)>,
}

/// Per-adapter validation configuration. Calibrated externally; fixtures in
/// tests. `min_distinct_slots` and `min_window_seconds` reject single-block
/// (one-slot) windows outright — a manipulated sample set that lives inside
/// one block can never commit a reference (test 31).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReferenceValidation {
    /// Window must span at least this many distinct finalized slots.
    pub min_distinct_slots: u64,
    /// Window must span at least this many seconds between first and last.
    pub min_window_seconds: i64,
    /// Maximum tolerated spread `(hi - lo) / lo` across the window.
    pub max_spread: Ratio,
    /// Maximum tolerated oracle confidence bound relative to the mid.
    pub max_confidence: Ratio,
}

impl ReferenceValidation {
    /// Rejection reason, so callers can assert *why* a window was refused.
    pub fn check(&self, samples: &[PriceSample]) -> EligibilityResult<()> {
        use EligibilityError::InvalidPriceWindow as Rej;
        if samples.is_empty() {
            return Err(Rej("empty"));
        }
        let mut slots = BTreeSet::new();
        let mut lo: Option<Ratio> = None;
        let mut hi: Option<Ratio> = None;
        for s in samples {
            if s.units_den == 0 {
                return Err(Rej("zero denominator"));
            }
            slots.insert(s.slot);
            let price = Ratio::new(s.lamports_num, s.units_den);
            if !price.is_valid() {
                return Err(Rej("zero denominator"));
            }
            lo = Some(match lo {
                None => price,
                Some(l) => l.min(&price)?,
            });
            hi = Some(match hi {
                None => price,
                Some(h) => {
                    if price.gt(&h)? {
                        price
                    } else {
                        h
                    }
                }
            });
            if let Some((c_num, c_den)) = s.confidence {
                if c_den == 0 {
                    return Err(Rej("zero confidence denominator"));
                }
                if Ratio::new(c_num, c_den).gt(&self.max_confidence)? {
                    return Err(Rej("confidence too wide"));
                }
            }
        }
        if (slots.len() as u64) < self.min_distinct_slots {
            return Err(Rej("not enough distinct slots"));
        }
        let first = samples.iter().map(|s| s.unix_timestamp).min().unwrap();
        let last = samples.iter().map(|s| s.unix_timestamp).max().unwrap();
        if last.saturating_sub(first) < self.min_window_seconds {
            return Err(Rej("window too short"));
        }
        // (hi - lo) / lo <= max_spread, cross-multiplied with checked math.
        let (lo, hi) = (lo.unwrap(), hi.unwrap());
        if hi.gt(&lo)? {
            let diff = hi
                .num
                .checked_mul(lo.den)
                .and_then(|x| x.checked_sub(lo.num.checked_mul(hi.den)?))
                .ok_or(EligibilityError::MathOverflow)?;
            let base = lo
                .num
                .checked_mul(hi.den)
                .ok_or(EligibilityError::MathOverflow)?;
            if Ratio::new(diff, base).gt(&self.max_spread)? {
                return Err(Rej("spread too wide"));
            }
        }
        Ok(())
    }
}

/// Commit a reference price from a validated sample window: the exact
/// time-weighted average (each sample held from its timestamp to the next;
/// the last sample held to `window_end`). Rejects any window failing
/// [`ReferenceValidation::check`] — including every single-block
/// manipulation attempt (test 31).
pub fn commit_reference(
    samples: &[PriceSample],
    window_end: i64,
    validation: &ReferenceValidation,
    basis: PriceBasis,
) -> EligibilityResult<UnitPrice> {
    validation.check(samples)?;
    let mut sorted: Vec<&PriceSample> = samples.iter().collect();
    sorted.sort_by_key(|s| (s.unix_timestamp, s.slot));
    let mut acc_num: u128 = 0; // sum(dt_i * num_i)
    let mut acc_den: u128 = 0; // sum(dt_i * den_i)
    fn push(
        acc_num: &mut u128,
        acc_den: &mut u128,
        s: &PriceSample,
        dt: u128,
    ) -> EligibilityResult<()> {
        *acc_num = acc_num
            .checked_add(
                dt.checked_mul(s.lamports_num)
                    .ok_or(EligibilityError::MathOverflow)?,
            )
            .ok_or(EligibilityError::MathOverflow)?;
        *acc_den = acc_den
            .checked_add(
                dt.checked_mul(s.units_den)
                    .ok_or(EligibilityError::MathOverflow)?,
            )
            .ok_or(EligibilityError::MathOverflow)?;
        Ok(())
    }
    for w in sorted.windows(2) {
        let dt = u128::try_from(w[1].unix_timestamp.saturating_sub(w[0].unix_timestamp))
            .map_err(|_| EligibilityError::MathOverflow)?;
        push(&mut acc_num, &mut acc_den, w[0], dt)?;
    }
    let last = *sorted.last().expect("checked non-empty");
    let tail = u128::try_from(window_end.saturating_sub(last.unix_timestamp).max(0))
        .map_err(|_| EligibilityError::MathOverflow)?;
    push(&mut acc_num, &mut acc_den, last, tail)?;
    if acc_den == 0 {
        return Err(EligibilityError::InvalidPriceWindow("zero-weight window"));
    }
    Ok(UnitPrice {
        lamports_num: acc_num,
        units_den: acc_den,
        basis,
    })
}

/// One mint's committed epoch reference: the price and the epoch start it
/// was committed *before*. A commitment after the epoch start is not a
/// reference (the asset is then inactive for that epoch, test 32).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReferenceCommit {
    pub key: PriceKey,
    pub price: UnitPrice,
    /// On-chain finalized time at which the commitment landed.
    pub committed_at: i64,
    /// Half-open epoch start: units held at `t <= epoch_start` are
    /// reference-priced; units acquired at `t > epoch_start` follow the
    /// min rule (test 30).
    pub epoch_start: i64,
}

impl ReferenceCommit {
    pub fn new(
        key: PriceKey,
        price: UnitPrice,
        committed_at: i64,
        epoch_start: i64,
    ) -> EligibilityResult<Self> {
        if price.units_den == 0 {
            return Err(EligibilityError::InvalidAdapter);
        }
        if committed_at >= epoch_start {
            return Err(EligibilityError::LateReferenceCommit);
        }
        Ok(Self {
            key,
            price,
            committed_at,
            epoch_start,
        })
    }
}

/// The committed references in force for one settlement window. Pass #4's
/// price input, now with commitment times attached. SOL is always 1:1
/// (never lot-tracked, never epoch-bound).
#[derive(Clone, Debug, Default)]
pub struct EpochReferences {
    commits: BTreeMap<PriceKey, ReferenceCommit>,
}

impl EpochReferences {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, commit: ReferenceCommit) -> Self {
        self.commits.insert(commit.key, commit);
        self
    }

    pub fn get(&self, key: &PriceKey) -> Option<ReferenceCommit> {
        match key {
            PriceKey::Sol => Some(ReferenceCommit {
                key: PriceKey::Sol,
                price: UnitPrice {
                    lamports_num: 1,
                    units_den: 1,
                    basis: PriceBasis::Reference,
                },
                committed_at: i64::MIN,
                epoch_start: i64::MAX,
            }),
            k => self.commits.get(k).copied(),
        }
    }

    pub fn len(&self) -> usize {
        self.commits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commits.is_empty()
    }
}

/// `min(a, b)` over unit prices by exact rational comparison. The addition
/// rule of section 5: a post-reference unit is never valued above either the
/// reference or its verified addition price.
pub fn min_price(a: &UnitPrice, b: &UnitPrice) -> EligibilityResult<UnitPrice> {
    if a.units_den == 0 || b.units_den == 0 {
        return Err(EligibilityError::InvalidAdapter);
    }
    if Ratio::new(a.lamports_num, a.units_den).le(&Ratio::new(b.lamports_num, b.units_den))? {
        Ok(*a)
    } else {
        Ok(*b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Addr;

    const MINT: Addr = Addr::fixture(0x11);

    fn sample(slot: u64, ts: i64, num: u128, den: u128) -> PriceSample {
        PriceSample {
            slot,
            unix_timestamp: ts,
            lamports_num: num,
            units_den: den,
            confidence: None,
        }
    }

    /// Fixture calibration — NOT a production threshold.
    fn validation() -> ReferenceValidation {
        ReferenceValidation {
            min_distinct_slots: 3,
            min_window_seconds: 60,
            max_spread: Ratio::new(1, 20), // 5%
            max_confidence: Ratio::new(1, 50),
        }
    }

    #[test]
    fn twap_weights_each_sample_by_its_holding_time() {
        let samples = [
            sample(1, 100, 200, 100), // 2.00, held 100..160
            sample(2, 160, 205, 100), // 2.05, held 160..190
            sample(3, 190, 210, 100), // 2.10, held 190..200
        ];
        // (200*60 + 205*30 + 210*10) / (100*100) = 20250/10000
        let p = commit_reference(&samples, 200, &validation(), PriceBasis::Reference).unwrap();
        assert_eq!((p.lamports_num, p.units_den), (20_250, 10_000));
    }

    #[test]
    fn single_block_window_is_rejected() {
        let one_block = [
            sample(7, 1_000, 5, 1),
            sample(7, 1_000, 9, 1),
            sample(7, 1_000, 6, 1),
        ];
        assert_eq!(
            commit_reference(&one_block, 1_000, &validation(), PriceBasis::Reference),
            Err(EligibilityError::InvalidPriceWindow(
                "not enough distinct slots"
            ))
        );
        // Even a lone honest-looking spot sample is prohibited.
        let lone = [sample(7, 1_000, 5, 1)];
        assert!(commit_reference(&lone, 1_060, &validation(), PriceBasis::Reference).is_err());
    }

    #[test]
    fn manipulated_outlier_is_rejected_by_spread_or_confidence() {
        let manipulated = [
            sample(1, 100, 2, 1),
            sample(2, 130, 2, 1),
            sample(3, 160, 2, 1),
            sample(4, 190, 90, 1), // one-block push
        ];
        assert_eq!(
            commit_reference(&manipulated, 200, &validation(), PriceBasis::Reference),
            Err(EligibilityError::InvalidPriceWindow("spread too wide"))
        );
        let wide_conf = [
            PriceSample {
                slot: 1,
                unix_timestamp: 100,
                lamports_num: 2,
                units_den: 1,
                confidence: Some((1, 10)), // 50% of mid — above the fixture bound
            },
            sample(2, 130, 2, 1),
            sample(3, 160, 2, 1),
        ];
        assert_eq!(
            commit_reference(&wide_conf, 200, &validation(), PriceBasis::Reference),
            Err(EligibilityError::InvalidPriceWindow("confidence too wide"))
        );
    }

    #[test]
    fn late_commitment_is_not_a_reference() {
        let p = UnitPrice {
            lamports_num: 2,
            units_den: 1,
            basis: PriceBasis::Reference,
        };
        assert_eq!(
            ReferenceCommit::new(PriceKey::Mint(MINT), p, 500, 500).unwrap_err(),
            EligibilityError::LateReferenceCommit
        );
        assert!(ReferenceCommit::new(PriceKey::Mint(MINT), p, 499, 500).is_ok());
    }
}
