use knid_solana_core::rpc::{validate_mint, SolanaRpc};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 || !matches!(args[1].as_str(), "inspect-mint" | "inspect-signatures") {
        eprintln!("usage: knid-solana-core <inspect-mint|inspect-signatures> <https-rpc-url> <mint-or-account-address>");
        std::process::exit(2)
    }
    let address = &args[3];
    if validate_mint(address).is_err() {
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
        _ => unreachable!(),
    }
}
