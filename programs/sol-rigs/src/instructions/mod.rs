pub mod admin;
pub mod buy_rigs;
pub mod claim;
pub mod compound;
pub mod fund_rewards;
pub mod initialize;

// Globbed so the `#[program]` macro can see the generated client-accounts
// modules. Handlers are named per-instruction rather than `handler` so the
// re-exports stay unambiguous.
pub use admin::*;
pub use buy_rigs::*;
pub use claim::*;
pub use compound::*;
pub use fund_rewards::*;
pub use initialize::*;
