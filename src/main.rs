use knid_solana_core::rpc::{validate_mint, SolanaRpc};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4
        || !matches!(
            args[1].as_str(),
            "inspect-mint"
                | "inspect-signatures"
                | "inspect-history"
                | "inspect-transaction"
                | "inspect-deltas"
                | "inspect-pump-trades"
        )
    {
        eprintln!("usage: knid-solana-core <inspect-mint|inspect-signatures|inspect-history|inspect-transaction|inspect-deltas|inspect-pump-trades> <https-rpc-url> <mint-or-account-address>");
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
    match args[1].as_str() {
        "inspect-pump-trades" => {
            let valid=bs58::decode(address).into_vec().is_ok_and(|b|b.len()==64);
            if !valid {eprintln!("invalid transaction signature");std::process::exit(2)}
            let response=client.call("getTransaction",serde_json::json!([address,{"encoding":"jsonParsed","commitment":"finalized","maxSupportedTransactionVersion":0}]));
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
            let response=client.call("getTransaction",serde_json::json!([address,{"encoding":"json","commitment":"finalized","maxSupportedTransactionVersion":0}]));
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
