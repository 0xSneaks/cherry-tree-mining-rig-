# Security

Only disclosed fees can be withdrawn, and only to the configured fee recipient. CAPITAL and REWARD have no authority drain path. Pausing blocks user actions while funding remains available. Authority transfer is two-step.

The program checks PDA seeds, owners, vault kind, signers, fee bounds, arithmetic, schedule solvency, launch rules, referrers, and reward watermarks. Tests cover vault substitution, drain attempts, forged referrers, pause behavior, checked math, token verification, and CSP. Independent economics and security review is part of the mainnet readiness gates before launch.
