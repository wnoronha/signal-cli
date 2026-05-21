# signal-cli (Rust)

A lightweight, drop-in replacement for the original `signal-cli`, written in Rust and powered by the [presage](https://github.com/whisperfish/presage) library.

## Project Status

**Version:** 0.1.0 (Alpha)

This project is currently in early development. It aims to provide a lightweight alternative to the Java-based `signal-cli`, focusing on performance and reduced resource usage.

### Implemented Features

*   **Account Management:**
    *   `register`: Register a new phone number (SMS/Voice).
    *   `link`: Link as a secondary device (QR code).
    *   `listAccounts`: List locally configured accounts.
    *   `deleteLocalAccountData`: Safely remove local database files.
*   **Messaging:**
    *   `send`: Send text and attachments to individuals or groups.
    *   `receive`: Fetch new messages from the server (supports plain-text and JSON output).
*   **Information Retrieval:**
    *   `getUserStatus`: Check if a number is registered on Signal.
    *   `listGroups`: List joined groups.
    *   `listContacts`: List known contacts.
    *   `listDevices`: List devices linked to an account.
*   **Profile Management:**
    *   `updateProfile`: Update your Signal name and "About" information.

### Currently Stubbed (In Progress)

Many advanced features are currently parsed by the CLI but return a "simulated success" or "not implemented" message:
*   **Security:** `unregister`, `setPin`, `removePin` (Library support pending).
*   **Advanced Messaging:** Reactions, receipts, typing indicators, poll management, remote deletes.
*   **Group Admin:** Creating/updating group settings, member management.
*   **Social:** Trusting identities, blocking/unblocking.
*   **Infrastructure:** Daemon mode, JSON-RPC interface.

## Installation

Ensure you have the Rust toolchain installed.

```bash
cargo build --release
```

## Usage

The binary acts as a drop-in replacement for `signal-cli`.

```bash
# Register a new account
./target/release/signal-cli -a +1234567890 register

# Send a message
./target/release/signal-cli -a +1234567890 send -m "Hello from Rust!" +0987654321

# Receive messages
./target/release/signal-cli -a +1234567890 receive
```

## License

This project is licensed under the same terms as `presage` (AGPL-3.0-only).
