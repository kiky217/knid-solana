//! Bounded read-only CA subscription probe; observations are provisional, not swaps.
use futures_util::{SinkExt, StreamExt};
use knid_solana_core::rpc::validate_mint;
use knid_solana_core::ws_events::parse_logs_notification;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};

async fn run(ca: &str, url: &str, seconds: u64) -> Result<(), String> {
    let (mut socket, _) = timeout(Duration::from_secs(12), connect_async(url))
        .await
        .map_err(|_| "WS_CONNECT_TIMEOUT")?
        .map_err(|_| "WS_CONNECT_FAILED")?;
    let req = json!({"jsonrpc":"2.0","id":1,"method":"logsSubscribe",
        "params":[{"mentions":[ca]},{"commitment":"processed"}]});
    socket
        .send(Message::Text(req.to_string().into()))
        .await
        .map_err(|_| "WS_SUBSCRIBE_SEND_FAILED")?;
    let ack = timeout(Duration::from_secs(12), socket.next())
        .await
        .map_err(|_| "WS_ACK_TIMEOUT")?
        .ok_or("WS_ACK_DISCONNECTED")?
        .map_err(|_| "WS_ACK_PROTOCOL_FAILED")?;
    let Message::Text(text) = ack else {
        return Err("WS_ACK_NOT_TEXT".into());
    };
    let parsed: Value = serde_json::from_str(text.as_str()).map_err(|_| "WS_ACK_INVALID_JSON")?;
    let subscription = parsed
        .get("result")
        .and_then(Value::as_u64)
        .ok_or("WS_SUBSCRIBE_REJECTED")?;
    println!(
        "SUBSCRIBED subscription={} commitment=processed coverage=mint_mentions_only",
        subscription
    );
    let mut seen = HashSet::new();
    let mut notifications = 0_u64;
    let mut duplicates = 0_u64;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(seconds) {
        let item = match timeout(Duration::from_secs(2), socket.next()).await {
            Err(_) => continue,
            Ok(None) => return Err("WS_DISCONNECTED".into()),
            Ok(Some(Err(_))) => return Err("WS_READ_ERROR".into()),
            Ok(Some(Ok(msg))) => msg,
        };
        let Message::Text(text) = item else { continue };
        let parsed: Value =
            serde_json::from_str(text.as_str()).map_err(|_| "WS_NOTIFICATION_BAD_JSON")?;
        let observed_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "CLOCK_INVALID")?
            .as_nanos();
        match parse_logs_notification(&parsed, observed_ns) {
            Ok(Some(event)) => {
                if !seen.insert((event.signature.clone(), event.slot)) {
                    duplicates += 1;
                    continue;
                }
                notifications += 1;
                println!(
                    "{}",
                    json!({
                        "signature":event.signature,"slot":event.slot,
                        "received_at_ns":event.received_at_ns.to_string(),
                        "execution_succeeded":event.execution_succeeded,
                        "commitment":"processed","trade_verified":false,
                        "timestamp_semantics":"local_receive_time_not_block_execution"
                    })
                );
            }
            Ok(None) => {}
            Err(_) => return Err("WS_INVALID_NOTIFICATION".into()),
        }
    }
    println!(
        "STREAM_PROBE_END notifications={} duplicates={} coverage=mint_mentions_only",
        notifications, duplicates
    );
    let _ = socket.close(None).await;
    Ok(())
}
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args.len() > 4 {
        eprintln!("usage: knid-solana-stream <mint-ca> [wss-endpoint] [seconds:1..900]");
        std::process::exit(2)
    }
    let ca = &args[1];
    if validate_mint(ca).is_err() {
        eprintln!("invalid CA");
        std::process::exit(2)
    }
    let default_url = "wss://api.mainnet-beta.solana.com";
    let url = args.get(2).map(String::as_str).unwrap_or(default_url);
    if !(url.starts_with("wss://")
        || url.starts_with("ws://127.0.0.1:")
        || url.starts_with("ws://localhost:"))
        || url.contains('@')
        || url.contains('#')
    {
        eprintln!("invalid websocket endpoint");
        std::process::exit(2)
    }
    let secs = args
        .get(3)
        .map(String::as_str)
        .unwrap_or("20")
        .parse::<u64>()
        .unwrap_or_else(|_| {
            eprintln!("invalid seconds");
            std::process::exit(2)
        });
    if !(1..=900).contains(&secs) {
        eprintln!("seconds must be 1..900");
        std::process::exit(2)
    }
    if let Err(e) = run(ca, url, secs).await {
        eprintln!("{e}");
        std::process::exit(1)
    }
}
