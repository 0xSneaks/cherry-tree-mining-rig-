pub mod admin;
pub mod buy_rigs;
pub mod claim;
pub mod compound;
pub mod fruit_claim;
pub mod fruit_ledger;
pub mod fruit_roots;
pub mod fund_rewards;
pub mod initialize;
pub mod migrate_v2;

// Globbed so the `#[program]` macro can see the generated client-accounts
// modules. Handlers are named per-instruction rather than `handler` so the
// re-exports stay unambiguous.
pub use admin::*;
pub use buy_rigs::*;
pub use claim::*;
pub use compound::*;
pub use fruit_claim::*;
pub use fruit_ledger::*;
pub use fruit_roots::*;
pub use fund_rewards::*;
pub use initialize::*;
pub use migrate_v2::*;
