# Presage dependency policy

`signal-cli` depends on [presage](https://github.com/whisperfish/presage) via git:

```toml
presage = { git = "https://github.com/whisperfish/presage", features = ["cdsi"] }
```

## Reproducibility

- **Development:** `Cargo.lock` pins an exact git revision. Run `cargo update -p presage` only when intentionally upgrading.
- **Release tags:** Before tagging, record the pinned presage commit in `CHANGELOG.md` and verify `cargo test` / CI.
- **Optional:** For release branches, consider adding `rev = "<sha>"` to `Cargo.toml` so the requirement is visible without opening the lockfile.

## Upgrading

1. Update the git ref or run `cargo update -p presage -p presage-store-sqlite`.
2. Fix compile breaks and re-run the full CI checklist locally (`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`).
3. Exercise `register`, `link`, `send`, `receive`, and daemon JSON-RPC smoke tests if you have linked accounts.
