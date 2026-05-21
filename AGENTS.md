# Agent Instructions for signal-cli (Rust)

## Architecture Overview
This project is a Rust port of `signal-cli`. It uses the `presage` library for the Signal protocol.

- `src/cli.rs`: Command-line interface definitions using `clap`.
- `src/commands.rs`: Implementation of the commands.
- `src/main.rs`: Orchestration and account selection logic.

## Command Implementation Pattern
When implementing a new command:
1.  Check the match arm in `execute_command` in `src/commands.rs`.
2.  Create a corresponding `run_<command_name>` async function.
3.  Utilize `presage::Manager<S, Registered>` for server-side operations.
4.  If the functionality is not yet supported by `presage`, implement an informative stub that explains the limitation rather than a silent failure.

## Storage
Account data is stored as SQLite databases in `~/.local/share/signal-cli/` (default) with the naming convention `+<phone_number>.db`.
Use `open_sqlite_store` to access these databases.

## Established Patterns
- **Account Selection:** Handled automatically if only one account exists; otherwise requires `-a`.
- **Error Handling:** Use `anyhow` for consistent error reporting.
- **Output:** Supports `plain-text` (default) and `json`.
