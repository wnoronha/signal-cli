#!/usr/bin/env python3
import argparse
import json
import sys
import socket
import threading
import time
import urllib.request
import urllib.error
import queue

try:
    import websocket
    WEBSOCKET_AVAILABLE = True
except ImportError:
    WEBSOCKET_AVAILABLE = False

class SignalClient:
    def start(self, on_message_callback): pass
    def send_message(self, recipient, message): pass
    def stop(self): pass

class JsonRpcClient(SignalClient):
    def __init__(self, sock_path=None, tcp_addr=None):
        self.sock_path = sock_path
        self.tcp_addr = tcp_addr
        self.sock = None
        self.reader = None
        self.request_id = 0
        self.running = False
        
    def start(self, on_message_callback):
        if self.sock_path:
            print(f"Connecting to UNIX socket: {self.sock_path}")
            self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            self.sock.connect(self.sock_path)
        elif self.tcp_addr:
            host, port = self.tcp_addr.split(':')
            print(f"Connecting to TCP: {host}:{port}")
            self.sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            self.sock.connect((host, int(port)))
        else:
            raise ValueError("Must provide either socket or tcp address for JSON-RPC")
            
        self.reader = self.sock.makefile('r')
        self.running = True
        
        def listen_loop():
            try:
                for line in self.reader:
                    if not self.running: break
                    line = line.strip()
                    if not line: continue
                    try:
                        data = json.loads(line)
                        if data.get("method") == "receive":
                            on_message_callback(data)
                    except json.JSONDecodeError:
                        pass
            except Exception as e:
                print(f"JSON-RPC Listener Error: {e}", file=sys.stderr)
                
        threading.Thread(target=listen_loop, daemon=True).start()

    def send_message(self, recipient, message):
        self.request_id += 1
        req = {
            "jsonrpc": "2.0",
            "method": "send",
            "params": {
                "recipient": [recipient],
                "message": message
            },
            "id": self.request_id
        }
        msg = json.dumps(req) + "\n"
        self.sock.sendall(msg.encode("utf-8"))
        print(f"SENT (RPC): to {recipient}: {message}")

    def stop(self):
        self.running = False
        if self.sock:
            self.sock.close()

class RestClient(SignalClient):
    def __init__(self, host, number):
        self.host = host
        self.number = number
        self.ws = None
        self.running = False
        
    def start(self, on_message_callback):
        if not WEBSOCKET_AVAILABLE:
            print("Error: The 'websocket-client' package is required for REST mode streaming.", file=sys.stderr)
            print("Please install it using: pip install websocket-client", file=sys.stderr)
            sys.exit(1)
            
        ws_url = self.host.replace("http://", "ws://").replace("https://", "wss://")
        url = f"{ws_url}/v1/receive/{self.number}"
        
        def on_message(ws, message):
            try:
                data = json.loads(message)
                on_message_callback(data)
            except json.JSONDecodeError:
                pass
                
        def on_error(ws, error):
            print(f"WebSocket Error: {error}", file=sys.stderr)
            
        def on_close(ws, close_status_code, close_msg):
            print("WebSocket connection closed")
            
        self.ws = websocket.WebSocketApp(
            url,
            on_message=on_message,
            on_error=on_error,
            on_close=on_close
        )
        
        self.running = True
        threading.Thread(target=self.ws.run_forever, daemon=True).start()

    def send_message(self, recipient, message):
        url = f"{self.host}/v2/send"
        payload = {
            "number": self.number,
            "recipients": [recipient],
            "message": message
        }
        req_data = json.dumps(payload).encode("utf-8")
        req = urllib.request.Request(
            url, 
            data=req_data, 
            headers={"Content-Type": "application/json"}, 
            method="POST"
        )
        try:
            with urllib.request.urlopen(req) as response:
                pass
            print(f"SENT (REST): to {recipient}: {message}")
        except Exception as e:
            print(f"REST Send Error: {e}", file=sys.stderr)

    def stop(self):
        self.running = False
        if self.ws:
            self.ws.close()

class BotManager:
    def __init__(self, client, interactive=False):
        self.client = client
        self.interactive = interactive
        self.msg_queue = queue.Queue()

    def start(self):
        self.client.start(self.queue_message)
        print("Bot started. Listening for messages...")
        
        try:
            while True:
                try:
                    data = self.msg_queue.get(timeout=1.0)
                    self.process_message(data)
                except queue.Empty:
                    pass
        except KeyboardInterrupt:
            print("\nStopping bot...")
        finally:
            self.client.stop()

    def queue_message(self, data):
        self.msg_queue.put(data)
        
    def process_message(self, data):
        print(f"DEBUG RAW MSG: {json.dumps(data)}")
        
        # Extract logic handling both RPC and REST variants effortlessly
        params = data.get("params", data)
        envelope = params.get("envelope", params)
        
        sender = envelope.get("sourceUuid") or envelope.get("source")
        data_message = envelope.get("dataMessage") or envelope.get("syncMessage", {}).get("sentMessage") or {}
        msg_text = data_message.get("message")
            
        if sender and msg_text:
            print(f"\n[INCOMING] From {sender}: {msg_text}")
            
            if self.interactive:
                reply = input(f"Reply to {sender}> ")
                if reply.strip():
                    self.client.send_message(sender, reply.strip())
            else:
                reply = f"Bot echo: {msg_text}"
                self.client.send_message(sender, reply)

def main():
    parser = argparse.ArgumentParser(
        description="Signal Bot Tool - Test dual integrations (JSON-RPC and REST) via persistent bot logic.",
        epilog="""
Examples:
  # Test JSON-RPC via UNIX socket with auto-reply
  ./signal-bot-tool.py --mode rpc --socket /tmp/signal-cli.sock

  # Test HTTP REST / WebSockets with interactive terminal replies
  ./signal-bot-tool.py --mode rest --host http://127.0.0.1:8080 --account +123456789 --interactive
""",
        formatter_class=argparse.RawDescriptionHelpFormatter
    )
    
    parser.add_argument("--mode", choices=["rpc", "rest"], required=True, help="Protocol mode to use")
    parser.add_argument("--interactive", action="store_true", help="Prompt terminal for manual replies instead of auto-reply")
    
    # RPC Args
    parser.add_argument("--socket", help="Path to UNIX socket (for RPC mode)")
    parser.add_argument("--tcp", help="Host:Port for TCP connection (for RPC mode)")
    
    # REST Args
    parser.add_argument("--host", default="http://127.0.0.1:8080", help="HTTP host for REST mode")
    parser.add_argument("--account", help="Registered account number (Required for REST mode)")
    
    if len(sys.argv) == 1:
        parser.print_help(sys.stderr)
        sys.exit(1)
        
    args = parser.parse_args()

    client = None
    if args.mode == "rpc":
        if not (args.socket or args.tcp):
            parser.error("RPC mode requires either --socket or --tcp")
        client = JsonRpcClient(sock_path=args.socket, tcp_addr=args.tcp)
    elif args.mode == "rest":
        if not args.account:
            parser.error("REST mode requires --account")
        client = RestClient(host=args.host, number=args.account)

    bot = BotManager(client, interactive=args.interactive)
    bot.start()

if __name__ == "__main__":
    main()
