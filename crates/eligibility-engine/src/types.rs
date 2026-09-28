//! Addresses, the global event key and the finalized event vocabulary.

/// A 32-byte account address. Deliberately opaque: this crate never embeds
/// production addresses (gate 14.2-D); they arrive through configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Addr(pub [u8; 32]);

impl Addr {
    /// A deterministic test/fixture address. Never a production constant.
    pub const fn fixture(n: u8) -> Addr {
        let mut b = [0u8; 32];
        b[0] = 0xF1;
        b[31] = n;
        Addr(b)
    }
}

/// Global, total order of finalized chain effects:
/// `(slot, transaction index, instruction index, inner-instruction index)`.
///
/// `inner_index = 0` is the top-level instruction's own effect; inner
/// instruction `k` of that instruction is `inner_index = k + 1`, so a
/// top-level effect always precedes its CPIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct EventKey {
    pub slot: u64,
    pub tx_index: u32,
    pub ix_index: u32,
    pub inner_index: u32,
}

impl EventKey {
    pub const fn new(slot: u64, tx_index: u32, ix_index: u32, inner_index: u32) -> Self {
        Self {
            slot,
            tx_index,
            ix_index,
            inner_index,
        }
    }
}

/// Who authorised a token movement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Authority {
    Owner,
    Delegate,
}

/// Unit-level encumbrances reported by an adapter (section 5): units pledged
/// as collateral, held in a collateral/debt wrapper, or vesting-locked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EncumbranceKind {
    Collateral,
    Vesting,
}

/// Observed finalized state of one token account (checkpoint snapshot).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccountSnapshot {
    pub account: Addr,
    pub mint: Addr,
    pub owner: Addr,
    pub amount: u64,
    pub delegate: Option<Addr>,
    pub delegated_amount: u64,
    pub frozen: bool,
}

/// Observed finalized state of one stake beneficiary position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StakeSnapshot {
    pub mint: Addr,
    pub beneficiary: Addr,
    pub active: u64,
    pub pending_unstake: u64,
}

/// Observed finalized mint supply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SupplySnapshot {
    pub mint: Addr,
    pub supply: u64,
}

/// One debt amount, in units of a priced asset (`PriceKey`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DebtAmount {
    pub price_key: crate::valuation::PriceKey,
    pub units: u64,
}

/// Finalized ownership-, encumbrance- and debt-changing effects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventKind {
    // ---- SPL / Token-2022 account lifecycle and movement ----
    InitializeAccount {
        account: Addr,
        mint: Addr,
        owner: Addr,
    },
    CloseAccount {
        account: Addr,
    },
    MintTo {
        account: Addr,
        amount: u64,
    },
    Burn {
        account: Addr,
        amount: u64,
        authority: Authority,
    },
    Transfer {
        source: Addr,
        destination: Addr,
        amount: u64,
        authority: Authority,
    },
    /// `SetAuthority(AccountOwner)`: an ownership transfer of the whole
    /// account. Clears any delegate, as the token program does.
    SetAccountOwner {
        account: Addr,
        new_owner: Addr,
    },
    /// Sets (replaces) the delegate and `delegated_amount`.
    Approve {
        account: Addr,
        delegate: Addr,
        amount: u64,
    },
    Revoke {
        account: Addr,
    },
    Freeze {
        account: Addr,
    },
    Thaw {
        account: Addr,
    },

    // ---- adapter-reported unit encumbrance ----
    Encumber {
        account: Addr,
        kind: EncumbranceKind,
        amount: u64,
    },
    Release {
        account: Addr,
        kind: EncumbranceKind,
        amount: u64,
    },

    // ---- stake (beneficiary positions; custody accounts hold the units) ----
    Stake {
        source: Addr,
        custody: Addr,
        beneficiary: Addr,
        amount: u64,
    },
    RequestUnstake {
        mint: Addr,
        beneficiary: Addr,
        amount: u64,
    },
    CancelUnstake {
        mint: Addr,
        beneficiary: Addr,
        amount: u64,
    },
    WithdrawUnstaked {
        custody: Addr,
        destination: Addr,
        beneficiary: Addr,
        amount: u64,
    },

    // ---- participation ----
    Register {
        wallet: Addr,
        asset: crate::adapter::AssetKind,
    },
    Exit {
        wallet: Addr,
    },

    // ---- debt registry ----
    /// A finalized reading of `wallet`'s outstanding debt in `program`.
    /// Readings for programs outside the fixed registry are ignored: that
    /// debt is not detected (section 5, test 28).
    DebtObserved {
        wallet: Addr,
        program: Addr,
        debts: Vec<DebtAmount>,
    },
    /// `program`'s state for `wallet` could not be read.
    DebtUnreadable {
        wallet: Addr,
        program: Addr,
    },

    // ---- external program versions (adapter pinning) ----
    ProgramVersion {
        program: Addr,
        version: [u8; 32],
    },

    // ---- indexer continuity ----
    /// Full observed finalized state of every tracked account, stake
    /// position and mint supply. Closes a reconciliation window.
    Checkpoint {
        accounts: Vec<AccountSnapshot>,
        stakes: Vec<StakeSnapshot>,
        supplies: Vec<SupplySnapshot>,
    },
    /// The indexer knows history is missing from here to the next checkpoint.
    DeclaredGap,
}

/// One finalized effect at its global position and slot timestamp.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub key: EventKey,
    /// `Clock::unix_timestamp` of `key.slot`.
    pub unix_timestamp: i64,
    pub kind: EventKind,
}
