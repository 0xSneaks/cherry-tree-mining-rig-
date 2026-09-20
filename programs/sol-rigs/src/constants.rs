//! PDA seeds and vault discriminants.

/// Seed for the singleton CONFIG PDA.
pub const CONFIG_SEED: &[u8] = b"config";
/// Seed prefix for a MINER POSITION PDA: `["miner", owner]`.
pub const MINER_SEED: &[u8] = b"miner";
/// Seed for the CAPITAL VAULT PDA.
pub const CAPITAL_VAULT_SEED: &[u8] = b"capital_vault";
/// Seed for the REWARD VAULT PDA.
pub const REWARD_VAULT_SEED: &[u8] = b"reward_vault";
/// Seed for the FEE VAULT PDA.
pub const FEE_VAULT_SEED: &[u8] = b"fee_vault";

/// On-chain state layout version.
pub const STATE_VERSION: u16 = 1;

/// Marks which vault a [`crate::state::Vault`] account is, so a mis-wired
/// account cannot pose as a different vault even before seed validation.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VaultKind {
    Capital = 0,
    Reward = 1,
    Fee = 2,
}
