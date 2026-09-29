# Pass #7 — Lock decision amendment (P7-L1, P7-L2, rounding, F7)

Date: 2026-09-29. Founder decisions, frozen for Pass #7. Narrow amendment to
`CHERRY_TREE_FINAL_TOKENOMICS_ARCHITECTURE.md` (§8, §10a, §12); it resolves
only the conflicts quoted below and changes nothing else. Implemented by
`crates/rigs-engine/src/fruit_lock.rs` (P7-A).

## P7-L1 — Registration cutoff vs allocation finalization

Conflict: §10a inv. 10 ("`H_lock` is written once, at registration
finalization") and §12 ("Lock election/cutoff … finalize strictly before
Season start") against §8 ("At t_s, set `ell_i=min(requested_weight,w_i(t_s))`"
and "an indexer gap at t_s keeps all of B_lock reserved until H_lock is
final").

Decision — two separate events:

1. **Registration cutoff.** Registration is open only on `[FRUIT_T0, t_s)`.
   At exactly `t_s` it closes and elections become immutable. §12's
   "finalize strictly before Season start" refers to this cutoff, not to
   `H_lock`.
2. **Allocation finalization.** `ell_i = min(requested_weight, w_i(t_s))`
   comes from the `t_s` snapshot, so `H_lock` and `sum A` may only be posted
   at or after `t_s`. They become final only after the challenge/veto
   process, and are then write-once and immutable. If the `t_s` snapshot has
   an unresolved data gap, the allocation cannot finalize and all of
   `B_lock` stays reserved.

This overrides any literal reading that requires `H_lock` to exist before
`t_s`.

## P7-L2 — Unallocated budget release

Conflict: §8 ("Reserve the unallocated balance until registration is
finalized, then release it to Z") against §8 forfeiture ("At lock-pool close
the released capacity `E - final_total` is split … unallocated … -> 100% to
Z") and §10a inv. 8 ("unallocated pool capacity stays in U until the pool is
Closed").

Decision: all unallocated lock capacity stays in `U` until the lock pool
closes. Allocation finalization moves nothing to `Z`. At close,
`B_lock - final_total` is released exactly once under the canonical close
accounting; the 80/20 split applies only to the actual `forfeited`
component. No path may release the same capacity twice.

## Rounding

```text
u_i               = 5 ell_i                          (lamport-days)
H_lock            = max(sum u_i, 62.5 SOL-days)
gross_allocatable = floor(B * sum u_i / H_lock)      (computed by the engine, not poster input)
A_i               = floor(B * u_i / H_lock)          (never rounded up)
assigned_total    = sum_i A_i                        <= gross_allocatable <= B
A_i <= floor(B/5)
```

Checked arithmetic with u128 intermediates.

## F7 — Rounding dust is unallocated, not forfeiture

Date: 2026-09-29. Owner decision, frozen.

Rounding dust was never assigned to a wallet, so it is unallocated capacity:

```text
gross_allocatable = floor(B * sum u_i / H_lock)
assigned_total    = sum_i floor(B * u_i / H_lock)
rounding_dust     = gross_allocatable - assigned_total
unallocated       = B - assigned_total                (dust included)
forfeited        <= assigned_total - final_total
```

- Rounding dust returns 100% to `Z` at close, with the rest of `unallocated`.
- It is never eligible for the 80% treasury forfeiture transfer: the
  forfeiture bound is `assigned_total - final_total`, which excludes dust.
- `gross_allocatable` is never treated as the sum of wallet awards. The
  engine sees only `sum ell`, so the poster commits `assigned_total` with the
  allocation; it is bounded by `gross_allocatable` and goes through the same
  challenge/veto window as `H_lock`. Lock root capacity (the cliff) is
  `assigned_total`.

## Allocation lifecycle (engine, pool-owned)

The allocation lifecycle is stored on the lock pool itself, never inferred
from a numeric total:

```text
Uninitialized --post--> Pending(generation g) --finalize--> Final (terminal)
                           +--veto at threshold--> Uninitialized (g retired)
```

- Posting records the whole commitment (pool identity, generation, budget,
  `sum ell`, requested total, `H_lock`, `gross_allocatable`,
  `assigned_total`, window) on the pool.
- Veto and finalize act only on the pool's own Pending commitment and must be
  handed exactly that commitment, so a stale copy, a vetoed generation or an
  allocation from another pool cannot finalize.
- Finalize revalidates every committed field; Final is written explicitly and
  is permanent, including a Final allocation of zero.
- Lock pools are created only by `commit_lock_pool` (window derived from
  `FRUIT_T0`). The generic `post_root` and `terminal_resolution` refuse lock
  pools; roots, terminal resolution and close of a lock pool read only the
  pool's Final allocation.

This records engine behavior only; it changes no rule of the architecture.
