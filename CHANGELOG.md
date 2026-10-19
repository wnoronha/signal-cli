# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - 2026-10-19

### Added
- Terminal QR code for `link` (no external tool needed).
- Updated to latest `presage` / `libsignal-service-rs` / `libsignal`.

### Fixed
- CI: use Node 24 action runtimes.

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
