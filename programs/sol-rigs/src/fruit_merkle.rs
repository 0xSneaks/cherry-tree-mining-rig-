//! Domain-separated Merkle leaves and proofs for Fruit claims (§10a).
//!
//! ```text
//! leaf = sha256("CHERRY_FRUIT_LEAF_V1" ‖ program_id ‖ ledger_kind ‖ pool_id_le ‖ wallet ‖ cumulative_le)
//! node = sha256("CHERRY_FRUIT_NODE_V1" ‖ min(a, b) ‖ max(a, b))
//! ```
//!
//! Binding the program, ledger, pool and wallet into the leaf means a proof
//! cannot be replayed across any of them. Leaf and node hashes use different
//! tags and every leaf preimage has one fixed length, so an interior node can
//! never be presented as a leaf. The same functions build roots off-chain.

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::hash::hashv;

pub const LEAF_TAG: &[u8] = b"CHERRY_FRUIT_LEAF_V1";
pub const NODE_TAG: &[u8] = b"CHERRY_FRUIT_NODE_V1";

pub fn leaf(
    program_id: &Pubkey,
    ledger_kind: u8,
    pool_id: u64,
    wallet: &Pubkey,
    cumulative_amount: u64,
) -> [u8; 32] {
    hashv(&[
        LEAF_TAG,
        program_id.as_ref(),
        &[ledger_kind],
        &pool_id.to_le_bytes(),
        wallet.as_ref(),
        &cumulative_amount.to_le_bytes(),
    ])
    .to_bytes()
}

pub fn node(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    hashv(&[NODE_TAG, lo, hi]).to_bytes()
}

/// Whether `proof` connects `leaf` to `root`.
pub fn verify(proof: &[[u8; 32]], root: &[u8; 32], leaf: [u8; 32]) -> bool {
    proof.iter().fold(leaf, |acc, sibling| node(&acc, sibling)) == *root
}

/// Root and per-leaf proofs of `leaves` (off-chain helper; also used by the
/// tests). An odd node is carried up unchanged.
pub fn build(leaves: &[[u8; 32]]) -> ([u8; 32], Vec<Vec<[u8; 32]>>) {
    assert!(!leaves.is_empty(), "empty tree");
    let mut proofs = vec![Vec::new(); leaves.len()];
    let mut level: Vec<([u8; 32], Vec<usize>)> = leaves
        .iter()
        .enumerate()
        .map(|(i, l)| (*l, vec![i]))
        .collect();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut it = level.into_iter();
        while let Some((a, ia)) = it.next() {
            match it.next() {
                Some((b, ib)) => {
                    for &i in &ia {
                        proofs[i].push(b);
                    }
                    for &i in &ib {
                        proofs[i].push(a);
                    }
                    let mut members = ia;
                    members.extend(ib);
                    next.push((node(&a, &b), members));
                }
                None => next.push((a, ia)),
            }
        }
        level = next;
    }
    (level[0].0, proofs)
}
