use knid_solana_core::rpc::{validate_mint, SolanaRpc};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4
        || !matches!(
            args[1].as_str(),
            "inspect-mint" | "inspect-signatures" | "inspect-history" | "inspect-transaction"
        )
    {
        eprintln!("usage: knid-solana-core <inspect-mint|inspect-signatures|inspect-history|inspect-transaction> <https-rpc-url> <mint-or-account-address>");
        std::process::exit(2)
    }
    let address = &args[3];
    if args[1] != "inspect-transaction" && validate_mint(address).is_err() {
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
