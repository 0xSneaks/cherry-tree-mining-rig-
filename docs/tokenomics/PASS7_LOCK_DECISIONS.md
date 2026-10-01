# Pass #7 — Lock decision amendment (P7-L1, P7-L2, rounding, F7, P7-B)

Date: 2026-09-29. Founder decisions, frozen for Pass #7. Narrow amendment to
`CHERRY_TREE_FINAL_TOKENOMICS_ARCHITECTURE.md` (§8, §10a, §12); it resolves
only the conflicts quoted below and changes nothing else. Implemented by
`crates/rigs-engine/src/fruit_lock.rs` (P7-A) and
`programs/sol-rigs/src/instructions/fruit_lock.rs` (P7-B).

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
- `gross_allocatable` is never treated as the sum of wallet awards. Lock
  root capacity (the cliff) is `assigned_total`.
- Superseded in P7-B (see below): at 7a28be1 the poster committed
  `assigned_total`, bounded only by `gross_allocatable` (verification
  findings V-1 / V-2). Since P7-B the engine computes it; it is never an
  input.

## Authorization of the P7-A remediation (F1, F2, F3, F7)

Date: 2026-09-29. The founder confirmed that F1, F2, F3 and F7 were
authorized remediation findings for P7-A, and that the `programs/sol-rigs`
changes of the remediation commit `7a28be1` (`Pool::accounting()` deriving
the pool kind, and the two updated `t89` program tests) were authorized
remediation. P7-A was accepted as verified at `7a28be1`.

## P7-B — on-chain registration and exact allocation tally

Date: 2026-09-29. Founder decisions and acceptance criteria, frozen.

### V-1 / V-2 closure (acceptance criteria)

`assigned_total` is never poster input. It is computed by the engine from
authoritative on-chain registration state:

1. Each wallet registers once per Season (`LockRegistration` PDA
   `["lock_reg", pool, wallet]`, `0 < requested <= Kmax`).
2. At or after `t_s` the poster posts, per active registration and once per
   generation, `ell_i <= requested_i` (the `t_s` snapshot weight).
3. Weights seal only when every active registration is weighed; the engine
   then fixes `H_lock` and `gross_allocatable`.
4. The permissionless award pass computes `A_i = floor(B * 5 ell_i / H_lock)`
   per registration, once per generation, and adds it to `assigned_total`.
5. Awards seal only when every active registration is awarded; the tally is
   revalidated in full (counts, recomputed `H_lock` / gross, `budget`,
   `assigned_total <= gross <= B`, and the additional invariant
   `assigned_total >= gross - (positive_count - 1)`) and the challenge window
   opens. Finalization revalidates again and is write-once.

Hence `assigned_total = sum_i floor(A_i)` exactly and
`dust = gross_allocatable - assigned_total` exactly. The forfeiture bound is
`assigned_total - final_total`, so no poster value can reclassify dust as
forfeiture or route it to the treasury. The poster still attests each
`ell_i` (inherent: `w_i(t_s)` is off-chain history), visible per wallet on
its registration account and subject to the veto window.

### D-B1 — cancellation window (approved)

Cancellation is allowed only during `[FRUIT_T0, t_s)`. At `t_s`
registrations become immutable. Accounting:

- `total_reg_count` counts every registration ever made; `active_reg_count`
  and `active_requested_total` cover non-cancelled ones.
- A first valid cancellation atomically marks the registration cancelled,
  decrements `active_reg_count` once and subtracts its `requested` from
  `active_requested_total` (checked; an underflow is corrupt state and fails
  closed). A repeated cancellation fails without mutation.
- The registration account is kept, so the wallet cannot register again.
- Cancelled registrations cannot be weighed or awarded; both seals compare
  against `active_reg_count`. Counters freeze at `t_s`, and every veto
  re-tally runs over the same frozen active set.
- **Zero-active path.** If every registration is cancelled (or none is
  made), at or after `t_s` the empty tally seals with `sum ell = 0`,
  `H_lock` at the floor and `gross = assigned = 0`, and finalizes after the
  challenge window like any allocation. A zero-total complete root then
  closes the pool with all of `B_lock` returned to `Z` (P7-D wires the lock
  root / close path).

### D-B2 / D-B3 / D-B4 / D-B5

- D-B2 (approved default): registration accounts stay open; no refund-close
  yet (addable later without a layout change).
- D-B3 (approved default): no minimum weight and no registration cap.
  Registration-spam / liveness risk: every active registration must be
  weighed and awarded before the allocation can finalize, so a large number
  of registrations lengthens the tally (about `n / 24` weight transactions
  and `n / 24` award transactions). Each spam registration costs its creator
  the account rent; counters are checked. A tally that is never completed
  keeps all of `B_lock` reserved in `U` (fail closed), it never releases it.
- D-B4 (approved): the award pass, both seals and finalization are
  permissionless; posting weights is root-poster only; veto uses the Fruit
  veto set.
- D-B5: deferred to P7-C / P7-D.

## P7-D — lock settlement (roots, claims, terminal resolution, close)

Date: 2026-10-01. Implemented under the founder's instruction to finish
Pass #7; each item below enforces an existing frozen rule, none adds one.

- **B-1, one lock pool per Season (§8 "one immutable lock election per
  wallet per Season").** `commit_lock_pool` requires `pool_id == FRUIT_T0`
  of the Season, so a second lock pool for the same Season resolves to the
  same pool PDA and cannot be created; one wallet can hold at most one
  registration per Season.
- Lock roots, root finalization, terminal resolution, claims and close go
  only through `post_lock_root`, `finalize_lock_root`,
  `resolve_lock_pool_terminal`, `claim_lock` and `close_lock_pool`, which load
  the Season's Final allocation. The generic Fruit instructions refuse lock
  pools (`claim_fruit` included); vetoes use `veto_fruit_root`.
- A lock claim is bounded by the wallet's own Final award `A_i` (its
  registration account), so even a faulty or malicious root cannot pay a
  wallet more than its award, a cancelled wallet anything, or a wallet with
  no registration anything.
- Capacity is the cliff only: the safety-release path stays disabled because
  the EXTREME dwell period is unset and no instruction can set it (D-B5).
- Close releases `B_lock - final_total` from `U` to `Z` exactly once and then
  `floor(0.80 forfeited)` from `Z` to the Protocol Treasury
  (`config.fee_recipient`); unallocated capacity and dust stay 100% in `Z`.
- Open (founder): B-2 — the allocation tally advances only when the root
  poster posts weights; there is no veto-quorum fallback for the allocation.
  A stalled tally keeps `B_lock` reserved in `U` (fail closed).

## Allocation lifecycle (engine, pool-owned)

Since P7-B the lifecycle is a tally, stored on the lock pool (on-chain: the
pool's `LockSeason` PDA `["lock_season", pool]`), never inferred from a
numeric total:

```text
Registering --t_s--> Weighing(g) --seal--> Awarding(g) --seal--> Pending(g) --window--> Final
                        ^                                            |
                        +---------- veto at threshold: g+1 ----------+
```

- Final is written explicitly and is permanent, including a Final zero
  allocation; `Pool.h_lock` mirrors the Final `H_lock` and must agree with
  the Season on every read.
- Lock pools are created only by `commit_lock_pool` (window derived from
  `launch_t0`). The generic `post_root` and `terminal_resolution` refuse lock
  pools; roots, terminal resolution and close of a lock pool read only the
  pool's Final allocation.
- No account layout changes: `Pool`, `FruitConfig`, `Ledger`, `Root` and
  `ClaimRecord` are unchanged, `LockSeason` and `LockRegistration` are new,
  `FRUIT_STATE_VERSION` stays 1; no migration.

This records engine and program behavior only; it changes no rule of the
architecture.
