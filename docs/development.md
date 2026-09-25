# Development

Run `./scripts/verify.sh` for the repository’s main local verification pipeline. It runs Rust tests, regenerates pricing/accrual/yield/launch/penalty fixtures, Vitest, and configured browser checks where dependencies exist. Native `solana-program-test` exercises the real Anchor entrypoint without SBF. Browser tests stub RPC and Cherry boundaries, so they do not prove live network/device integration.

The app is under `app/`; workspace metadata is in `Cargo.toml`; Anchor configuration is in `Anchor.toml`; SBF tooling is `scripts/build-sbf.sh`. Never place keypairs or secrets in the repository.
