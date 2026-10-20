#!/usr/bin/env python3
"""Independent read-only RPC fixture for CA -> verified Pump.fun trade -> SOL candles."""
import base64
import hashlib
import json
import struct
import subprocess
import threading
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from pathlib import Path

ROOT=Path(__file__).parent
BIN=ROOT/"target/debug/knid-solana-core"
MINT="3xr8F1c8XcNJCFtzJgEAV8kNGrdJxU4sdidZFiWBpump"
PUMP="6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"
SIG="1"*64

def pubkey_bytes(address):
    alphabet="123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
    n=0
    for ch in address: n=n*58+alphabet.index(ch)
    return n.to_bytes(32,"big")

def pump_log():
    event=(hashlib.sha256(b"event:TradeEvent").digest()[:8]+pubkey_bytes(MINT)
        +struct.pack("<QQB",1_000_000_000,2_000_000,1)+bytes(32)
        +struct.pack("<q",1_700_000_001)+bytes(32))
    assert len(event)==129
    return "Program data: "+base64.b64encode(event).decode()

class Fixture(BaseHTTPRequestHandler):
    program=PUMP
    missing=False
    def log_message(self,*_):pass
    def do_POST(self):
        n=int(self.headers["Content-Length"])
        request=json.loads(self.rfile.read(n))
        method=request["method"]
        if method=="getAccountInfo":
            result={"value":{"owner":"TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb","executable":False}}
        elif method=="getTokenSupply":
            result={"value":{"amount":"1000000000000000","decimals":6}}
        elif method=="getSignaturesForAddress":
            result=[{"signature":SIG,"slot":123,"blockTime":1_700_000_001,"err":None}]
        elif method=="getTransaction":
            result=None if self.missing else {"slot":123,"meta":{"err":None,"logMessages":[
                f"Program {self.program} invoke [1]",pump_log(),f"Program {self.program} success"
            ]}}
        else: result=None
        data=json.dumps({"jsonrpc":"2.0","id":1,"result":result}).encode()
        self.send_response(200)
        self.send_header("Content-Type","application/json")
        self.send_header("Content-Length",str(len(data)))
        self.end_headers();self.wfile.write(data)

def main():
    server=ThreadingHTTPServer(("127.0.0.1",0),Fixture)
    thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
    try:
        command=[str(BIN),"scan-pump-history",f"http://127.0.0.1:{server.server_port}",MINT]
        good=subprocess.run(command,text=True,capture_output=True,timeout=20)
        assert good.returncode==0,good.stderr
        assert "decoded_trade_events=1" in good.stdout,good.stdout
        assert "scanned_signatures=1" in good.stdout,good.stdout
        assert "reached_oldest=true" in good.stdout,good.stdout
        assert "price_sol=0.50000000000000" in good.stdout,good.stdout
        assert "tf_seconds=1 observed_candles=1 reconstructed_trade_count=1" in good.stdout,good.stdout
        assert "tf_seconds=600 observed_candles=1 reconstructed_trade_count=1" in good.stdout,good.stdout
        Fixture.program="11111111111111111111111111111111"
        spoof=subprocess.run(command,text=True,capture_output=True,timeout=20)
        assert spoof.returncode==0,spoof.stderr
        assert "decoded_trade_events=0" in spoof.stdout,spoof.stdout
        Fixture.missing=True
        missing=subprocess.run(command,text=True,capture_output=True,timeout=20)
        assert missing.returncode==1,missing
        print("BACKFILL_FIXTURE_PASS; PUMPFUN_EVENT_DECODING_PASS; CANDLE_1S_600S_PASS; SPOOF_PROGRAM_REJECTED; MISSING_HISTORY_FAIL_CLOSED")
    finally:
        server.shutdown();server.server_close()
if __name__=="__main__":main()
