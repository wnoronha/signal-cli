# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.3-dev] - Unreleased

### Added
- Split `src/commands/` into domain modules (account, messaging, groups, etc.); `mod.rs` holds dispatch only.
- Command status matrix (`docs/command-status.adoc`) and daemon port notes (`docs/daemon-rust-port.adoc`).
- `--log-file` support; JSON CLI errors aligned with JSON-RPC codes.
- Split `src/commands/` modules; tests for group IDs and JSON-RPC models.
- macOS aarch64 release artifact in CI; optional `cargo audit` job.

### Fixed
- Ignored Java-compat CLI flags emit stderr warnings; stub commands no longer exit 0 silently.
- CLI stubs (`unregister`, group admin, account migration, etc.) exit non-zero instead of success with a message only.
- JSON-RPC: stub methods return library-limitation errors instead of "Account not specified".
- JSON-RPC: `unblock` routes to the unblock handler (not `block`).
- CLI: `addStickerPack` validates sticker pack URIs before install.

### Changed
- `--trust-new-identities` applies when opening account SQLite stores.
- Version output uses `CARGO_PKG_VERSION`; account list order is sorted.
- CLI errors use readable formatting on stderr.

## [0.1.1] - 2026-10-19

### Added
- Terminal QR code for `link` (no external tool needed).
- Updated to latest `presage` / `libsignal-service-rs` / `libsignal`.

### Fixed
- CI: use Node 24 action runtimes.
- Attachments: match new `CdnKey` pointers; logs go to stderr so binary output stays clean.

## [0.1.0] - 2025-05-15

### Added
- Initial Rust port of `signal-cli`.
- Account management: `register`, `link`, `listAccounts`, `deleteLocalAccountData`.
- Messaging: `send`, `receive`, `sendReaction`, `sendReceipt`, `sendTyping`, `remoteDelete`.
- Media Retrieval: `getAttachment`, `getAvatar`.
- Profile management: `updateProfile`.
- Group information: `listGroups`.
- Contact management: `listContacts`.
- JSON-RPC daemon mode (`daemon`, `jsonRpc`).
- Structured logging with `tracing`.
- Output formatting (plain-text and JSON).
- Man page generation and installation via Makefile.
