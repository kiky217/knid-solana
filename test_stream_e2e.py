#!/usr/bin/env python3
"""Deterministic E2E test for the Rust WebSocket observation stream."""
import asyncio
import json
import subprocess
from pathlib import Path
import websockets

ROOT=Path(__file__).resolve().parent
BIN=ROOT/"target/debug/knid-solana-stream"
CA="3xr8F1c8XcNJCFtzJgEAV8kNGrdJxU4sdidZFiWBpump"
SIG="1"*64

async def mock_handler(ws):
    request=json.loads(await ws.recv())
    assert request["method"]=="logsSubscribe"
    assert request["params"][0]["mentions"]==[CA]
    assert request["params"][1]["commitment"]=="processed"
    await ws.send(json.dumps({"jsonrpc":"2.0","result":999,"id":1}))
    notice={"jsonrpc":"2.0","method":"logsNotification","params":{
       "result":{"context":{"slot":123},"value":{"signature":SIG,"err":None,"logs":[]}},
       "subscription":999}}
    await ws.send(json.dumps(notice))
    await ws.send(json.dumps(notice))
    try:
        await asyncio.sleep(3)
    except websockets.exceptions.ConnectionClosed:
        pass

async def run():
    async with websockets.serve(mock_handler,"127.0.0.1",0) as server:
        port=server.sockets[0].getsockname()[1]
        proc=await asyncio.create_subprocess_exec(str(BIN),CA,f"ws://127.0.0.1:{port}","2",
            stdout=asyncio.subprocess.PIPE,stderr=asyncio.subprocess.PIPE)
        out,err=await asyncio.wait_for(proc.communicate(),timeout=12)
        out=out.decode();err=err.decode()
        assert proc.returncode==0,(out,err)
        assert "SUBSCRIBED subscription=999" in out,out
        assert "STREAM_PROBE_END notifications=1 duplicates=1" in out,out
        events=[json.loads(line) for line in out.splitlines() if line.startswith("{")]
        assert len(events)==1
        event=events[0]
        assert event["slot"]==123 and event["signature"]==SIG
        assert event["commitment"]=="processed" and event["trade_verified"] is False
        assert int(event["received_at_ns"])>0
        assert event["timestamp_semantics"]=="local_receive_time_not_block_execution"
        print("RUST_WS_MOCK_E2E_PASS; SUBSCRIPTION_PASS; DUPLICATE_SUPPRESSION_PASS; NS_OBSERVATION_PASS; UNVERIFIED_TRADE_LABEL_PASS")

if __name__=="__main__":
    asyncio.run(run())
