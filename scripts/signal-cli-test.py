#!/usr/bin/env python3
import argparse
import json
import os
import socket
import sys
import urllib.request
import urllib.error

def send_rpc(args, method, params=None):
    request = {
        "jsonrpc": "2.0",
        "method": method,
        "id": 1
    }
    if params is not None:
        request["params"] = params

    payload = json.dumps(request) + "\n"

    if args.socket:
        try:
            sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            sock.connect(args.socket)
            sock.sendall(payload.encode("utf-8"))
            
            # Read single line response (JSON-RPC responses are one line per message)
            response_data = b""
            while b"\n" not in response_data:
                chunk = sock.recv(4096)
                if not chunk:
                    break
                response_data += chunk
            
            sock.close()
            line = response_data.decode("utf-8").split("\n")[0]
            return json.loads(line)
        except Exception as e:
            print(f"UNIX Socket Error: {e}", file=sys.stderr)
            sys.exit(1)

    elif args.tcp:
        try:
            host, port = args.tcp.split(":")
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.connect((host, int(port)))
            sock.sendall(payload.encode("utf-8"))
            
            response_data = b""
            while b"\n" not in response_data:
                chunk = sock.recv(4096)
                if not chunk:
                    break
                response_data += chunk
                
            sock.close()
            line = response_data.decode("utf-8").split("\n")[0]
            return json.loads(line)
        except Exception as e:
            print(f"TCP Socket Error: {e}", file=sys.stderr)
            sys.exit(1)

    elif args.http:
        url = args.http
        if not url.endswith("/api/v1/rpc"):
            if not url.startswith("http"):
                url = "http://" + url
            url = url.rstrip("/")
            if "/api/v1" not in url:
                url = f"{url}/api/v1/rpc"
                
        req = urllib.request.Request(
            url, 
            data=payload.encode("utf-8"),
            headers={"Content-Type": "application/json"},
            method="POST"
        )
        try:
            with urllib.request.urlopen(req) as response:
                body = response.read().decode("utf-8")
                return json.loads(body)
        except urllib.error.HTTPError as e:
            body = e.read().decode("utf-8")
            try:
                return json.loads(body)
            except json.JSONDecodeError:
                print(f"HTTP Error {e.code}: {body}", file=sys.stderr)
                sys.exit(1)
        except Exception as e:
            print(f"HTTP Connection Error: {e}", file=sys.stderr)
            sys.exit(1)
    else:
        print("Error: You must specify a connection method: --socket, --tcp, or --http", file=sys.stderr)
        sys.exit(1)

def print_result(result):
    if "error" in result:
        print(f"Error (Code {result['error']['code']}): {result['error']['message']}")
        if result['error'].get('data'):
            print(json.dumps(result['error']['data'], indent=2))
    else:
        print(json.dumps(result.get("result"), indent=2))

def cmd_send(args):
    params = {}
    if args.recipients:
        params["recipient"] = args.recipients
    if args.group_id:
        params["groupId"] = args.group_id
    if args.message:
        params["message"] = args.message
    if args.attachments:
        params["attachments"] = args.attachments
        
    print(f"Sending JSON-RPC 'send' request...")
    res = send_rpc(args, "send", params)
    print_result(res)

def cmd_receive(args):
    print(f"Sending JSON-RPC 'receive' request...")
    res = send_rpc(args, "receive")
    print_result(res)

def cmd_list_groups(args):
    print(f"Sending JSON-RPC 'listGroups' request...")
    res = send_rpc(args, "listGroups")
    print_result(res)

def cmd_list_contacts(args):
    print(f"Sending JSON-RPC 'listContacts' request...")
    res = send_rpc(args, "listContacts")
    print_result(res)

def cmd_update_profile(args):
    params = {}
    if args.given_name:
        params["givenName"] = args.given_name
    if args.family_name:
        params["familyName"] = args.family_name
    if args.about:
        params["about"] = args.about
    if args.about_emoji:
        params["aboutEmoji"] = args.about_emoji

    print(f"Sending JSON-RPC 'updateProfile' request...")
    res = send_rpc(args, "updateProfile", params)
    print_result(res)

def cmd_raw(args):
    try:
        raw_json = json.loads(args.request_json)
    except json.JSONDecodeError as e:
        print(f"Error: Invalid raw JSON: {e}", file=sys.stderr)
        sys.exit(1)
        
    print(f"Sending raw JSON-RPC request...")
    method = raw_json.get("method", "version")
    params = raw_json.get("params")
    res = send_rpc(args, method, params)
    print_result(res)

def main():
    parser = argparse.ArgumentParser(
        description="Python utility to test the signal-cli JSON-RPC daemon interface.",
        epilog="""
Examples:
  # Send a message via HTTP JSON-RPC endpoint
  ./scripts/signal-cli-test.py --http http://127.0.0.1:8080 send -r +987654321 -m "Hello"

  # List groups via UNIX socket
  ./scripts/signal-cli-test.py --socket /tmp/signal-cli.sock list-groups

  # Poll messages via TCP socket
  ./scripts/signal-cli-test.py --tcp 127.0.0.1:8080 receive
"""
    )
    
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--socket", help="Path to the JSON-RPC UNIX socket")
    group.add_argument("--tcp", help="Host:Port of the JSON-RPC TCP endpoint")
    group.add_argument("--http", help="Base URL or full RPC URL of the HTTP JSON-RPC endpoint (e.g. http://127.0.0.1:8080)")

    subparsers = parser.add_subparsers(dest="command", required=True, help="JSON-RPC action to perform")

    # send
    send_parser = subparsers.add_parser("send", help="Send a message")
    send_parser.add_argument("-r", "--recipients", nargs="+", help="Recipient phone numbers / UUIDs")
    send_parser.add_argument("-g", "--group-id", help="Group ID (base64 encoded)")
    send_parser.add_argument("-m", "--message", help="Message body")
    send_parser.add_argument("-a", "--attachments", nargs="+", help="Paths to attachments")

    # receive
    subparsers.add_parser("receive", help="Receive/poll incoming messages")

    # list-groups
    subparsers.add_parser("list-groups", help="List registered Signal groups")

    # list-contacts
    subparsers.add_parser("list-contacts", help="List contact profiles")

    # update-profile
    profile_parser = subparsers.add_parser("update-profile", help="Update account profile details")
    profile_parser.add_argument("--given-name", help="Given (first) name")
    profile_parser.add_argument("--family-name", help="Family (last) name")
    profile_parser.add_argument("--about", help="About status text")
    profile_parser.add_argument("--about-emoji", help="Emoji for about status")

    # raw
    raw_parser = subparsers.add_parser("raw", help="Send a raw JSON-RPC request")
    raw_parser.add_argument("request_json", help="JSON-RPC request payload string")

    if len(sys.argv) == 1:
        parser.print_help(sys.stderr)
        sys.exit(1)

    args = parser.parse_args()

    if args.command == "send" and not (args.recipients or args.group_id):
        parser.error("send command requires either -r/--recipients or -g/--group-id")

    commands = {
        "send": cmd_send,
        "receive": cmd_receive,
        "list-groups": cmd_list_groups,
        "list-contacts": cmd_list_contacts,
        "update-profile": cmd_update_profile,
        "raw": cmd_raw
    }

    commands[args.command](args)

if __name__ == "__main__":
    main()
