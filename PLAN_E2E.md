# KNID-Solana — Final E2E Plan (read-only)

## Goal and constraints
Given only a Solana mint CA, discover trading venues from token creation, reconstruct each *verified* DEX swap in canonical chain order, build OHLCV at 1-second and higher intervals, and transition from archival backfill to live streaming without silent gaps. Never trade, sign, or infer swaps from transfers.

**Time semantics:** Solana block time is second-level estimate. `received_at_ns` means the local arrival timestamp, **not** historical execution millisecond precision. Candle timestamps are tagged `block_time_estimate` or `observation_time`. Never claim exact historical 1-ms timing.

## Ordered acceptance gates
1. **CA + discovery.** Validate Token and Token-2022 owners; find mint creation and bonding curve/pools (Pump.fun/PumpSwap, Raydium, Orca, Meteora); preserve pool provenance, migrations and detection gaps. PASS: known CA resolves; invalid CA fails closed.
2. **Archival history.** Page `getSignaturesForAddress` with before cursor, deduplicate and persist cursor per account; discover all pool histories, not just the mint address; require archival coverage or explicitly report inaccessible ranges. PASS: pagination, replay, empty page, rate limit and missing history tests.
3. **Verified swaps.** Decode protocol-specific instructions, inner instructions and amounts; distinguish swaps from transfers, failed transactions, mint/burn and liquidity. PASS: fixture parity for each DEX and on-chain spot checks; no unknown transactions classified as trades.
4. **Canonical tick ledger.** Durable append-only entries with signature, slot, tx-index, instruction-index, pool, base/quote amounts, price, side, commitment, provenance and observation timestamp; unique event ID, fork rollback, ordering and restart idempotency. PASS: replay consistency and zero accepted duplicates.
5. **OHLCV 1s and derived intervals.** Aggregate only verified swaps; expose 1s, 5s, 10s, 30s, 1m, 10m; track empty intervals as no-trade not fake trades. Keep provisional observations separate from finalized reconstruction. PASS: recomputation matches original tick ledger exactly.
6. **Live stream and catch-up.** Start Yellowstone/Geyser streaming (RPC WebSocket fallback), snapshot anchor slot, concurrent historical catch-up, checkpoint barriers, gap scan and recovery. PASS: disconnection/reorg/failover under stress does not silently lose ticks.
7. **Visibility and performance.** Display tick tape with arrival timestamps to millisecond, 1s/30s OHLCV chart, latency percentiles, stream health, gaps and provider coverage. Target p99 decode <=5ms and >=10k events/s worker as a **benchmark goal**, not promised chain-level delivery latency.
8. **Release / EG.** Rust fmt/test/clippy/build, deterministic and real CA tests with evidence; separately trusted EG executor PRE -> effect -> signed receipt -> POST -> VERIFIED for every task criterion. Never edit EG checkpoints directly; independent HITL remains mandatory when required by policy.

## Initial validation CA
`3xr8F1c8XcNJCFtzJgEAV8kNGrdJxU4sdidZFiWBpump`, observed Pump.fun pool `Gb6jnAQed7oQenNmQKG6kL7u6JstzzTajshJC2HEwmmt`. Provider candles are reference-only, not verification of individual swaps.

## Engineering discipline
Karpathy guidelines: minimal changes, tests first, explicit assumptions, no unasked trading functionality, no hardcoded DEX-specific logic in EG core, every claim tied to test output.

[executed on device: 6a59e52b5668 (5255b4bd-412b-4ac8-9b22-0b77cd4dee13)]