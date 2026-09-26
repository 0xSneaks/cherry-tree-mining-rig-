# Overview

## Plain English

THE CHERRY TREE is a game loop built around rigs and a funded reward stream. A purchase creates production capacity; it does not fund rewards owed to other users. Rewards exist only when an authorized funder deposits SOL into the reward vault and sets an emission window.

Rigs cannot be sold back or redeemed. Users can claim settled rewards or compound them into whole additional rigs.

## Technical detail

Three program-owned vaults are separate: CAPITAL receives non-fee purchase proceeds and is locked in this version; REWARD receives `fund_rewards` deposits and pays `claim`/`compound`; FEE receives purchase fees and is the only vault with an authority withdrawal path. Accounting lives in `crates/rigs-engine/`; Anchor handlers validate accounts and move lamports.

- **VERIFIED:** source/tests cover separation, integer accounting, instruction validation, and adversarial cases.
- **DOCUMENTED INTENT:** deployment and operational procedures describe a live launch.
- **INCOMPLETE / UNCONFIRMED:** this checkout does not prove live deployment, real Cherry handshake, or live wallet execution.
- **INFERENCE:** figures are not a live schedule unless read from chain state.

## Current vs. planned

This site documents the base Tree as implemented on the release lane. The planned Fruit layer, the roadmap, and the open decisions are summarized in [What's next](whats-next.md) and [Roadmap and status](roadmap.md).
