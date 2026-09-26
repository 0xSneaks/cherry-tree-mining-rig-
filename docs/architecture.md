# Architecture

## Plain English

The frontend is the interface, the Rust engine is the accounting calculator, and the Solana program checks accounts and moves SOL. The backend is intended to read/index state and authenticate integration requests; it does not hold a user-transaction signing key.

## Technical detail

| Layer | Location | Responsibility |
|---|---|---|
| UI/integration | `app/src/` | Next.js, wallet signing, chain reads, Cherry surfaces |
| Pure engine | `crates/rigs-engine/src/` | pricing, accrual, shares, penalties, APR |
| Anchor program | `programs/sol-rigs/src/` | constraints, state transitions, lamport movement |
| Tests | `crates/rigs-engine/tests/`, `programs/sol-rigs/tests/`, `app/tests/`, `app/e2e/` | unit, native program-test, browser, parity |

The canonical program identity in the primary source is `C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt`, declared in `programs/sol-rigs/src/lib.rs` and `Anchor.toml`, and independently confirmed by the release reconciliation branch.
