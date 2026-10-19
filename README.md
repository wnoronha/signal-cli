# signal-cli (Rust)

A command-line interface for Signal, written in Rust. It uses the [presage](https://github.com/whisperfish/presage) library.

Work in progress. Some commands are stubs. Presage does not support them yet; they exit with an error. Use `signal-cli --version` to see your build. See [CHANGELOG.md](CHANGELOG.md) and git tags for release history.

## Requirements

* Rust toolchain (stable)
* `libsqlite3-dev`, `pkg-config`, `protobuf-compiler`, `cmake`, `libssl-dev` (Debian/Ubuntu)

## Install

```bash
git clone https://github.com/wnoronha/signal-cli
cd signal-cli
make install
```

This puts the binary in `~/.local/bin/signal-cli`. Or: `cargo build --release` (binary in `./target/release/`).

## Link an account

```bash
signal-cli link
```

Scan the QR code with Signal on your phone (Settings → Linked devices). The QR code prints in the terminal. If you scan from another machine, use the printed `sgnl://` URL.

To register a new number instead:

```bash
signal-cli -a +1234567890 register
```

Enter the SMS code when asked. If the server asks for a captcha, get a token at `https://signalcaptchas.org/registration/generate.html` and pass `--captcha <token>`.

## Send and receive

```bash
# One account configured: -a is optional. More than one: -a is required.
signal-cli -a +1234567890 send -m "Hello" +0987654321
signal-cli -a +1234567890 send -m "Photo" -a ./photo.jpg +0987654321
signal-cli -a +1234567890 receive
```

Use `--output json` for machine-readable output.

If a phone number does not resolve (Signal privacy settings), use the recipient's UUID. To map a UUID to a number locally: `updateContact <UUID> --phone-number <NUMBER>`.

## Daemon mode (JSON-RPC)

```bash
signal-cli -a +1234567890 daemon --socket /tmp/signal.sock
signal-cli -a +1234567890 daemon --http   # 127.0.0.1:8080
```

Test scripts are in `scripts/` (e.g. `python3 scripts/signal-cli-test.py --socket /tmp/signal.sock`).

## Full command list

```bash
signal-cli --help
```

Reference docs are in `docs/`: `signal-cli.1.adoc` (all commands), `signal-cli-jsonrpc.5.adoc` (daemon API), `getting-started.adoc`, `faq.adoc`, `command-status.adoc` (implemented vs stub), `daemon-rust-port.adoc` (HTTP/SSE and flag behavior).

## Data storage

Account databases are SQLite files in `~/.local/share/signal-cli/`, named `+<phone>.db`. Change the location with `--config <path>`.

## License

AGPL-3.0-only. See [LICENSE](LICENSE). This is a port of [AsamK/signal-cli](https://github.com/AsamK/signal-cli).
