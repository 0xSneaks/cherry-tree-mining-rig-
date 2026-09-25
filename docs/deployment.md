# Deployment

**DOCUMENTED INTENT:** scripts are devnet-guarded and verify the cluster by genesis hash. **INCOMPLETE / UNCONFIRMED:** this worktree does not establish a live deployment.

Safe sequence: run `./scripts/verify.sh`; run `./scripts/verify-deploy-ready.sh`; build/deploy with `./scripts/devnet-deploy.sh`; initialize with `./scripts/devnet-init.sh`; fund an explicit schedule with `./scripts/devnet-fund.sh amount duration`; then run smoke/rehearsal scripts and inspect read-back checks. Scripts refuse mainnet. Vercel configuration is in `app/vercel.json`; production hosting, Cherry registration, and production deployment are not verified.

Variables are documented in `.env.example` and `app/.env.example`. This package intentionally omits secret values.
