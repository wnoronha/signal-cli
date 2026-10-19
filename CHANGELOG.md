# Changelog

This file lists notable project changes.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).  
Versions: [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.3-dev] - Unreleased

### Added
- Daemon: multi-account JSON-RPC without `-a` (`params.account`, `listAccounts`, cached managers).
- JSON-RPC: `startLink`, `finishLink`, `addDevice`, group method stubs (`joinGroup`, `updateGroup`, `quitGroup`), `subscribeReceive` / `unsubscribeReceive` compat.
- JSON-RPC / receive: shared envelope builder (`threadTitle`, base64 `groupId`, `dataMessage.timestamp`, empty `mentions`/`contacts`).
- JSON: `listGroups` ids as base64; device fields use camelCase in JSON output.
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

### Security
- Accounts: validate `+E.164` / `link_temp_*` names before touching the filesystem (path-traversal block); non-UTF8 DB paths error instead of panic.
- Daemon: `--trust-new-identities` enforced per open (default `Reject`); previously daemon workers silently fell back to `Trust`.
- Daemon: `finishLink` accounts get a live `receive` push loop, not just a warmed cache.
- Attachments: capped at 10 files / 100 MiB, regular files only, symlinks rejected; optional `--attachment-dir` confinement (also `SIGNAL_CLI_ATTACHMENT_DIR`) with canonical-path check.
- Timestamps/ids: checked conversions (`target_timestamp`, `device_id`, `sticker_id`, `expiration`, poll options); no negative wrap, no clock-skew panic.

### Fixed
- CLI: `version`, `register`, `link`, `unregister`, `deleteLocalAccountData` render via `--output json`; progress/QR/prompts go to stderr so stdout stays JSON-clean.
- CLI: `register --verification-code` enables non-interactive verification.
- CLI: missing-account errors use `AccountNotFound` (`-32001`) consistently.
- CLI: contact phone renders as display string (no `Some(..)`); device times labeled `ms`; `receive` shows number/UUID fallback; binary attachments no longer dumped raw to stdout.
- CLI: ignored flags (`trust`, `listContacts`, `updateContact`, `sendReaction`, `sendReceipt`, `updateProfile`) warn instead of silently dropping.
- RPC: no `unwrap()` panics in HTTP/envelope paths; batch failures return per-request errors; `finishLink` params validated (`-32602`).
- Daemon: one-shot commands no longer spawn a lossy background receive loop; manager locks narrowed (clone-then-receive).
- Contacts: new contacts get an empty (not zero) profile key; discovery picks the first resolved identity; profile enrichment has a 5s per-contact timeout.
- Media: `getAttachment` scan bounded to the most recent 1000 thread messages; phone discovery picks the first resolved identity.
- Logging: OTP codes, DB paths, and message bodies redacted; `--scrub-log` suppresses debug metadata logs.
- Async: stdin and filesystem I/O moved off blocking calls (`tokio::fs`, async stdin).
- Deps: dropped unused `tokio/signal`; documented bundled SQLite and the `futures` vs `tokio` oneshot split.
- Tests: 27 passing, incl. account-validation, conversion, and command-name drift guards.

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
