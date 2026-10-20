#!/usr/bin/env python3
"""Read-only CA -> pool discovery -> provider OHLCV smoke test; not on-chain DEX swap verification."""
import argparse, json, math, urllib.request, datetime
from urllib.parse import quote

def get(url):
    req=urllib.request.Request(url,headers={"Accept":"application/json","User-Agent":"KNID-Solana-readonly-OHLC-test/1.0"})
    with urllib.request.urlopen(req,timeout=15) as r: return json.load(r)

def run(ca,limit):
    import subprocess
    mint=subprocess.run(["/content/workspace/knid-solana/target/debug/knid-solana-core","inspect-mint",
        "https://api.mainnet-beta.solana.com",ca],capture_output=True,text=True,timeout=20,check=True)
    pools=get("https://api.dexscreener.com/token-pairs/v1/solana/"+quote(ca,safe=""))
    assert isinstance(pools,list) and pools,"NO_POOLS"
    candidates=[p for p in pools if p.get("chainId")=="solana" and
        p.get("baseToken",{}).get("address")==ca or p.get("chainId")=="solana" and p.get("quoteToken",{}).get("address")==ca]
    assert candidates,"NO_MATCHING_CA_POOLS"
    candidates.sort(key=lambda p:float((p.get("liquidity") or {}).get("usd") or 0),reverse=True)
    output={"ca":ca,"mint_rpc":mint.stdout.strip(),"discovered_pools":len(candidates),"ohlcv_source":"GeckoTerminal third-party pool candles (not independently verified swaps)","results":[]}
    for p in candidates[:2]:
        pool=p["pairAddress"]
        result={"dex":p.get("dexId"),"pool":pool,"liquidity_usd":(p.get("liquidity") or {}).get("usd")}
        for timeframe,aggregate in (("1m",1),("5m",5)):
            url=f"https://api.geckoterminal.com/api/v2/networks/solana/pools/{quote(pool,safe='')}/ohlcv/minute?aggregate={aggregate}&limit={limit}&currency=usd"
            try:
                body=get(url)
                candles=body["data"]["attributes"]["ohlcv_list"]
                assert len(candles)>0,"EMPTY_CANDLES"
                timestamps=[]
                for row in candles:
                    assert len(row)==6,"INVALID_ROW_LENGTH"
                    ts,o,h,l,c,v=row
                    assert isinstance(ts,int) and ts>0 and all(isinstance(q,(float,int)) and math.isfinite(q) for q in [o,h,l,c,v]),"NONFINITE"
                    assert 0<l<=min(o,c)<=max(o,c)<=h and v>=0,"INVALID_OHLC"
                    timestamps.append(ts)
                assert timestamps==sorted(timestamps,reverse=True) and len(set(timestamps))==len(timestamps),"UNSORTED"
                assert all(x%(aggregate*60)==0 for x in timestamps),"MISALIGNED"
                result[timeframe]={"status":"PASS","candles":len(candles),"latest_utc":datetime.datetime.fromtimestamp(timestamps[0],datetime.timezone.utc).isoformat(),
                    "latest_ohlcv":candles[0]}
            except Exception as e:
                result[timeframe]={"status":"FAIL","error":str(e)[:180]}
        output["results"].append(result)
    print(json.dumps(output,indent=2))
    assert all(r[t]["status"]=="PASS" for r in output["results"] for t in ("1m","5m")),"INCOMPLETE_OHLC"
if __name__=="__main__":
    ap=argparse.ArgumentParser();ap.add_argument("--ca",default="So11111111111111111111111111111111111111112")
    ap.add_argument("--limit",type=int,default=5)
    a=ap.parse_args();assert 1<=a.limit<=20;run(a.ca,a.limit)
