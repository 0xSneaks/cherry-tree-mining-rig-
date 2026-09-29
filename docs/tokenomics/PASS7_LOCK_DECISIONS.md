# Pass #7 — Lock decision amendment (P7-L1, P7-L2, rounding)

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
u_i    = 5 ell_i                       (lamport-days)
H_lock = max(sum u_i, 62.5 SOL-days)
sum A  = floor(B * sum u_i / H_lock)   (computed by the engine, not poster input)
A_i    = floor(B * u_i / H_lock)       (never rounded up)
sum_i floor(A_i) <= sum A <= B
A_i <= floor(B/5)
```

Checked arithmetic with u128 intermediates.
