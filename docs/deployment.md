# Deployment

Scripts are devnet-guarded and verify the cluster by genesis hash; mainnet deployment follows the gated runbook in [Roadmap and status](roadmap.md).

Safe sequence: run `./scripts/verify.sh`; run `./scripts/verify-deploy-ready.sh`; build/deploy with `./scripts/devnet-deploy.sh`; initialize with `./scripts/devnet-init.sh`; fund an explicit schedule with `./scripts/devnet-fund.sh amount duration`; then run smoke/rehearsal scripts and inspect read-back checks. Vercel configuration is in `app/vercel.json`; production hosting and Cherry registration are configured at launch per the runbook.

Variables are documented in `.env.example` and `app/.env.example`. This package intentionally omits secret values.
