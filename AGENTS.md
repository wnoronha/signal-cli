# Agent Instructions for signal-cli (Rust)

## Architecture Overview
This project is a Rust port of `signal-cli`. It uses the `presage` library for the Signal protocol.

- `src/cli.rs`: Command-line interface definitions and command metadata.
- `src/commands.rs`: Implementation of the commands (decoupled from output).
- `src/main.rs`: Orchestration and account selection logic.
- `src/utils.rs`: Shared utilities for path resolution, account discovery, and recipient resolution.
- `src/output.rs`: Unified output rendering logic (PlainText/JSON).
- `Makefile`: System-level orchestration (build, install, man pages).

## Command Implementation Pattern
When implementing a new command:
1.  Add the command to `src/cli.rs` and implement its metadata (`name()`, `requires_account()`).
2.  Add a match arm to `execute_command` in `src/commands.rs`.
3.  Implement a corresponding `run_<command_name>` async function that returns a type implementing `Render`.
4.  Utilize `presage::Manager<S, Registered>` for server-side operations.
5.  If the functionality is not yet supported, use the `not_implemented` helper.

## Storage
Account data is stored as SQLite databases in `~/.local/share/signal-cli/` (default) with the naming convention `+<phone_number>.db`.
Use `utils::open_sqlite_store` to access these databases.

## Established Patterns
- **Account Selection:** Handled automatically if only one account exists; otherwise requires `-a`.
- **Error Handling:** Use `anyhow` for consistent error reporting.
- **Output:** Supports `plain-text` (default) and `json`.

## Common Issues & Troubleshooting
- **Recipient Resolution Failure:** If a phone number cannot be resolved (often due to Signal privacy settings), use the recipient's UUID. 
- **Manual Contact Linking:** Use `updateContact <UUID> --phone-number <NUMBER>` to create a local mapping, allowing subsequent commands to use the phone number directly.
