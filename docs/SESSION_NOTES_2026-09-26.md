# THE CHERRY TREE — Session Notes / Operational Handoff

Date: 2026-09-26  
Purpose: capture the non-code decisions, wallet roles, websites, release notes, and follow-ups discussed during the current mainnet-preparation session.

This document is operational reference only. It does not supersede the frozen tokenomics architecture or implementation commits.

## 1. Current project checkpoint

### Canonical architecture / fee docs
- Branch: `claude/cherry-tree-tokenomics-review-12fa4b`
- Commit: `46433cc9234b78726949740c30eb81a4a31e1cd1`
- Status: frozen architecture plus founder-approved fee amendment.

### Pass #1 implementation
- Branch: `claude/cherry-tree-pass1-cap-skr`
- Branch tip: `c7241dfaa4997d56e93342c043a6a7a9b2953c0e`
- Core Pass #1 code commit: `d6f095789a003ca5a2bd6cc45b53f5cc0fa9406d`
- Implemented: permanent 250-seed cap, capped compound behavior, Day-1 one-.skr-domain-to-one-wallet binding.

### Pass #2 implementation
- Branch: `claude/cherry-tree-pass2-fruit-ledger`
- Commit: `8d0ccb62a1badcbba2f8897420b14c9728401c67`
- Implemented: Fruit ledger foundation, separate Fruit vaults, V/L/U/Z accounting, pools, roots, claims, challenge/veto mechanics, replay protection and funding transitions.
- Pass #2 does not yet contain the newest canonical docs from `46433cc`.

### Next-session starting rule
Start implementation work from:
`8d0ccb62a1badcbba2f8897420b14c9728401c67`

Then import the canonical docs from:
`46433cc9234b78726949740c30eb81a4a31e1cd1`

Do not build protocol code from the docs branch.

## 2. Wallets and authority roles

### Founder / fee-recipient / intended deployment wallet
Current recorded public wallet:

`FtuidxSaoNDeTuNTDuf9jLACVwqjgQeBwKTYC54f3GYp`

Current intended human-readable identity:
`sneaks.skr`

The session treated `sneaks.skr` as intended to resolve to the wallet above, but this must be independently re-verified immediately before deployment.

Current intended roles associated with this address:
- founder / activation wallet;
- configured protocol fee recipient / treasury destination;
- intended wallet identity used for the mainnet deployment workflow.

Important: role names are separate even when they point to the same public key.

### Reward funder
The project has a separate reward-funder role used to fund emissions.

Action required before mainnet:
- identify the exact production reward-funder public key;
- verify control of the signing key;
- verify the intended signer is available on the deployment machine;
- do not assume control merely because the address appears in Vercel or repo configuration.

### Key-control preflight
Before any mainnet initialization or funding, verify control of every required signer:
1. deployment / upgrade authority;
2. founder / activation signer;
3. reward funder;
4. treasury / fee recipient destination;
5. future Fruit governance keys: root poster and veto signers.

Never place private keys, seed phrases, or secret keypair contents in repository documentation.

## 3. Program identity and network state

Declared/current production program ID under review:

`C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt`

Founder-supplied mainnet read-only check returned `AccountNotFound`.

Operational conclusion:
- this is currently an initial-mainnet-deployment case for that program ID;
- there is no live mainnet Config/Miner migration population at that address;
- if a different production program ID is selected, or this address appears on-chain before deployment, repeat the deployed-state check before initialization.

Current work remains devnet/test/release-preparation work. Nothing in this session was deployed to mainnet.

## 4. Websites and public surfaces

### Repository
`https://github.com/PhantomCapAI/cherry-tree-mining-rig-`

### Current web app
`https://cherry-tree-mining-rig.vercel.app`

The app should remain usable outside Cherry where practical. Cherry-native users can receive product/community benefits, but the standalone website should not be intentionally crippled merely to force Cherry usage.

### Canonical documentation
`https://the-cherry-tree.gitbook.io/the-cherry-tree-docs`

Public documentation should stay confident, clear and forward-looking while remaining accurate about funded rewards, risks and protocol mechanics.

### Cherry mini-app
The product is intended to operate as a Cherry mini-app using the Cherry miniapp SDK and the existing Cherry room/community integration.

Before publication:
- SDK integration must remain correct;
- the exact production app URL must be supplied to Cherry;
- Cherry reviews the app before publishing it;
- affiliate/revenue-share terms should be documented separately from protocol accounting.

## 5. Website / UX backlog discussed

These items are parked until the protocol implementation/release path is stable.

### Navigation and accessibility
- add a language selector, with English first and support planned for major languages such as Spanish, Korean and Chinese;
- preserve a clean standalone experience outside Cherry;
- keep mobile behavior first-class.

### Tree progression
Replace unclear industrial/mining imagery with a clear Cherry Tree growth progression:

Seed → Sprout → Young Tree → Cherry Tree → Producing Cherry Tree

The tree/stage elements should be interactive where useful, with concise explanatory popovers.

### Visual polish
- use light cherry-blossom motion/background effects in statistics/yield sections where it does not hurt performance;
- visually communicate urgency for a funded window ending soon;
- keep the site polished rather than looking like a generic AI-generated dashboard.

### Information architecture
- Daily Miner Cockpit should link to a dedicated detail page rather than overcrowd the main page;
- replace bearish/confusing wording around “Your share & dilution” with clearer factual language;
- improve “Trust & Release” wording so users understand that rewards are funded/backed without overstating guarantees;
- remove any “stock mining” terminology;
- replace the old Phase 2 wording with a highly visual clickable roadmap.

### Roadmap metaphor
Use the existing product metaphor:
- roots;
- seeds;
- tree growth;
- fruit qualification;
- harvest.

The roadmap should visually show what is live, what is being built, and what comes later.

## 6. X / Twitter plan

An X account is worth establishing before launch because posting can be largely automated.

Handle discussion:
- `@CherryFinance` was considered, but the phrase has unrelated-brand collision risk;
- more distinctive candidates discussed include `@CherryTreeFi`, `@TheCherryTreeFi`, and `@CherryTreeSOL`.

No handle is recorded here as claimed or final.

Operating model:
- automate low-risk background posting and build updates;
- manually approve launch status, financial numbers, deployment claims and other sensitive announcements;
- do not announce a fixed launch time until the final mainnet smoke test passes;
- use the account to build history/credibility before launch rather than creating it on launch day.

## 7. Finalized economic decisions from this session

These are already represented in the canonical architecture at `46433cc`.

### Seed / wallet rules
- permanent current holding cap: 250 seeds;
- Fruit qualification threshold: 150 current seeds;
- Day-1 access: .skr only for the first 24 hours;
- one .skr domain may open Day-1 access for only one wallet;
- public access begins at T0 + 86,400 seconds.

### Lock
- one lock duration only: 5 days;
- no 1-day, 2-day or 3-day lock option;
- already-earned/vested value remains protected;
- voluntary early exit forfeits only the unvested conditional lock bonus;
- no additional early-exit percentage fee;
- released voluntary lock-forfeit capacity: 80% Protocol Treasury / 20% next-Season reserve;
- never-allocated capacity and safety-release future capacity return to reserve under the architecture rules.

### Protocol fees
Founder-approved architecture:
- fresh seed purchase fee: 10%;
- independent compound fee: 10%;
- compound fee may never exceed the purchase fee;
- fee changes: 72-hour on-chain timelock;
- referral: 20% of eligible fee to the direct referrer / 80% retained by the Protocol Treasury, subject to the retained-margin rule;
- required retained margin: 8% of fee-bearing principal;
- early-claim fee: 30% under 24h, 20% from 24–48h, 10% from 48–72h, 0% thereafter;
- early-claim fee destination: 60% Protocol Treasury / 40% grove redistribution;
- CAPITAL vault remains non-withdrawable user principal;
- protocol fees do not automatically fund rewards.

Public terminology should use “Protocol Treasury” or “Protocol Sustainability Fund”, not language implying arbitrary founder extraction.

## 8. Treasury and revenue interpretation

Legitimate treasury revenue includes:
- purchase fees;
- compound fees;
- retained protocol portion of referral-eligible fees;
- approved treasury share of the disclosed early-claim fee;
- approved treasury share of released voluntary lock-forfeit capacity.

Do not classify the following as treasury revenue:
- seed principal in the CAPITAL vault;
- already-earned/vested user rewards;
- Fruit/loyalty/lock reward reserves or liabilities;
- referral liabilities;
- network fees/rent;
- reward-funder capital until an explicit separate treasury transfer exists.

The fee-model scenarios are projections, not guaranteed income.

Example from the approved fee model at 1.0x emission:
- 1,000 wallets averaging 150 seeds → approximately 204.4 SOL modeled net treasury revenue;
- this is a scenario output and depends on actual participation, activity, referrals, claiming behavior and final live configuration.

## 9. Branch cleanup / repository hygiene

The repository accumulated multiple specialist branches during the security, tokenomics and implementation work.

Before final release:
- inventory all branches;
- identify unique commits not represented in the intended release lineage;
- preserve any unique security or implementation fix;
- establish one canonical release branch;
- do not delete branches until their unique commits are proven merged/preserved;
- do not implement from stale review/docs branches.

The current known implementation lineage should continue from Pass #2, not from the tokenomics-review branch.

## 10. Release target and scope discipline

The working target discussed is Tuesday, but it is a target, not a launch promise.

Do not expand scope merely to hit the date.

Remaining release work includes:
- import latest canonical docs into the next implementation branch;
- implement the approved fee architecture;
- continue the remaining architecture passes;
- complete the expanded implementation test gate;
- run a real SBF build/rehearsal;
- verify all production signer/key control;
- verify final program ID and production configuration;
- perform controlled real-SOL mainnet smoke actions before broad public opening.

No extra product ideas should interrupt these release gates unless they expose a genuine blocker.

## 11. Important build caveat

Pass #1 and Pass #2 passed host/native test suites, but the environment used by those sessions did not contain the Solana SBF build toolchain.

Therefore a real SBF compilation and deployment rehearsal remains a release gate.

Do not treat host-test success alone as final deploy readiness.

## 12. Partnership / Cherry notes

Cherry team guidance previously received:
- integrate the Cherry miniapp SDK;
- create/use the Cherry room;
- provide the app URL;
- Cherry reviews the app;
- if successful, Cherry publishes it;
- commercial relationship is expected to use an affiliate/revenue-share model.

Potential promotional requests discussed for later negotiation:
- pinned launch announcement in relevant Cherry/Seeker communities;
- banner/promotional placement;
- coordinated traffic support.

Do not let partnership negotiation interrupt the release-critical engineering sequence.

## 13. Next-session operational checklist

1. Start from Pass #2 implementation commit `8d0ccb62a1badcbba2f8897420b14c9728401c67`.
2. Import canonical docs from `46433cc9234b78726949740c30eb81a4a31e1cd1`.
3. Verify worktree, branch, HEAD and clean state before edits.
4. Verify all wallet roles and signing-key control before deployment work.
5. Continue one narrow implementation pass at a time.
6. Keep the Tuesday launch goal subordinate to security/tests.
7. Do not deploy, initialize or change authorities until the release gates explicitly authorize it.

---

This handoff intentionally contains no private keys or secret material.
