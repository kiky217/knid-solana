use knid_solana_core::rpc::{validate_mint, SolanaRpc};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if (args.len() != 4
        && !(args.len() == 5 && args[1] == "scan-pump-history")
        && !((args.len() == 5 || args.len() == 6) && args[1] == "scan-pump-pool-history"))
        || !matches!(
            args[1].as_str(),
            "inspect-mint"
                | "inspect-signatures"
                | "inspect-history"
                | "inspect-transaction"
                | "inspect-deltas"
                | "inspect-pump-trades"
                | "scan-pump-history"
                | "scan-pump-pool-history"
                | "discover-pools"
        )
    {
        eprintln!("usage: knid-solana-core <inspect-mint|inspect-signatures|inspect-history|inspect-transaction|inspect-deltas|inspect-pump-trades|scan-pump-history|discover-pools> <https-rpc-url> <mint-or-account-address>");
        std::process::exit(2)
    }
    let address = &args[3];
    if !matches!(
        args[1].as_str(),
        "inspect-transaction" | "inspect-deltas" | "inspect-pump-trades"
    ) && validate_mint(address).is_err()
    {
        eprintln!("invalid address");
        std::process::exit(2)
    }
    let client = match SolanaRpc::new(args[2].clone()) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("invalid rpc endpoint");
            std::process::exit(2)
        }
    };
    if args[1] == "scan-pump-pool-history" {
        let pool = &args[4];
        let result = knid_solana_core::backfill::scan_pool_trade_events_from(
            &client,
            address,
            pool,
            10,
            2,
            args.get(5).map(String::as_str),
        );
        match result {
            Ok(batch) => {
                for row in &batch.trades {
                    println!(
                        "slot={} signature={} mint={} event_index={} side={} price_sol={:.14}",
                        row.slot,
                        row.signature,
                        row.event.mint,
                        row.event.event_index,
                        if row.event.is_buy { "BUY" } else { "SELL" },
                        row.price_sol
                    );
                }
                for interval_s in [1, 30, 600] {
                    let candles =
                        knid_solana_core::sol_candles::aggregate_sol(&batch.trades, interval_s)
                            .unwrap_or_else(|e| {
                                eprintln!("invalid OHLCV: {e:?}");
                                std::process::exit(1)
                            });
                    println!(
                        "tf_seconds={} candles={} trades={}",
                        interval_s,
                        candles.len(),
                        candles.iter().map(|c| c.trades).sum::<u64>()
                    );
                }
                for sig in &batch.unresolved_signatures {
                    println!("unresolved_signature={} reason=RPC_LOG_TRUNCATED", sig);
                }
                println!("scanned_signatures={} decoded_trade_events={} reached_oldest={} next_cursor={:?} coverage={} pool={} mint_membership_derived_from_trade_event=true",
                    batch.scanned_signatures,batch.trades.len(),batch.reached_oldest,batch.next_cursor,batch.coverage_scope,pool);
                return;
            }
            Err(e) => {
                eprintln!("pool scan failed closed: {e:?}");
                std::process::exit(1)
            }
        }
    }
    match args[1].as_str() {
        "discover-pools" => {
            match knid_solana_core::pool_discovery::discover_pumpfun_pools(&client,address) {
                Ok(pools)=>{
                    for pool in &pools {
                        println!("pool={} dex=pump-fun owner={} owner_verified={} mint_membership_verified={}",pool.address,pool.owner_program,pool.onchain_owner_verified,pool.mint_membership_verified);
                    }
                    println!("pumpfun_pool_candidates={} source=geckoterminal_with_rpc_owner_check",pools.len());
                }
                Err(e)=>{eprintln!("pool discovery failed: {e:?}");std::process::exit(1)}
            }
        },
        "scan-pump-history" => {
            let result=knid_solana_core::backfill::scan_mint_trade_events_from(&client,address,10,2,args.get(4).map(String::as_str));
            match result {
                Ok(result)=>{
                    for t in &result.trades {
                        println!("slot={} signature={} mint={} side={} timestamp_s={} price_sol={:.14} token_raw={} sol_lamports={}",t.slot,t.signature,t.event.mint,if t.event.is_buy{"BUY"}else{"SELL"},t.event.timestamp_s,t.price_sol,t.event.token_raw,t.event.sol_lamports);
                    }
                    for interval_s in [1,30,600] {
                        let candles=knid_solana_core::sol_candles::aggregate_sol(&result.trades,interval_s)
                            .unwrap_or_else(|e|{eprintln!("candle reconstruction failed: {e:?}");std::process::exit(1)});
                        println!("tf_seconds={} observed_candles={} reconstructed_trade_count={} source_coverage=partial",interval_s,candles.len(),candles.iter().map(|c|c.trades).sum::<u64>());
                        for c in candles.iter().rev().take(3) {
                            println!("candle tf_s={} start_s={} open_sol={:.14} high_sol={:.14} low_sol={:.14} close_sol={:.14} volume_lamports={} volume_token_raw={} trades={} open_close_order_verified={}",interval_s,c.start_s,c.open_sol,c.high_sol,c.low_sol,c.close_sol,c.volume_lamports,c.volume_token_raw,c.trades,c.open_close_order_verified);
                        }
                    }
                    for sig in &result.unresolved_signatures {println!("unresolved_signature={} reason=RPC_LOG_TRUNCATED",sig);}
                    println!("scanned_signatures={} pages={} decoded_trade_events={} reached_oldest={} next_cursor={:?} coverage={} ms_execution_precision=false",result.scanned_signatures,result.pages,result.trades.len(),result.reached_oldest,result.next_cursor,result.coverage_scope);
                }
                Err(e)=>{eprintln!("historical scan failed closed: {e:?}");std::process::exit(1)}
            }
        },
        "inspect-pump-trades" => {
            let valid=bs58::decode(address).into_vec().is_ok_and(|b|b.len()==64);
            if !valid {eprintln!("invalid transaction signature");std::process::exit(2)}
            let response=client.call("getTransaction",serde_json::json!([address,{"encoding":"jsonParsed","commitment":"finalized","maxSupportedTransactionVersion":1}]));
            let events=response.and_then(|r|knid_solana_core::pumpfun_trade::decode_pumpfun_trade_logs(&r));
            match events {
                Ok(events)=>{
                    for event in &events {
                        let decimals=match client.token_snapshot(&event.mint){Ok((_,d,_))=>d,Err(e)=>{eprintln!("mint validation failed: {e:?}");std::process::exit(1)}};
                        let price=event.price_sol(decimals).unwrap_or_else(|e|{eprintln!("invalid trade price: {e:?}");std::process::exit(1)});
                        println!("mint={} side={} timestamp_s={} sol_lamports={} token_raw={} price_sol={:.14} event_index={} verified_pumpfun_event=true",event.mint,if event.is_buy{"BUY"}else{"SELL"},event.timestamp_s,event.sol_lamports,event.token_raw,price,event.event_index);
                    }
                    println!("verified_pumpfun_event_count={}",events.len());
                }
                Err(e)=>{eprintln!("Pump.fun event decode failed: {e:?}");std::process::exit(1)}
            }
        },
        "inspect-deltas" => {
            let valid=bs58::decode(address).into_vec().is_ok_and(|b|b.len()==64);
            if !valid {eprintln!("invalid transaction signature");std::process::exit(2)}
            let response=client.call("getTransaction",serde_json::json!([address,{"encoding":"json","commitment":"finalized","maxSupportedTransactionVersion":1}]));
            let deltas=response.and_then(|r|knid_solana_core::token_deltas::token_deltas(&r));
            match deltas {
                Ok(rows)=>{for row in &rows {println!("mint={} index={} before={} after={} decimals={}",row.mint,row.account_index,row.before_raw,row.after_raw,row.decimals)}println!("token_delta_count={} verified_swap=false",rows.len())},
                Err(e)=>{eprintln!("token balance extraction failed: {e:?}");std::process::exit(1)}
            }
        },
        "inspect-transaction" => match client.finalized_transaction(address) {
            Ok(Some(tx)) => println!("slot={} block_time={:?} succeeded={} fee_lamports={} account_keys={} verified_swap=false", tx.slot,tx.block_time,tx.succeeded,tx.fee_lamports,tx.account_keys.len()),
            Ok(None) => println!("transaction_unavailable=true verified_swap=false"),
            Err(e) => {eprintln!("transaction query failed: {e:?}");std::process::exit(1)}
        },
        "inspect-mint" => match client.token_snapshot(address) {
            Ok((amount, decimals, owner)) => println!(
                "mint={} raw_supply={} decimals={} owner={}",
                address, amount, decimals, owner
            ),
            Err(err) => {
                eprintln!("RPC validation failed: {err:?}");
                std::process::exit(1)
            }
        },
        "inspect-signatures" => match client.recent_signatures(address, 10) {
            Ok(rows) => {
                for row in rows {
                    println!(
                        "slot={} signature={} block_time={:?} failed={}",
                        row.slot, row.signature, row.block_time, row.has_error
                    )
                }
            }
            Err(err) => {
                eprintln!("RPC validation failed: {err:?}");
                std::process::exit(1)
            }
        },
        "inspect-history" => {
            // Bounded historical account activity. Not a complete swap history.
            let mut before: Option<String> = None;
            let mut history = knid_solana_core::history::HistoryScan::default();
            for page in 0..3 {
                let rows = match client.signatures_page(address, 10, before.as_deref()) {
                    Ok(rows) => rows,
                    Err(e) => {
                        eprintln!("history query failed: {e:?}");
                        std::process::exit(1)
                    }
                };
                let count = rows.len();
                before = match history.ingest_page(&rows, 10) {
                    Ok(next) => next,
                    Err(err) => {
                        eprintln!("invalid history page: {err:?}");
                        std::process::exit(1)
                    }
                };
                println!(
                    "page={} rows={} total={} oldest_slot={:?} before={:?}",
                    page + 1,
                    count,
                    history.unique_count(),
                    rows.last().map(|r| r.slot),
                    before
                );
                if before.is_none() {
                    break;
                }
            }
            println!("account_history_only=true verified_swaps=not_decoded");
        }
        _ => unreachable!(),
    }
}
