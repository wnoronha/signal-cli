import subprocess
import json
import sys
import argparse
import socket
import threading
import time
from typing import Dict, Any, Optional

class SignalTester:
    def __init__(self, args):
        self.args = args
        self.proc = None
        self.sock = None
        self.request_id = 0

    def start(self):
        if self.args.socket:
            print(f"Connecting to UNIX socket: {self.args.socket}")
            self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            self.sock.connect(self.args.socket)
            self.reader = self.sock.makefile('r')
        elif self.args.tcp:
            host, port = self.args.tcp.split(':')
            print(f"Connecting to TCP: {host}:{port}")
            self.sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            self.sock.connect((host, int(port)))
            self.reader = self.sock.makefile('r')
        else:
            cmd = [self.args.bin, "-a", self.args.account, "jsonRpc"]
            print(f"Spawning subprocess: {' '.join(cmd)}")
            self.proc = subprocess.Popen(
                cmd,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                bufsize=1
            )
            self.reader = self.proc.stdout
            threading.Thread(target=self._log_stderr, daemon=True).start()

    def _log_stderr(self):
        for line in self.proc.stderr:
            print(f"LOG: {line.strip()}", file=sys.stderr)

    def send_request(self, method: str, params: Optional[Dict[str, Any]] = None):
        self.request_id += 1
        request = {
            "jsonrpc": "2.0",
            "method": method,
            "id": self.request_id
        }
        if params:
            request["params"] = params
        
        msg = json.dumps(request) + "\n"
        if self.sock:
            self.sock.sendall(msg.encode())
        else:
            self.proc.stdin.write(msg)
            self.proc.stdin.flush()
        print(f"SENT: {msg.strip()}")

    def run(self):
        try:
            print("Listening for messages (Ctrl+C to stop)...")
            for line in self.reader:
                line = line.strip()
                if not line: continue
                
                print(f"RECV: {line}")
                try:
                    data = json.loads(line)
                except json.JSONDecodeError:
                    continue

                if self.args.auto_reply and data.get("method") == "receive":
                    params = data.get("params", {})
                    source_uuid = params.get("sourceUuid")
                    msg_text = (params.get("dataMessage") or {}).get("message")
                    
                    if source_uuid and msg_text:
                        reply = f"Test Tool received: '{msg_text}'"
                        self.send_request("send", {"recipient": [source_uuid], "message": reply})

        except KeyboardInterrupt:
            print("\nStopping...")
        finally:
            self.cleanup()

    def cleanup(self):
        if self.sock:
            self.sock.close()
        if self.proc:
            self.proc.terminate()

def main():
    parser = argparse.ArgumentParser(description="Generalized testing tool for signal-cli (Rust)")
    parser.add_argument("-a", "--account", help="Phone number account (for stdin mode)")
    parser.add_argument("--bin", default="./target/release/signal-cli", help="Path to signal-cli binary")
    parser.add_argument("--socket", help="Path to UNIX socket")
    parser.add_argument("--tcp", help="Host:Port for TCP connection")
    parser.add_argument("--auto-reply", action="store_true", help="Automatically reply to incoming messages")
    
    args = parser.parse_args()

    if not (args.socket or args.tcp or args.account):
        parser.error("Must specify either --socket, --tcp, or --account")

    tester = SignalTester(args)
    tester.start()
    tester.run()

if __name__ == "__main__":
    main()
