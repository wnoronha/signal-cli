#!/usr/bin/env python3
import argparse
import base64
import json
import os
import sys
import urllib.request
import urllib.error

DEFAULT_HOST = "http://127.0.0.1:8080"

def make_request(url, method="GET", data=None, headers=None):
    if headers is None:
        headers = {}
    
    req_data = None
    if data is not None:
        req_data = json.dumps(data).encode("utf-8")
        headers["Content-Type"] = "application/json"

    req = urllib.request.Request(url, data=req_data, headers=headers, method=method)
    
    try:
        with urllib.request.urlopen(req) as response:
            status = response.status
            body = response.read().decode("utf-8")
            try:
                parsed_body = json.loads(body)
                return status, parsed_body
            except json.JSONDecodeError:
                return status, body
    except urllib.error.HTTPError as e:
        body = e.read().decode("utf-8")
        try:
            parsed_body = json.loads(body)
            return e.code, parsed_body
        except json.JSONDecodeError:
            return e.code, body
    except urllib.error.URLError as e:
        print(f"Network Connection Error: {e.reason}", file=sys.stderr)
        print("Please check that the signal-cli daemon is running in HTTP mode.", file=sys.stderr)
        sys.exit(1)

def print_result(status, body):
    print(f"HTTP Status: {status}")
    print("Response:")
    print(json.dumps(body, indent=2) if isinstance(body, (dict, list)) else body)

def cmd_about(args):
    url = f"{args.host}/v1/about"
    print(f"Querying about info from {url}...")
    status, body = make_request(url, "GET")
    print_result(status, body)

def cmd_health(args):
    url = f"{args.host}/v1/health"
    print(f"Querying health status from {url}...")
    status, body = make_request(url, "GET")
    print_result(status, body)

def cmd_accounts(args):
    url = f"{args.host}/v1/accounts"
    print(f"Listing local accounts from {url}...")
    status, body = make_request(url, "GET")
    print_result(status, body)

def cmd_send(args):
    url = f"{args.host}/v2/send"
    attachments = []
    
    if args.attachments:
        for path in args.attachments:
            if not os.path.exists(path):
                print(f"Error: Attachment file not found: {path}", file=sys.stderr)
                sys.exit(1)
            print(f"Encoding attachment: {path}...")
            with open(path, "rb") as f:
                encoded = base64.b64encode(f.read()).decode("utf-8")
                # Simple base64 format without data prefix is fully supported
                attachments.append(encoded)
                
    payload = {
        "number": args.number,
        "recipients": args.recipients,
        "message": args.message,
    }
    if attachments:
        payload["base64_attachments"] = attachments

    print(f"Sending message from {args.number} to {args.recipients}...")
    status, body = make_request(url, "POST", payload)
    print_result(status, body)

def cmd_receive(args):
    url = f"{args.host}/v1/receive/{args.number}"
    print(f"Polling new messages for {args.number} from {url}...")
    status, body = make_request(url, "GET")
    print_result(status, body)

def cmd_receive_ws(args):
    ws_url = args.host.replace("http://", "ws://").replace("https://", "wss://")
    url = f"{ws_url}/v1/receive/{args.number}"
    
    try:
        import websocket
    except ImportError:
        print("Error: The 'websocket-client' package is required for WebSocket streaming.", file=sys.stderr)
        print("Please install it using: pip install websocket-client", file=sys.stderr)
        sys.exit(1)
        
    print(f"Connecting to WebSocket stream: {url}")
    print("Listening for real-time messages (Press Ctrl+C to stop)...")
    
    def on_message(ws, message):
        try:
            data = json.loads(message)
            print(json.dumps(data, indent=2))
        except json.JSONDecodeError:
            print(message)
            
    def on_error(ws, error):
        print(f"WebSocket Error: {error}", file=sys.stderr)
        
    def on_close(ws, close_status_code, close_msg):
        print("### WebSocket connection closed ###")

    ws = websocket.WebSocketApp(
        url,
        on_message=on_message,
        on_error=on_error,
        on_close=on_close
    )
    
    try:
        ws.run_forever()
    except KeyboardInterrupt:
        print("\nStopping WebSocket client...")

def cmd_groups(args):
    url = f"{args.host}/v1/groups/{args.number}"
    print(f"Retrieving groups for {args.number}...")
    status, body = make_request(url, "GET")
    print_result(status, body)

def cmd_register(args):
    url = f"{args.host}/v1/register/{args.number}"
    payload = {
        "use_voice": args.voice,
        "captcha": args.captcha
    }
    print(f"Requesting registration for number {args.number} (voice={args.voice})...")
    status, body = make_request(url, "POST", payload)
    print_result(status, body)

def cmd_verify(args):
    url = f"{args.host}/v1/register/{args.number}/verify/{args.code}"
    payload = {}
    if args.pin:
        payload["pin"] = args.pin
    print(f"Verifying registration for number {args.number} with code {args.code}...")
    status, body = make_request(url, "POST", payload)
    print_result(status, body)

def main():
    parser = argparse.ArgumentParser(
        description="Python utility to test the signal-cli REST API compatibility layer.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Check version & health
  ./signal-rest-test.py about
  ./signal-rest-test.py health

  # List locally configured accounts
  ./signal-rest-test.py accounts

  # Send a message
  ./signal-rest-test.py send -n +123456789 -r +987654321 -m "Hello World"

  # Send a message with an attachment
  ./signal-rest-test.py send -n +123456789 -r +987654321 -m "See attachment" -a image.png

  # Poll messages (HTTP GET)
  ./signal-rest-test.py receive -n +123456789

  # Stream messages (WebSocket)
  ./signal-rest-test.py receive-ws -n +123456789
"""
    )
    
    parser.add_argument(
        "--host", 
        default=DEFAULT_HOST, 
        help=f"Host URL of the daemon (default: {DEFAULT_HOST})"
    )

    subparsers = parser.add_subparsers(dest="command", required=True, help="REST action to perform")

    # about
    subparsers.add_parser("about", help="Get API/daemon version info")
    
    # health
    subparsers.add_parser("health", help="Get daemon health status")
    
    # accounts
    subparsers.add_parser("accounts", help="List registered local accounts")

    # send
    send_parser = subparsers.add_parser("send", help="Send a message")
    send_parser.add_argument("-n", "--number", required=True, help="Sender phone number")
    send_parser.add_argument("-r", "--recipients", required=True, nargs="+", help="Recipient phone numbers / UUIDs")
    send_parser.add_argument("-m", "--message", help="Message body")
    send_parser.add_argument("-a", "--attachments", nargs="+", help="Paths to files to attach")

    # receive (poll)
    recv_parser = subparsers.add_parser("receive", help="Poll incoming messages")
    recv_parser.add_argument("-n", "--number", required=True, help="Registered account phone number")

    # receive-ws (stream)
    ws_parser = subparsers.add_parser("receive-ws", help="Stream incoming messages via WebSocket")
    ws_parser.add_argument("-n", "--number", required=True, help="Registered account phone number")

    # groups
    groups_parser = subparsers.add_parser("groups", help="List groups for account")
    groups_parser.add_argument("-n", "--number", required=True, help="Registered account phone number")

    # register
    reg_parser = subparsers.add_parser("register", help="Request SMS/voice verification code")
    reg_parser.add_argument("-n", "--number", required=True, help="Phone number to register")
    reg_parser.add_argument("--voice", action="store_true", help="Request registration verification via voice call")
    reg_parser.add_argument("--captcha", help="Optional captcha token required by Signal server")

    # verify
    verify_parser = subparsers.add_parser("verify", help="Confirm registration code")
    verify_parser.add_argument("-n", "--number", required=True, help="Registered phone number")
    verify_parser.add_argument("-c", "--code", required=True, help="Received verification code")
    verify_parser.add_argument("--pin", help="Optional registration lock PIN")

    if len(sys.argv) == 1:
        parser.print_help(sys.stderr)
        sys.exit(1)

    args = parser.parse_args()

    # Dispatch commands
    commands = {
        "about": cmd_about,
        "health": cmd_health,
        "accounts": cmd_accounts,
        "send": cmd_send,
        "receive": cmd_receive,
        "receive-ws": cmd_receive_ws,
        "groups": cmd_groups,
        "register": cmd_register,
        "verify": cmd_verify,
    }

    commands[args.command](args)

if __name__ == "__main__":
    main()
