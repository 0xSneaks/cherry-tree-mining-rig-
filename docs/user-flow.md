# User flow

1. Connect through Cherry’s wallet bridge, or use the standalone wallet path.
2. During launch gating, satisfy the applicable access rule.
3. Buy whole rigs; the quote separates capital and fee.
4. Let an active funded schedule accrue rewards.
5. Claim the eligible balance or compound it into whole rigs.

The embedded path is intended to arrive with a wallet and show no connect-wallet step. Standalone wallet behavior is implemented but real extension execution is not verified here.

Technically, actions are `buy_rigs`, `claim`, and `compound`. The client builds transactions and receives signatures from the wallet/Cherry bridge. Chain state is authoritative for balances, schedules, gates, and APR. Pre-launch permits only the configured founder activation. From T0 through the first 24 hours, valid `.skr` proof and a cumulative per-wallet fresh-purchase cap apply; at the exact boundary access becomes public. `.sns` is not the V2 launch gate.
