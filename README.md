# signal-cli (Rust)

A lightweight, drop-in replacement for the original `signal-cli`, written in Rust and powered by the [presage](https://github.com/whisperfish/presage) library.

## Project Status

**Version:** 0.1.0 (Alpha)

This project is currently in early development. It aims to provide a lightweight alternative to the Java-based `signal-cli`, focusing on performance and reduced resource usage.

## Documentation

Comprehensive documentation is available on the [GitHub Pages site](https://<username>.github.io/<repository_name>/).

*   **[Getting Started Guide](https://<username>.github.io/<repository_name>/getting-started.html)**: A step-by-step guide for new users.
*   **[Frequently Asked Questions (FAQ)](https://<username>.github.io/<repository_name>/faq.html)**: Solutions to common issues.
*   **[CLI Man Page](https://<username>.github.io/<repository_name>/signal-cli.1.html)**: Detailed command-line reference.
*   **[JSON-RPC API](https://<username>.github.io/<repository_name>/signal-cli-jsonrpc.5.html)**: Reference for the daemon interface.

### Implemented Features

*   **Account Management:**
    *   `register`: Register a new phone number (SMS/Voice).
    *   `link`: Link as a secondary device (QR code).
    *   `listAccounts`: List locally configured accounts.
    *   `deleteLocalAccountData`: Safely remove local database files.
*   **Messaging (Core & Advanced):**
    *   `send`: Send text and attachments to individuals or groups.
    *   `receive`: Fetch new messages from the server (supports plain-text and JSON output).
    *   `sendReaction`: Send or remove an emoji reaction.
    *   `sendReceipt`: Send read or viewed receipts.
    *   `sendTyping`: Send typing indicators (start/stop).
    *   `remoteDelete`: Remotely delete a previously sent message.
*   **Media Retrieval:**
    *   `getAttachment`: Download an attachment from a specific message.
    *   `getAvatar`: Download a profile or group avatar.
*   **Information Retrieval:**
    *   `getUserStatus`: Check if a number is registered on Signal.
    *   `listGroups`: List joined groups.
    *   `listContacts`: List known contacts.
    *   `listDevices`: List devices linked to an account.
*   **Profile Management:**
    *   `updateProfile`: Update your Signal name and "About" information.
*   **Daemon Mode:**
    *   `daemon`: Start the JSON-RPC daemon. Supports UNIX sockets (`--socket`) and TCP (`--tcp`).
    *   `jsonRpc`: Alias for daemon mode using stdin/stdout.

### System & Logging
*   **Structured Logging:** Powered by `tracing`. Use `-v` or `--verbose` for debug logs.
*   **Configurable Output:** Supports `--output plain-text` (default) and `--output json`.

### Pending / Stubbed Features

Features that are parsed by the CLI but return an informative "not implemented" message due to pending library support in `presage`:
*   **Account Migration/Security:** `unregister`, `setPin`, `removePin`, `updateAccount`, `startChangeNumber`.
*   **Advanced Messaging:** Poll management (`sendPollCreate`, etc.), pinned messages (`sendPinMessage`), payment notifications.
*   **Group Admin (V2):** Creating/updating group settings (`updateGroup`), member management, joining via URI (`joinGroup`), leaving (`quitGroup`).
*   **Social & Trust:** Trusting safety numbers, blocking/unblocking, contact synchronization.
*   **Stickers:** Uploading sticker packs (`uploadStickerPack`).
*   **Infrastructure:** Admin commands, advanced device management.

**Note on Category 4 (Account & Group V2):** These features require complex server-side interactions and Zero-Knowledge proofs that are currently handled internally by the `presage` library but not yet exposed via a public high-level API. They are implemented as architectural stubs to maintain interface compatibility and will be fully enabled as the upstream library matures.

## Installation

### System Dependencies
Ensure you have the following installed on your system:
*   **Rust Toolchain**: `rustc` and `cargo` (latest stable).
*   **SQLite3 Development Headers**: e.g., `libsqlite3-dev` on Debian/Ubuntu or `sqlite` on macOS.
*   **Pkg-config**: To locate system libraries during build.

### Using Make (Recommended for Linux/macOS)

```bash
# Build and install to ~/.local/bin/signal-cli
make build
make install
```

### Using Cargo

```bash
# Build the optimized release binary
cargo build --release
# Binary will be located at ./target/release/signal-cli
```

## Usage

The binary acts as a drop-in replacement for `signal-cli`. 

### Common Commands (CLI)

```bash
# Register a new account (requires SMS verification)
signal-cli -a +1234567890 register

# Send a message with attachment
signal-cli -a +1234567890 send -m "Hello from Rust!" -a /path/to/file.png +0987654321

# Receive messages (one-time fetch)
signal-cli -a +1234567890 receive
```

### JSON-RPC (For Agents & Integrations)

The daemon supports concurrent clients via UNIX or TCP sockets.

```bash
# Start the daemon
signal-cli -a +1234567890 daemon --socket /tmp/signal.sock

# Example Request (listGroups)
echo '{"jsonrpc":"2.0","method":"listGroups","id":1}' | socat - UNIX-CONNECT:/tmp/signal.sock
```

### Testing Tool

A generalized Python testing tool is included to help verify daemon connectivity and automation logic.

```bash
# Test the daemon via UNIX socket with auto-reply enabled
python3 signal-test-tool.py --socket /tmp/signal.sock --auto-reply
```

## Credits

This project is a Rust port of the original [signal-cli](https://github.com/AsamK/signal-cli) created by **AsamK**.

It is powered by the excellent [presage](https://github.com/whisperfish/presage) library created by **Gabriel Féron**.

## License

This project is licensed under the **GNU Affero General Public License v3.0** (AGPL-3.0-only). See the [LICENSE](LICENSE) file for the full license text.

---

**Support the Signal Foundation:** If you find this tool useful, please consider [donating to Signal](https://signal.org/donate/) to help keep the service free and secure for everyone.
