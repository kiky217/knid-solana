//! Transaction token balance changes for decoder candidates, never proof of DEX swap.
use crate::rpc::RpcError;
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenBalanceDelta {
    pub account_index: u64,
    pub mint: String,
    pub owner: Option<String>,
    pub before_raw: u64,
    pub after_raw: u64,
    pub decimals: u8,
}
type BalanceMap = BTreeMap<(u64, String), (u64, u8, Option<String>)>;
fn balances(tx: &Value, field: &str) -> Result<BalanceMap, RpcError> {
    let meta = tx.get("meta").ok_or(RpcError::InvalidResponse)?;
    let list = meta
        .get(field)
        .and_then(Value::as_array)
        .ok_or(RpcError::InvalidResponse)?;
    if list.len() > 256 {
        return Err(RpcError::InvalidResponse);
    }
    let mut map = BTreeMap::new();
    for item in list {
        let index = item
            .get("accountIndex")
            .and_then(Value::as_u64)
            .ok_or(RpcError::InvalidResponse)?;
        let mint = item
            .get("mint")
            .and_then(Value::as_str)
            .ok_or(RpcError::InvalidResponse)?;
        let bytes = bs58::decode(mint)
            .into_vec()
            .map_err(|_| RpcError::InvalidResponse)?;
        if bytes.len() != 32 {
            return Err(RpcError::InvalidResponse);
        }
        let v = item.get("uiTokenAmount").ok_or(RpcError::InvalidResponse)?;
        let amount = v
            .get("amount")
            .and_then(Value::as_str)
            .ok_or(RpcError::InvalidResponse)?
            .parse::<u64>()
            .map_err(|_| RpcError::InvalidResponse)?;
        let decimals = v
            .get("decimals")
            .and_then(Value::as_u64)
            .ok_or(RpcError::InvalidResponse)?;
        if decimals > 18 {
            return Err(RpcError::InvalidResponse);
        }
        let owner = item.get("owner").and_then(Value::as_str).map(str::to_owned);
        if map
            .insert((index, mint.to_owned()), (amount, decimals as u8, owner))
            .is_some()
        {
            return Err(RpcError::InvalidResponse);
        }
    }
    Ok(map)
}
pub fn token_deltas(response: &Value) -> Result<Vec<TokenBalanceDelta>, RpcError> {
    let tx = response
        .get("result")
        .filter(|v| !v.is_null())
        .ok_or(RpcError::InvalidResponse)?;
    let meta = tx.get("meta").ok_or(RpcError::InvalidResponse)?;
    if !meta.get("err").is_some_and(Value::is_null) {
        return Err(RpcError::InvalidResponse);
    }
    let pre = balances(tx, "preTokenBalances")?;
    let post = balances(tx, "postTokenBalances")?;
    let mut keys = std::collections::BTreeSet::new();
    keys.extend(pre.keys().cloned());
    keys.extend(post.keys().cloned());
    let mut rows = Vec::new();
    for (index, mint) in keys {
        let p = pre.get(&(index, mint.clone()));
        let q = post.get(&(index, mint.clone()));
        let decimals = match (p, q) {
            (Some(a), Some(b)) if a.1 != b.1 => return Err(RpcError::InvalidResponse),
            (Some(a), _) => a.1,
            (_, Some(b)) => b.1,
            _ => unreachable!(),
        };
        let before_raw = p.map_or(0, |x| x.0);
        let after_raw = q.map_or(0, |x| x.0);
        if before_raw != after_raw {
            rows.push(TokenBalanceDelta {
                account_index: index,
                mint,
                owner: q
                    .and_then(|x| x.2.clone())
                    .or_else(|| p.and_then(|x| x.2.clone())),
                before_raw,
                after_raw,
                decimals,
            });
        }
    }
    Ok(rows)
}
#[cfg(test)]
mod tests {
    use super::*;
    const M: &str = "So11111111111111111111111111111111111111112";
    #[test]
    fn extracts_deltas_not_swaps() {
        let data = serde_json::json!({"result":{"meta":{"err":null,
            "preTokenBalances":[{"accountIndex":0,"mint":M,"uiTokenAmount":{"amount":"100","decimals":9}}],
            "postTokenBalances":[{"accountIndex":0,"mint":M,"uiTokenAmount":{"amount":"175","decimals":9}}]}}});
        let d = token_deltas(&data).unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].before_raw, 100);
        assert_eq!(d[0].after_raw, 175);
    }
    #[test]
    fn failed_transactions_do_not_emit() {
        let data = serde_json::json!({"result":{"meta":{"err":{"InstructionError":[0,"Custom"]},
            "preTokenBalances":[],"postTokenBalances":[]}}});
        assert_eq!(token_deltas(&data), Err(RpcError::InvalidResponse));
    }
    #[test]
    fn duplicate_accounts_fail_closed() {
        let item = serde_json::json!({"accountIndex":0,"mint":M,"uiTokenAmount":{"amount":"1","decimals":9}});
        let data = serde_json::json!({"result":{"meta":{"err":null,"preTokenBalances":[item.clone(),item],
        "postTokenBalances":[]}}});
        assert_eq!(token_deltas(&data), Err(RpcError::InvalidResponse));
    }
}
