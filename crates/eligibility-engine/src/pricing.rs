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
/// tests. The single-spot prohibition is **structural, not calibrated**
/// (architecture section 5): `min_distinct_slots >= 2` and
/// `min_window_seconds >= 1` are hard floors on the configuration itself
/// (see [`ReferenceValidation::validate`]) — no calibration can admit a
/// window that lives in one slot or one instant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReferenceValidation {
    /// Window must span at least this many distinct finalized slots
    /// (floor: 2 — a single spot sample is prohibited unconditionally).
    pub min_distinct_slots: u64,
    /// Window must span at least this many seconds between first and last
    /// (floor: 1 — a zero-duration window is prohibited unconditionally).
    pub min_window_seconds: i64,
    /// Maximum tolerated spread `(hi - lo) / lo` across the window.
    pub max_spread: Ratio,
    /// Maximum tolerated oracle confidence bound relative to the mid.
    pub max_confidence: Ratio,
}

impl ReferenceValidation {
    /// The calibration itself must respect the structural floors: a
    /// configuration that would permit single-slot or zero-duration
    /// windows is invalid outright, whatever window it is later handed.
    pub fn validate(&self) -> EligibilityResult<()> {
        use EligibilityError::InvalidPriceWindow as Rej;
        if self.min_distinct_slots < 2 {
            return Err(Rej(
                "min_distinct_slots must be >= 2 (single spot prohibited)",
            ));
        }
        if self.min_window_seconds < 1 {
            return Err(Rej(
                "min_window_seconds must be >= 1 (zero-duration window prohibited)",
            ));
        }
        Ok(())
    }

    /// Rejection reason, so callers can assert *why* a window was refused.
    pub fn check(&self, samples: &[PriceSample]) -> EligibilityResult<()> {
        use EligibilityError::InvalidPriceWindow as Rej;
        self.validate()?;
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
                // Confidence is a same-unit half-width; the tolerated
                // bound is RELATIVE to this sample's mid price:
                // (c_num/c_den) / (num/den) <= max_confidence, compared
                // exactly by cross-multiplication. A zero mid leaves the
                // ratio undefined: fail closed.
                let rel_num = c_num
                    .checked_mul(s.units_den)
                    .ok_or(EligibilityError::MathOverflow)?;
                let rel_den = c_den
                    .checked_mul(s.lamports_num)
                    .ok_or(EligibilityError::MathOverflow)?;
                if rel_den == 0 || Ratio::new(rel_num, rel_den).gt(&self.max_confidence)? {
                    return Err(Rej("confidence too wide"));
                }
            }
        }
        if (slots.len() as u64) < self.min_distinct_slots {
            return Err(Rej("not enough distinct slots"));
        }
        // Structural backstop (unreachable through `validate()`): the
        // single-spot prohibition holds even if a calibration bypasses
        // validation — one distinct slot is always refused.
        if slots.len() < 2 {
            return Err(Rej("single-slot window"));
        }
        let first = samples.iter().map(|s| s.unix_timestamp).min().unwrap();
        let last = samples.iter().map(|s| s.unix_timestamp).max().unwrap();
        if last.saturating_sub(first) < self.min_window_seconds {
            return Err(Rej("window too short"));
        }
        // Structural backstop: a zero-duration window is always refused.
        if last.saturating_sub(first) < 1 {
            return Err(Rej("zero-duration window"));
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

/// Greatest common divisor (Euclidean). Used to keep the exact-rational
/// accumulator in canonical reduced form.
fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// Exact rational addition `a + b` (denominators nonzero), reduced to
/// canonical form so accumulation cannot blow up and the result depends
/// only on the mathematical value, never the summation order.
fn add_ratios(a: (u128, u128), b: (u128, u128)) -> EligibilityResult<(u128, u128)> {
    let num =
        a.0.checked_mul(b.1)
            .and_then(|x| x.checked_add(b.0.checked_mul(a.1)?))
            .ok_or(EligibilityError::MathOverflow)?;
    let den = a.1.checked_mul(b.1).ok_or(EligibilityError::MathOverflow)?;
    let g = gcd(num, den);
    Ok((num / g, den / g))
}

/// Commit a reference price from a validated sample window: the exact
/// time-weighted average (each sample held from its timestamp to the next;
/// the last sample held to `window_end`), computed as ONE exact rational
/// `sum(dt_i * num_i/den_i) / sum(dt_i)` — never an average of encoded
/// numerators and denominators, which is wrong whenever the sample
/// denominators differ. Rejects any window failing
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
    let mut acc = (0u128, 1u128); // exact sum of dt_i * num_i/den_i
    let mut total_dt: u128 = 0;
    fn push(
        acc: &mut (u128, u128),
        total_dt: &mut u128,
        s: &PriceSample,
        dt: u128,
    ) -> EligibilityResult<()> {
        let term_num = dt
            .checked_mul(s.lamports_num)
            .ok_or(EligibilityError::MathOverflow)?;
        *acc = add_ratios(*acc, (term_num, s.units_den))?;
        *total_dt = total_dt
            .checked_add(dt)
            .ok_or(EligibilityError::MathOverflow)?;
        Ok(())
    }
    for w in sorted.windows(2) {
        let dt = u128::try_from(w[1].unix_timestamp.saturating_sub(w[0].unix_timestamp))
            .map_err(|_| EligibilityError::MathOverflow)?;
        push(&mut acc, &mut total_dt, w[0], dt)?;
    }
    let last = *sorted.last().expect("checked non-empty");
    let tail = u128::try_from(window_end.saturating_sub(last.unix_timestamp).max(0))
        .map_err(|_| EligibilityError::MathOverflow)?;
    push(&mut acc, &mut total_dt, last, tail)?;
    if total_dt == 0 {
        return Err(EligibilityError::InvalidPriceWindow("zero-weight window"));
    }
    // acc / total_dt, reduced to canonical form.
    let num = acc.0;
    let den = acc
        .1
        .checked_mul(total_dt)
        .ok_or(EligibilityError::MathOverflow)?;
    let g = gcd(num, den);
    Ok(UnitPrice {
        lamports_num: num / g,
        units_den: den / g,
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
        // (200*60 + 205*30 + 210*10) / (100*100) = 2.025 = 81/40 exactly
        // (the accumulator reduces to canonical form after every step).
        let p = commit_reference(&samples, 200, &validation(), PriceBasis::Reference).unwrap();
        assert_eq!((p.lamports_num, p.units_den), (81, 40));
        assert_eq!(p.value(1000).unwrap(), 2_025);
    }

    /// ADD-A: exact-rational TWAP with UNEQUAL sample denominators. Averaging
    /// encoded numerators and denominators independently is wrong unless all
    /// denominators share a scale: 2.00 held 60s (200/100), 2.05 held 30s
    /// (41/20), 2.10 held 10s (210/100) must give exactly 2.025 = 81/40 —
    /// the old accumulator produced 15330/7600 ≈ 2.0171.
    #[test]
    fn twap_is_exact_when_sample_denominators_differ() {
        let samples = [
            sample(1, 100, 200, 100),
            sample(2, 160, 41, 20), // same 2.05, different denominator
            sample(3, 190, 210, 100),
        ];
        let p = commit_reference(&samples, 200, &validation(), PriceBasis::Reference).unwrap();
        assert_eq!((p.lamports_num, p.units_den), (81, 40));
    }

    /// ADD-A: the committed price is canonical — equal windows expressed
    /// with different (equal-value) encodings commit the identical pair.
    #[test]
    fn twap_representation_canonicalizes() {
        let a = [
            sample(1, 100, 2, 1),
            sample(2, 160, 41, 20),
            sample(3, 190, 21, 10),
        ];
        let b = [
            sample(1, 100, 200, 100),
            sample(2, 160, 410, 200),
            sample(3, 190, 210, 100),
        ];
        let pa = commit_reference(&a, 200, &validation(), PriceBasis::Reference).unwrap();
        let pb = commit_reference(&b, 200, &validation(), PriceBasis::Reference).unwrap();
        assert_eq!(
            (pa.lamports_num, pa.units_den),
            (pb.lamports_num, pb.units_den)
        );
        assert_eq!((pa.lamports_num, pa.units_den), (81, 40));
    }

    /// ADD-A: overflow anywhere in the exact accumulation is an error,
    /// never a wrapped or approximated price.
    #[test]
    fn twap_overflow_fails_closed() {
        let samples = [
            sample(1, 100, u128::MAX, u128::MAX - 1),
            sample(2, 160, u128::MAX, u128::MAX - 1),
            sample(3, 190, u128::MAX, u128::MAX - 1),
        ];
        assert_eq!(
            commit_reference(&samples, 200, &validation(), PriceBasis::Reference),
            Err(EligibilityError::MathOverflow)
        );
    }

    /// ADD-B: confidence is a same-unit half-width; the tolerated bound is
    /// RELATIVE to the sample's mid price. Comparing the raw confidence
    /// ratio against the relative bound is dimensionally wrong in both
    /// directions.
    #[test]
    fn oracle_confidence_is_relative_to_the_mid_price() {
        let s = |slot, ts, num, den, conf| PriceSample {
            slot,
            unix_timestamp: ts,
            lamports_num: num,
            units_den: den,
            confidence: conf,
        };
        // A large-looking absolute band on a large price is NARROW
        // relative to the mid (1e5 / 1e7 = 1% <= 2%): admissible — the
        // old absolute comparison wrongly rejected it.
        let big_price = [
            s(1, 100, 10_000_000, 1, Some((100_000, 1))),
            s(2, 130, 10_000_000, 1, None),
            s(3, 160, 10_000_000, 1, None),
        ];
        assert!(commit_reference(&big_price, 200, &validation(), PriceBasis::Reference).is_ok());

        // A small-looking absolute band on a small price is WIDE relative
        // (0.01 / 0.2 = 5% > 2%): rejected — the old comparison admitted it.
        let small_price = [
            s(1, 100, 1, 5, Some((1, 100))),
            s(2, 130, 1, 5, None),
            s(3, 160, 1, 5, None),
        ];
        assert_eq!(
            commit_reference(&small_price, 200, &validation(), PriceBasis::Reference),
            Err(EligibilityError::InvalidPriceWindow("confidence too wide"))
        );

        // Exactly at the bound (relative 1/50 == 1/50): admitted, by exact
        // rational comparison (no float boundary drift).
        let exact = [
            s(1, 100, 5, 1, Some((1, 10))),
            s(2, 130, 5, 1, None),
            s(3, 160, 5, 1, None),
        ];
        assert!(commit_reference(&exact, 200, &validation(), PriceBasis::Reference).is_ok());

        // Different scales/exponents: mid 3500/7 = 500; conf 70/13 is
        // (70·7)/(13·3500) ≈ 1.08% (admissible), conf 700/13 ≈ 10.8%
        // (rejected).
        let scaled_ok = [
            s(1, 100, 3_500, 7, Some((70, 13))),
            s(2, 130, 3_500, 7, None),
            s(3, 160, 3_500, 7, None),
        ];
        assert!(commit_reference(&scaled_ok, 200, &validation(), PriceBasis::Reference).is_ok());
        let scaled_wide = [
            s(1, 100, 3_500, 7, Some((700, 13))),
            s(2, 130, 3_500, 7, None),
            s(3, 160, 3_500, 7, None),
        ];
        assert_eq!(
            commit_reference(&scaled_wide, 200, &validation(), PriceBasis::Reference),
            Err(EligibilityError::InvalidPriceWindow("confidence too wide"))
        );

        // A zero mid makes the relative bound undefined: fail closed.
        let zero_mid = [
            s(1, 100, 0, 1, Some((1, 10))),
            s(2, 130, 0, 1, None),
            s(3, 160, 0, 1, None),
        ];
        assert!(commit_reference(&zero_mid, 200, &validation(), PriceBasis::Reference).is_err());

        // Overflow in the dimensional normalization is an error, never a
        // silent verdict.
        let huge = [
            s(1, 100, 1, u128::MAX, Some((u128::MAX, 1))),
            s(2, 130, 1, u128::MAX, None),
            s(3, 160, 1, u128::MAX, None),
        ];
        assert_eq!(
            commit_reference(&huge, 200, &validation(), PriceBasis::Reference),
            Err(EligibilityError::MathOverflow)
        );
    }

    /// ADD-C: the single-spot prohibition is structural. A calibration
    /// that would admit one-slot or zero-duration windows is itself
    /// invalid — no configuration can weaken the prohibition — while the
    /// weakest legitimate calibration still accepts a genuine multi-slot,
    /// multi-second window.
    #[test]
    fn single_spot_prohibition_cannot_be_configured_away() {
        // Reproduction: under {1 slot, 0 seconds} a LONE spot sample
        // committed a reference — the prohibition was pure configuration.
        let mut weak = validation();
        weak.min_distinct_slots = 1;
        weak.min_window_seconds = 0;
        assert!(weak.validate().is_err());
        let lone = [sample(7, 1_000, 5, 1)];
        assert!(
            commit_reference(&lone, 1_060, &weak, PriceBasis::Reference).is_err(),
            "a single spot sample must never commit, whatever the calibration"
        );
        // Even a healthy multi-slot window is refused under a calibration
        // that would permit single-spot windows: the calibration itself is
        // refused, not just the weak window.
        let healthy = [
            sample(1, 100, 2, 1),
            sample(2, 130, 2, 1),
            sample(3, 160, 2, 1),
        ];
        assert!(commit_reference(&healthy, 200, &weak, PriceBasis::Reference).is_err());
        let mut zero_window = validation();
        zero_window.min_window_seconds = 0;
        assert!(zero_window.validate().is_err());
        assert!(commit_reference(&healthy, 200, &zero_window, PriceBasis::Reference).is_err());

        // The weakest LEGITIMATE calibration (the test seam) still accepts
        // a genuine two-slot, one-second window: the prohibition is not a
        // blanket rejection.
        let minimal = ReferenceValidation {
            min_distinct_slots: 2,
            min_window_seconds: 1,
            max_spread: Ratio::new(1, 20),
            max_confidence: Ratio::new(1, 50),
        };
        assert!(minimal.validate().is_ok());
        let genuine = [sample(1, 1_000, 5, 1), sample(2, 1_001, 5, 1)];
        assert!(commit_reference(&genuine, 1_002, &minimal, PriceBasis::Reference).is_ok());
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
