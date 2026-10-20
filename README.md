# KNID-Solana Core (Rust) — Staging Candidate

Read-only Solana RPC and deterministic trade-analysis primitives. Paper-only order simulation. No wallet, private keys, live order submission, or privileged remote shell.

## Build and test
```bash
cargo fmt --all -- --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked
python3 test_cli_e2e.py
```

## Inspect a mint using a trusted Solana RPC
```bash
cargo run --locked -- inspect-mint https://api.mainnet-beta.solana.com So11111111111111111111111111111111111111112
```
The reported raw supply is the mint's current RPC value, **not a circulating-supply estimate or USD market cap**.

## Implemented
- Base58 32-byte mint validation and Token/Token-2022 owner checks.
- Read-only JSON-RPC getAccountInfo and getTokenSupply at finalized commitment.
- Normalized **verified** DEX swaps with duplicate rejection and chronological ordering.
- OHLCV aggregation: 30 seconds, 1 minute, 5 minutes, 15 minutes, 1 hour, 4 hours, 1 day.
- Explicit TVL and circulating market-cap calculations with independently provided asset USD quotes.
- Paper orders: market, limit, stop, stop-limit, and percent sizing, fees and worst-case bounded simulated slippage.
- Offline mock RPC E2E and negative tests.

## Not implemented or authorized
- No pool discovery, transaction decoding per Raydium/Orca/other DEX, WebSocket streaming, backfill or fork-aware persistence. **Never infer swaps directly from transfers.**
- No verified CLMM active liquidity, historical circulating supply, oracle integration, price feeds or real PnL accounting beyond paper fills.
- No signing, smart order routing, live buying/selling, market orders on-chain or wallet access.
- Source is published to GitHub main and present in the KNID-Solana workspace. Its GitHub commit is verified independently; EG canonical production E3 admission and promotion evidence have not been completed.

Safety: use this code for development/testing; it is not a trading platform ready for funds.

Recent finalized account signatures:
```bash
cargo run --locked -- inspect-signatures https://api.mainnet-beta.solana.com So11111111111111111111111111111111111111112
```
This is account-address activity only; mint signatures do **not** enumerate all DEX pool swaps, so these records must not be transformed directly into OHLCV candles.

## Verified staging acceptance (2026-10-20)

- Rust: 30 unit tests passed, formatting and Clippy passed; mock JSON-RPC CLI and live read-only mainnet calls passed.
- Candidate Evaluation Gateway: 345 tests passed, 1 skipped. A sandbox E3 Cargo transaction reached REGISTER → PRE → signed receipt → POST → SATISFIED.
- Non-root worker: UID `daemon` used a distinct local Rust toolchain and signed receipt with replay checks; Unix-socket liveness attested separately.
- **Not a production release:** canonical EG KNID-Solana task remains at CP4 as last verified. No production owner HITL, no canonical E3 promotion evidence, and no live trading authority. The GitHub main commit is a development-source publication, not an EG-verified production promotion. Mock tests are not production authorization.
