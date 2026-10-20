# signal-cli (Rust)

CLI for Signal. Written in Rust. Uses [presage](https://github.com/whisperfish/presage).

Work in progress. Some commands are stubs. Presage does not support them yet; they exit with an error. Use `signal-cli --version` to see your build. See GitHub releases and git tags for release history.

This port targets [AsamK/signal-cli](https://github.com/AsamK/signal-cli) CLI and JSON-RPC behavior. It does not keep compatibility with earlier Rust-only output or flags.

## Requirements

* Rust toolchain (stable)
* Debian/Ubuntu: `libsqlite3-dev`, `pkg-config`, `protobuf-compiler`, `cmake`, `libssl-dev`

## Install

```bash
git clone https://github.com/wnoronha/signal-cli
cd signal-cli
make install
```

* `make install` installs to `~/.local/bin/signal-cli`.
* Or run `cargo build --release`. Binary path: `./target/release/signal-cli`.

## Link an account

```bash
signal-cli link
```

* Scan the QR code in Signal: Settings → Linked devices.
* The QR code prints in the terminal.
* On another machine, open the printed `sgnl://` URL.

Register a new number:

```bash
signal-cli -a +1234567890 register
```

* Enter the SMS code when prompted.
* For captcha: open `https://signalcaptchas.org/registration/generate.html`, then pass `--captcha <token>`.

## Send and receive

```bash
# One account: -a is optional. Multiple accounts: -a is required.
signal-cli -a +1234567890 send -m "Hello" +0987654321
signal-cli -a +1234567890 send -m "Photo" -a ./photo.jpg +0987654321
signal-cli -a +1234567890 receive
```

* Use `--output json` for machine-readable output.
* If a number does not resolve, use the recipient UUID.
* Map UUID to number locally: `updateContact <UUID> --phone-number <NUMBER>`.

## Daemon mode (JSON-RPC)

```bash
signal-cli -a +1234567890 daemon --socket /tmp/signal.sock
signal-cli -a +1234567890 daemon --http   # 127.0.0.1:8080
```

* Test scripts: `scripts/` (example: `python3 scripts/signal-cli-test.py --socket /tmp/signal.sock`).

## Docker (sidecar / standalone)

Images publish to GHCR on `main` and tag pushes (`:latest`, `:<tag>`, `:sha-<sha>`):

```bash
docker pull ghcr.io/wnoronha/signal-cli:v0.1.3-dev
```

Run as a sidecar (single account) — the default command is `daemon --http 0.0.0.0:8080`:

```bash
docker run -d --name signal-cli \
  -e SIGNAL_ACCOUNT=+1234567890 \
  -v signal-data:/data -p 8080:8080 \
  ghcr.io/wnoronha/signal-cli:v0.1.3-dev
```

* `SIGNAL_ACCOUNT` sets the global `-a` flag. Omit it for multi-account mode (pass `account` per JSON-RPC request).
* `SIGNAL_CONFIG` overrides the config dir (default `/data`; keep it on a volume — `+<phone>.db` lives there).
* Override the command for anything else: `docker run ... <image> -a +1234567890 receive`.
* App wiring: JSON-RPC `POST http://signal-cli:8080/api/v1/rpc`, liveness `GET /api/v1/check`, push notifications via SSE `GET /api/v1/events`.

Build locally: `make docker-build` (or `docker build -t signal-cli .`).

## Command help

```bash
signal-cli --help
```

## Documentation

| Doc | Topic |
|-----|--------|
| `docs/command-status.adoc` | Implemented vs stub commands |
| `docs/daemon-rust-port.adoc` | HTTP, SSE, ignored daemon flags |
| `docs/faq.adoc` | Common questions |
| `docs/getting-started.adoc` | First-time setup |
| `docs/signal-cli-jsonrpc.5.adoc` | Daemon JSON-RPC API |
| `docs/signal-cli.1.adoc` | Full command reference |

## Data storage

* Account data: SQLite in `~/.local/share/signal-cli/`.
* File name: `+<phone>.db`.
* Override path: `--config <path>`.

## License

AGPL-3.0-only. See [LICENSE](LICENSE). Port of [AsamK/signal-cli](https://github.com/AsamK/signal-cli).
