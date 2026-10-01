//! Lock Season off-chain allocation (architecture §8; Pass #7 P7-E).
//!
//! Two replay-derived computations feed the on-chain lock tally and the
//! lock roots:
//!
//! 1. **Weights at `t_s`** ([`lock_snapshot`]): for each active registration,
//!    `ell_i = min(requested_i, w_i(t_s))` with `w_i` the recognized weight
//!    `1[S >= 150] min(K, X)` of the Season's replay at `t_s`. The poster
//!    posts exactly these values (`post_lock_weights`). If the `t_s`
//!    snapshot is not fully verified — an unreconciled segment, a PENDING
//!    registrant or a missing seed record — the result is
//!    [`LockSnapshot::Pending`]: nothing is posted and all of `B_lock` stays
//!    reserved (test 53, P7-L1).
//! 2. **Vesting at maturity** ([`lock_vesting`]): each wallet's surviving
//!    weight is the nonincreasing low-water mark of its verified weight over
//!    `[t_s, t_end)`, bounded by `ell_i`; restored weight never resurrects
//!    lost units (test 50). The vested leaf is
//!    `floor(A_i * ell_surv / ell_i)` and the voluntary shortfall
//!    `A_i - vested` is `forfeited` (tests 88/89). Unknown or involuntary
//!    history (unreconciled segments, PENDING, INELIGIBLE by asset
//!    configuration) is never treated as voluntary exit: the wallet is
//!    held and the Season stays [`LockVesting::Pending`] until the replay
//!    resolves it (or terminal resolution, §10).
//!
//! Awards `A_i` are read from the chain (each `LockRegistration.award` of
//! the Final generation); [`lock_award`] mirrors the on-chain formula so the
//! operator can check them before relying on them. Rounding dust is never
//! part of `forfeited` (F7): `sum vested + forfeited = sum A_i`.
//!
//! Like [`crate::settle_epoch`], the public entry points take only a
//! replayed [`ReplayReport`] and the seed timeline; the kernels over raw
//! segments are crate-private.

use std::collections::BTreeMap;

use crate::error::{EligibilityError, EligibilityResult};
use crate::replay::{Eligibility, IntervalStatus, ReplayReport, Segment};
use crate::settlement::{recognized_weight, SeedTimeline};
use crate::types::Addr;

/// The only lock duration: 5 days (§8), in seconds.
pub const LOCK_DURATION_SECONDS: i64 = 432_000;
/// `Kmax = 2.5 SOL-equivalent`: the largest `ell_i`.
pub const LOCK_KMAX_LAMPORTS: u64 = 2_500_000_000;
/// `Hfloor * 5 days = 62.5 SOL-days` in lamport-days (§8). Same frozen
/// constants as `rigs_engine::fruit_lock`; the crates are independent, so
/// each states its own copy.
pub const LOCK_H_FLOOR_LAMPORT_DAYS: u64 = 62_500_000_000;
const LOCK_DAYS: u64 = 5;

/// One active (non-cancelled) registration, read from the chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LockRequest {
    pub wallet: Addr,
    /// `0 < requested <= Kmax`.
    pub requested: u64,
}

/// `ell_i` for one registration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LockWeight {
    pub wallet: Addr,
    pub requested: u64,
    pub ell: u64,
}

/// The `t_s` snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LockSnapshot {
    /// Every registrant's `t_s` weight is verified: post exactly these.
    Ready(Vec<LockWeight>),
    /// The `t_s` history is not verified for these wallets (or the segment
    /// at `t_s` is unreconciled, listed as all registrants): post nothing.
    Pending(Vec<Addr>),
}

/// One Final award, read from the chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LockAward {
    pub wallet: Addr,
    /// `ell_i` of the Final generation.
    pub ell: u64,
    /// `A_i` of the Final generation.
    pub award: u64,
}

/// One wallet's vesting at maturity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LockVest {
    pub wallet: Addr,
    /// Surviving weight (low-water mark over `[t_s, t_end)`), `<= ell`.
    pub ell_surv: u64,
    /// The cumulative leaf: `floor(A_i * ell_surv / ell_i)`.
    pub vested: u64,
    /// Voluntary shortfall: `A_i - vested`.
    pub forfeited: u64,
}

/// Season settlement at maturity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LockVesting {
    /// Complete: post a complete lock root with `total` and `forfeited`.
    Ready {
        vests: Vec<LockVest>,
        total: u64,
        forfeited: u64,
    },
    /// These wallets have unknown or involuntary history in the Season:
    /// no complete root yet; their awards stay reserved.
    Pending(Vec<Addr>),
}

/// `H_lock = max(5 * sum_ell, 62.5 SOL-days)`.
pub fn lock_h(sum_ell: u64) -> EligibilityResult<u64> {
    sum_ell
        .checked_mul(LOCK_DAYS)
        .map(|u| u.max(LOCK_H_FLOOR_LAMPORT_DAYS))
        .ok_or(EligibilityError::MathOverflow)
}

/// `A_i = floor(B * 5 ell_i / H_lock)` — the on-chain award formula.
pub fn lock_award(budget: u64, ell: u64, h_lock: u64) -> EligibilityResult<u64> {
    if ell == 0 {
        return Ok(0);
    }
    let u = u128::from(ell) * u128::from(LOCK_DAYS);
    if ell > LOCK_KMAX_LAMPORTS || h_lock < LOCK_H_FLOOR_LAMPORT_DAYS || u > u128::from(h_lock) {
        return Err(EligibilityError::InvariantViolation(
            "lock award input out of range",
        ));
    }
    u64::try_from(u128::from(budget) * u / u128::from(h_lock))
        .map_err(|_| EligibilityError::MathOverflow)
}

fn segment_at(segs: &[Segment], t: i64) -> Option<&Segment> {
    segs.iter().find(|s| s.start <= t && t < s.end)
}

/// Verified weight of `wallet` in `seg` with `seeds` seeds, or `None` when
/// it is not verified (pending / involuntary).
fn verified_weight(e: Option<&Eligibility>, seeds: Option<u64>) -> Result<u128, ()> {
    match e {
        // Not in a verified segment's registrant set: known weight 0.
        None => Ok(0),
        Some(Eligibility::Verified { x_lamports, .. }) => match seeds {
            Some(s) => Ok(recognized_weight(s, *x_lamports)),
            None => Err(()),
        },
        Some(Eligibility::Pending(_)) | Some(Eligibility::Ineligible(_)) => Err(()),
    }
}

/// Kernel of [`lock_snapshot`].
pub(crate) fn snapshot_segments(
    segs: &[Segment],
    seeds: &SeedTimeline,
    t_s: i64,
    regs: &[LockRequest],
) -> EligibilityResult<LockSnapshot> {
    let mut seen = BTreeMap::new();
    for r in regs {
        if r.requested == 0 || r.requested > LOCK_KMAX_LAMPORTS {
            return Err(EligibilityError::InvariantViolation("invalid lock request"));
        }
        if seen.insert(r.wallet, ()).is_some() {
            return Err(EligibilityError::InvariantViolation(
                "duplicate lock registration",
            ));
        }
    }
    let all = || regs.iter().map(|r| r.wallet).collect::<Vec<_>>();
    let seg = match segment_at(segs, t_s) {
        Some(s) if s.status == IntervalStatus::Verified => s,
        _ => return Ok(LockSnapshot::Pending(all())),
    };
    let mut out = Vec::with_capacity(regs.len());
    let mut pending = Vec::new();
    for r in regs {
        match verified_weight(seg.wallets.get(&r.wallet), seeds.at(&r.wallet, t_s)) {
            Ok(w) => out.push(LockWeight {
                wallet: r.wallet,
                requested: r.requested,
                ell: u64::try_from(w.min(u128::from(r.requested)))
                    .map_err(|_| EligibilityError::MathOverflow)?,
            }),
            Err(()) => pending.push(r.wallet),
        }
    }
    Ok(if pending.is_empty() {
        LockSnapshot::Ready(out)
    } else {
        LockSnapshot::Pending(pending)
    })
}

/// `ell_i = min(requested_i, w_i(t_s))` for every active registration, from
/// the Season's replay at `t_s` (P7-L1). Pending unless fully verified.
pub fn lock_snapshot(
    report: &ReplayReport,
    seeds: &SeedTimeline,
    t_s: i64,
    regs: &[LockRequest],
) -> EligibilityResult<LockSnapshot> {
    snapshot_segments(report.segments(), seeds, t_s, regs)
}

/// Kernel of [`lock_vesting`].
pub(crate) fn vesting_segments(
    segs: &[Segment],
    seeds: &SeedTimeline,
    t_s: i64,
    awards: &[LockAward],
) -> EligibilityResult<LockVesting> {
    let t_end = t_s
        .checked_add(LOCK_DURATION_SECONDS)
        .ok_or(EligibilityError::MathOverflow)?;
    // The replay must cover the whole Season window.
    let mut cursor = t_s;
    let mut window: Vec<&Segment> = Vec::new();
    for s in segs {
        let (a, b) = (s.start.max(t_s), s.end.min(t_end));
        if a >= b {
            continue;
        }
        if a != cursor {
            return Err(EligibilityError::InvariantViolation(
                "replay does not cover the lock Season",
            ));
        }
        window.push(s);
        cursor = b;
    }
    if cursor != t_end {
        return Err(EligibilityError::InvariantViolation(
            "replay does not cover the lock Season",
        ));
    }
    let mut vests = Vec::with_capacity(awards.len());
    let mut pending = Vec::new();
    for a in awards {
        if (a.ell == 0) != (a.award == 0) || a.ell > LOCK_KMAX_LAMPORTS {
            return Err(EligibilityError::InvariantViolation("invalid lock award"));
        }
        let mut surv = u128::from(a.ell);
        let mut held = false;
        for s in &window {
            if s.status != IntervalStatus::Verified {
                held = true;
                break;
            }
            let (from, to) = (s.start.max(t_s), s.end.min(t_end));
            // The lowest seed count anywhere in the piece.
            let min_seeds = seeds.min_within(&a.wallet, from, to);
            match verified_weight(s.wallets.get(&a.wallet), min_seeds) {
                Ok(w) => surv = surv.min(w),
                Err(()) => {
                    held = true;
                    break;
                }
            }
        }
        if held {
            pending.push(a.wallet);
            continue;
        }
        let ell_surv = u64::try_from(surv).map_err(|_| EligibilityError::MathOverflow)?;
        let vested = if a.ell == 0 {
            0
        } else {
            u64::try_from(u128::from(a.award) * surv / u128::from(a.ell))
                .map_err(|_| EligibilityError::MathOverflow)?
        };
        vests.push(LockVest {
            wallet: a.wallet,
            ell_surv,
            vested,
            forfeited: a.award - vested,
        });
    }
    if !pending.is_empty() {
        return Ok(LockVesting::Pending(pending));
    }
    let total = vests
        .iter()
        .try_fold(0u64, |t, v| t.checked_add(v.vested))
        .ok_or(EligibilityError::MathOverflow)?;
    let forfeited = vests
        .iter()
        .try_fold(0u64, |t, v| t.checked_add(v.forfeited))
        .ok_or(EligibilityError::MathOverflow)?;
    Ok(LockVesting::Ready {
        vests,
        total,
        forfeited,
    })
}

/// Vesting at `t_s + 5 days` from the Season's replay and the chain's Final
/// awards (tests 50 / 88 / 89).
pub fn lock_vesting(
    report: &ReplayReport,
    seeds: &SeedTimeline,
    t_s: i64,
    awards: &[LockAward],
) -> EligibilityResult<LockVesting> {
    vesting_segments(report.segments(), seeds, t_s, awards)
}
