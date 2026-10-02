# Physical Seeker validation — executable test plan (Backlog 17)

**Status: NOT EXECUTED.** Nothing here has been run on a device. Fill each row with PASS/FAIL
and evidence (screenshot/screen recording filename, tx signature, explorer link). A row without
evidence is not a PASS.

## 0. Setup (record)

| Item | Value |
|---|---|
| Date / tester | |
| Seeker OS build, Cherry app version | |
| Cluster / RPC / program ID | |
| Deployment URL + git commit | |
| `CHERRY_APP_ID` configured? (Y/N) | |
| `NEXT_PUBLIC_CHERRY_TREE_OFFICIAL_REFERRER` (value or "unset") | |
| Wallets: A = has `.skr`; B = no domain; C = fresh (no position); R = existing planter (referrer) | |

Run on **devnet or a disposable qualification deployment first**, never the protected
qualification QID. A mainnet run happens only inside the mainnet runbook's physical-Seeker gate.

## 1. Wallet and identity

| # | Step | Expected | PASS/FAIL | Evidence |
|---|---|---|---|---|
| 1.1 | Open the app in Cherry (mobile) | Grove renders; **no** connect-wallet UI; wallet address shown via host | | |
| 1.2 | Open standalone in the Seeker browser | Connect UI present; Seed Vault wallet connects | | |
| 1.3 | Wallet A: launch panel identity | ".skr domain verified" | | |
| 1.4 | Wallet A with a wrapped (NFT) `.skr` if available | Verified via name-house path | | |
| 1.5 | Wallet B | "No .skr or .sns domain found"; planting closed during Genesis | | |
| 1.6 | AllDomains resolution under the production RPC | No RPC refusal errors in console (`getProgramAccounts` memcmp) | | |

## 2. First launch and WebView behaviour

| # | Step | Expected | PASS/FAIL | Evidence |
|---|---|---|---|---|
| 2.1 | Cold start in Cherry | First paint shows seeds with no interaction | | |
| 2.2 | Background Cherry 5 min, resume | Ticker stopped while suspended; re-anchors to chain on resume (claimable matches explorer within a few lamports/s × drift) | | |
| 2.3 | Rotate / small screen | No horizontal scroll | | |
| 2.4 | Airplane mode mid-session, then back | Clear error; no optimistic numbers; recovers on refresh | | |

## 3. Referral

| # | Step | Expected | PASS/FAIL | Evidence |
|---|---|---|---|---|
| 3.1 | Share `https://<app>/?ref=<R>` into a Cherry chat; wallet C opens it | Buy panel shows "Referred by R…" **before** signing | | |
| 3.2 | Same, but check whether Cherry preserved `?ref=` (inspect URL via share/debug) | Record: preserved / stripped | | |
| 3.3 | C plants | Grove record: weight 1.05×, referral bound to R; `ReferralBound` in tx logs | | |
| 3.4 | Fresh wallet opens app with **no** link, fallback **configured** and official wallet is a planter | Highlighted "official Cherry Tree referral" disclosure before signing; after planting, bound to official wallet, weight 1.05× | | |
| 3.5 | Fresh wallet, no link, fallback **unset** | No referral disclosure; plants at 1.00× | | |
| 3.6 | Fresh wallet with explicit link to R while fallback configured | R is bound, not the official wallet | | |
| 3.7 | Fresh wallet with link to a non-planter | "not a current planter" note; no referral bound; fallback NOT used | | |
| 3.8 | Existing planter opens any `?ref=` | No referral shown; later purchase binds nothing | | |

## 4. Buy / claim / compound

| # | Step | Expected | PASS/FAIL | Evidence |
|---|---|---|---|---|
| 4.1 | Plant 1 seed | One Cherry signing sheet; quote total == lamports debited (explorer) | | |
| 4.2 | Genesis (if in window): exceed allowance | Button disabled / allowance message; no tx sent | | |
| 4.3 | Replant | Seeds increase; no penalty; Genesis allowance unchanged | | |
| 4.4 | Claim within 24h | Confirm sheet shows 25% penalty and net; received == net | | |
| 4.5 | Reject the signing sheet | Clear "cancelled" message; state unchanged | | |
| 4.6 | Double-tap an action | One transaction only | | |
| 4.7 | Retry after RPC failure | Product-language error, raw error in console, retry works | | |

## 5. Share / inline render

| # | Step | Expected | PASS/FAIL | Evidence |
|---|---|---|---|---|
| 5.1 | Replant, accept share prompt | Share picker shows preview card | | |
| 5.2 | Recipient views the blink | Read-only card from signed params | | |
| 5.3 | Blink with `CHERRY_APP_ID` unset | Card refuses (fail closed) | | |

## 6. Growth layer

| # | Step | Expected | PASS/FAIL | Evidence |
|---|---|---|---|---|
| 6.1 | Open "Your share & dilution" | Figures match `/api/global`; what-if updates | | |
| 6.2 | Open "Grove milestones" | Tier / achievements consistent with grove record | | |
| 6.3 | Open "Referrals" | Copy link works; board closed unless qualification configured | | |
| 6.4 | Dormant deployment (runway 0) | Dormant banner; no projections; buy panel Dormant note | | |

## Sign-off

| Tester | Date | Result | Blocking failures |
|---|---|---|---|
| | | | |
