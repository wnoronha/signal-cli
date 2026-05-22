---
name: signal-cli
description: Manage Signal messenger communications, accounts, and groups. Use when needing to send/receive messages, register accounts, or integrate with Signal via JSON-RPC.
---

# signal-cli Skill

This skill provides procedural knowledge for using the Rust port of `signal-cli` to interact with the Signal messenger protocol.

## Core Commands

### Account Management
- **Register**: `signal-cli -a <NUMBER> register [--voice] [--captcha <TOKEN>]`
- **Verify**: `signal-cli -a <NUMBER> verify <CODE>`
- **Link**: `signal-cli link` (Scans QR code to link as secondary device)
- **List Accounts**: `signal-cli listAccounts`

### Messaging
- **Send**: `signal-cli [-a <SENDER>] send -m "<MESSAGE>" [-a <FILE_PATH_OR_DATA_URI>] <RECIPIENT>`
  - Supports RFC 2397 Data URIs: `data:image/png;base64,...`
- **Receive**: `signal-cli [-a <ACCOUNT>] receive`
  - Use `--output json` for machine-readable envelopes.
- **Reactions**: `signal-cli sendReaction --emoji <EMOJI> --target-author <UUID> --target-timestamp <TS> <RECIPIENT>`

### Information
- **Groups**: `signal-cli listGroups [-d]`
- **Contacts**: `signal-cli listContacts`
- **User Status**: `signal-cli getUserStatus <NUMBER_OR_UUID>`

## JSON-RPC Daemon (Integration)

Start the daemon to handle concurrent requests via sockets:

```bash
# UNIX Socket
signal-cli -a <NUMBER> daemon --socket /tmp/signal.sock

# TCP
signal-cli -a <NUMBER> daemon --tcp 127.0.0.1:7583
```

### Example RPC Request (via socat)
```bash
echo '{"jsonrpc":"2.0","method":"send","params":{"recipient":["+1234567890"],"message":"Hello"},"id":1}' | socat - UNIX-CONNECT:/tmp/signal.sock
```

### Testing & Verification
Use the included `signal-test-tool.py` to verify daemon health:
```bash
# Connect to daemon and echo all incoming messages
python3 signal-test-tool.py --socket /tmp/signal.sock --auto-reply
```

## Best Practices & Troubleshooting

### Recipient Resolution
- **Privacy**: If a phone number is not registered/discoverable, you **must** use the recipient's UUID.
- **Mapping**: Link a phone number to a UUID locally: `signal-cli updateContact <UUID> --phone-number <NUMBER>`.

### Error Handling
- **Library Limitation (-32005)**: Some management features (Group V2 admin, changing numbers) are architectural stubs awaiting upstream support.
- **Logging**: Use `-v` or `--verbose` to enable structured `tracing` logs for debugging.

### Environment
- Default config path: `~/.local/share/signal-cli/`
- Data format: SQLite databases named after the phone number (`+<number>.db`).
