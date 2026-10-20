# Contributing to signal-cli (Rust)

Thank you for your interest in contributing to the Rust port of `signal-cli`!

## How to Help

The project is currently in early development, and there are many areas where you can help:

1.  **Bug Reports**: If you find a bug, please open an issue with detailed reproduction steps and logs (run with `-v`).
2.  **Feature Requests**: If you need a feature that is not yet implemented, see [docs/command-status.adoc](docs/command-status.adoc).
3.  **Code Contributions**:
    *   Implementing stubbed features: Many features are currently stubbed because the underlying `presage` library hasn't exposed a high-level API for them yet. If you are familiar with the Signal protocol or `presage` internals, your help here would be invaluable.
    *   Improving documentation: Man pages, README, and code comments.
    *   Adding tests: We need more unit and integration tests.

## Development Setup

1.  **Prerequisites** (match `README.md` Requirements):
    *   Rust toolchain (latest stable)
    *   `libsqlite3-dev`, `pkg-config`, `protobuf-compiler`, `cmake`, `libssl-dev` (Linux) or Homebrew equivalents on macOS
    *   `asciidoctor` (optional, for man page generation via `make install`)

2.  **Building**:
    ```bash
    make build
    ```

3.  **Formatting and Linting**:
    Please ensure your code is formatted with `cargo fmt` and passes `cargo clippy`.

    ```bash
    cargo fmt
    cargo clippy
    ```

## Licensing

By contributing to this project, you agree that your contributions will be licensed under the **AGPL-3.0-only** license.
