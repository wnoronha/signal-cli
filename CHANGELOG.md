# Changelog

This file lists notable project changes.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).  
Versions: [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.3-dev] - Unreleased

### Added
- CLI: `whoami`, `clearSessions`, `sendSessionReset`.
- JSON-RPC: `whoami`, `clearSessions`, `sendSessionReset`, `removeStickerPack`.
- CLI: receive envelopes include `threadTitle` and `groupId`.
- Deps: presage `6f630791` (`SqliteStore::close`).
- CI: macOS aarch64 build.
- CI: optional `cargo audit` job.
- CI: releases include all architecture artifacts.
- CLI: `--log-file` for trace output.
- CLI: JSON errors use JSON-RPC error codes.
- CLI: `compat` module for Java-compat flag checks.
- CLI: stderr warnings when a flag is not implemented.
- CLI: `removeStickerPack` via presage.
- CLI: `sendSyncRequest` via presage contact sync.
- CLI: `submitRateLimitChallenge` via presage captcha submit.
- Code: command modules under `src/commands/`.
- Code: dispatch stays in `mod.rs`.
- Doc: command status matrix (`docs/command-status.adoc`).
- Doc: daemon port notes (`docs/daemon-rust-port.adoc`).
- Doc: presage API mapping (`docs/presage-deps.md`).
- Doc: README points at Java parity target.
- Tests: group ID parsing.
- Tests: JSON-RPC models.

### Fixed
- CLI: `addStickerPack` checks the sticker pack URI before install.
- CLI: `removeContact` stub exits before recipient lookup.
- CLI: stub commands exit non-zero.
- CLI: stubs use `SignalError::NotImplemented`.
- JSON-RPC: stubs call `core_*` handlers.
- JSON-RPC: stubs no longer return "Account not specified" for library limits.
- JSON-RPC: `unblock` uses the unblock handler.

### Changed
- CLI: `listContacts` JSON is a contact array (Java shape).
- CLI: detailed contacts and `getUserStatus` include profile fields when presage can fetch them.
- CLI: primary `unregister` unlinks all secondary devices via presage.
- CLI: `--scrub-log` appears in `--help`.
- CLI: `--trust-new-identities` applies when the store opens for a command.
- CLI: `--version` uses `CARGO_PKG_VERSION`.
- CLI: errors print once on stderr.
- CLI: `listAccounts` output is sorted.
- CLI: `listGroups` JSON is a group array (Java shape).
- CLI: `unregister` unlinks secondary devices via presage.
- CLI: `verify` exits non-zero with guidance.
- CLI: presage maps trust `always` to `on-first-use`.
- CLI: stub messages state when RPC params are ignored.
- CLI: trust `always` warns on start.
- Code: fewer `tokio` features enabled.
- Daemon: warns on default stdin/stdout mode.
- Doc: command status includes Presage column.
- Doc: daemon “won’t implement” compat flags.
- Doc: hidden global compat flags.
- Doc: logs are not redacted (documented).
- JSON-RPC: `listGroups` returns detailed group array.

## [0.1.2] - 2026-10-19

### Added
- CI: Linux aarch64 release binary.

### Fixed
- JSON-RPC: accepts one recipient string or an array.

## [0.1.1] - 2026-10-19

### Added
- CLI: terminal QR code for `link`.
- Deps: updated `presage`, `libsignal-service-rs`, and `libsignal`.

### Fixed
- CI: Node 24 action runtimes.
- CLI: attachments use new `CdnKey` pointers.
- CLI: binary stdout stays clean.
- CLI: logs go to stderr.

## [0.1.0] - 2025-05-15

### Added
- CLI: account commands: `register`, `link`, `listAccounts`, `deleteLocalAccountData`.
- CLI: contact commands: `listContacts`.
- CLI: group commands: `listGroups`.
- CLI: media commands: `getAttachment`, `getAvatar`.
- CLI: messaging commands: `send`, `receive`, `sendReaction`, `sendReceipt`, `sendTyping`, `remoteDelete`.
- CLI: output in plain text and JSON.
- CLI: profile commands: `updateProfile`.
- Code: structured logging with `tracing`.
- Doc: man page generation and install via Makefile.
- JSON-RPC: `daemon`, `jsonRpc`.
- Release: initial Rust port of `signal-cli`.
