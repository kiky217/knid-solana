#!/usr/bin/env python3
"""Resumable read-only mint-account backfill; durable batch evidence (not all-pool coverage)."""
import argparse
import fcntl
import json
import os
import re
import subprocess
import tempfile
from pathlib import Path

BIN=Path(__file__).parent/"target/debug/knid-solana-core"
SUMMARY=re.compile(
    r"scanned_signatures=(\d+) pages=(\d+) decoded_trade_events=(\d+) "
    r"reached_oldest=(true|false) next_cursor=(None|Some\(\"([^\"]+)\"\)) "
    r"coverage=(\S+) ms_execution_precision=false"
)
def atomic_json(path, record):
    content=json.dumps(record,sort_keys=True,indent=2).encode()+b"\n"
    fd,tmp=tempfile.mkstemp(prefix=".incoming-",dir=path.parent)
    try:
        with os.fdopen(fd,"wb") as f:
            f.write(content);f.flush();os.fsync(f.fileno())
        os.replace(tmp,path)
        dfd=os.open(path.parent,os.O_RDONLY)
        try:os.fsync(dfd)
        finally:os.close(dfd)
    finally:
        if os.path.exists(tmp):os.unlink(tmp)

def completed_pages(output, mint):
    files=sorted(output.glob("page-*.json"))
    cursor=None
    for i,path in enumerate(files,1):
        if path.name!=f"page-{i:06d}.json":
            raise ValueError("Non-contiguous checkpoint sequence")
        p=json.loads(path.read_text())
        if p.get("schema")!=1 or p.get("mint")!=mint or p.get("page")!=i:
            raise ValueError("Checkpoint identity mismatch")
        if p.get("start_cursor")!=cursor or p.get("coverage")!="mint_address_only_not_all_pools":
            raise ValueError("Checkpoint cursor/coverage mismatch")
        cursor=p.get("next_cursor")
        if i<len(files) and p.get("reached_oldest"):
            raise ValueError("Unexpected pages after history completion")
    return files,cursor

def run(mint,rpc,output,max_batches):
    output.mkdir(parents=True,exist_ok=True)
    with (output/".lock").open("a+") as lock:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        files,cursor=completed_pages(output,mint)
        if files and json.loads(files[-1].read_text())["reached_oldest"]:
            print(f"MINT_HISTORY_ALREADY_COMPLETE batches={len(files)} coverage=mint_only")
            return
        for _ in range(max_batches):
            command=[str(BIN),"scan-pump-history",rpc,mint]
            if cursor:command.append(cursor)
            result=subprocess.run(command,text=True,capture_output=True,timeout=240)
            if result.returncode!=0:
                raise RuntimeError("Backfill batch failed (checkpoint retained): "+result.stderr.strip()[:250])
            found=SUMMARY.search(result.stdout)
            if not found:raise ValueError("Missing canonical backfill summary")
            scanned,pages,events,done,_,next_cursor,coverage=found.groups()
            if coverage!="mint_address_only_not_all_pools" or (done=="false" and not next_cursor):
                raise ValueError("Incomplete or invalid backfill response")
            index=len(files)+1
            record={"schema":1,"mint":mint,"page":index,"start_cursor":cursor,
                    "next_cursor":next_cursor,"reached_oldest":done=="true",
                    "scanned_signatures":int(scanned),"rpc_pages":int(pages),
                    "decoded_trade_events":int(events),
                    "unresolved_signatures":[line.split(" ",1)[0].split("=",1)[1]
                        for line in result.stdout.splitlines() if line.startswith("unresolved_signature=")],
                    "coverage":coverage,
                    "time_precision":"block_timestamp_seconds",
                    "raw_receipt":result.stdout}
            dest=output/f"page-{index:06d}.json"
            if dest.exists():raise FileExistsError(dest)
            atomic_json(dest,record)
            print(f"CHECKPOINT_STORED batch={index} signatures={scanned} trades={events} reached_oldest={done} cursor={bool(next_cursor)}")
            files.append(dest)
            cursor=next_cursor
            if done=="true":
                print("MINT_ACCOUNT_HISTORY_EXHAUSTED; NOT_ALL_POOL_HISTORY")
                return
        print(f"PARTIAL_BACKFILL batches={len(files)} next_cursor={cursor} coverage=mint_only")

def main():
    p=argparse.ArgumentParser()
    p.add_argument("--ca",required=True)
    p.add_argument("--rpc",default="https://api.mainnet-beta.solana.com")
    p.add_argument("--output",default=None)
    p.add_argument("--max-batches",type=int,default=1)
    a=p.parse_args()
    if not 1<=a.max_batches<=100_000: p.error("max-batches must be 1..100000")
    output=Path(a.output or f"/content/workspace/.knid-solana-data/{a.ca}")
    run(a.ca,a.rpc,output,a.max_batches)
if __name__=="__main__":main()
