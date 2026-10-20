#!/usr/bin/env python3
"""Run CLI E2E against independent local JSON-RPC fixture, no secrets or RPC dependency."""
import json,subprocess,threading
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from pathlib import Path
ROOT=Path(__file__).parent
BIN=ROOT/"target/debug/knid-solana-core"
MINT="So11111111111111111111111111111111111111112"
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*a):pass
 def do_POST(self):
  n=int(self.headers["Content-Length"])
  request=json.loads(self.rfile.read(n))
  if request.get("method")=="getAccountInfo":
   result={"value":{"owner":"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA","executable":False}}
  elif request.get("method")=="getTokenSupply":
   result={"value":{"amount":"1000000000","decimals":9}}
  else:result=None
  data=json.dumps({"jsonrpc":"2.0","id":1,"result":result}).encode()
  self.send_response(200);self.send_header("Content-Type","application/json");self.send_header("Content-Length",str(len(data)));self.end_headers();self.wfile.write(data)
def main():
 srv=ThreadingHTTPServer(("127.0.0.1",0),Handler)
 thread=threading.Thread(target=srv.serve_forever,daemon=True);thread.start()
 try:
  url=f"http://127.0.0.1:{srv.server_port}"
  out=subprocess.run([str(BIN),"inspect-mint",url,MINT],text=True,capture_output=True,timeout=10)
  assert out.returncode==0,out.stderr
  assert "raw_supply=1000000000 decimals=9" in out.stdout,out.stdout
  bad=subprocess.run([str(BIN),"inspect-mint",url,"not-base58"],text=True,capture_output=True,timeout=10)
  assert bad.returncode==2,bad
  print("LIVE_CLI_MOCK_RPC_PASS; INVALID_MINT_FAIL_CLOSED_PASS")
 finally:srv.shutdown();srv.server_close()
if __name__=="__main__":main()
