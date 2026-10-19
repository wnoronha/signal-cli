# Presage dependency policy

`signal-cli` depends on [presage](https://github.com/whisperfish/presage) via git:

```toml
presage = { git = "https://github.com/whisperfish/presage", features = ["cdsi"] }
```

Rust signal-cli is a **translator**: Java-compatible CLI and JSON-RPC on top of presage. When presage gains API, wire it here (`core_*` → RPC → CLI).

## Reproducibility

- **Development:** `Cargo.lock` pins an exact git revision. Run `cargo update -p presage` only when intentionally upgrading.
- **Release tags:** Before tagging, note the pinned presage commit in the GitHub release notes and verify `cargo test` / CI.
- **Optional:** For release branches, consider adding `rev = "<sha>"` to `Cargo.toml` so the requirement is visible without opening the lockfile.

## Upgrading

1. Update the git ref or run `cargo update -p presage -p presage-store-sqlite`.
2. Fix compile breaks and re-run the full CI checklist locally (`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`).
3. Exercise `register`, `link`, `send`, `receive`, and daemon JSON-RPC smoke tests if you have linked accounts.
4. Diff `presage/src/manager/registered.rs` public methods and `presage-cli` subcommands; update the mapping below and `docs/command-status.adoc`.

## Presage → signal-cli mapping

Status: **wired** = implemented in this repo; **stub** = CLI/RPC exists, blocked or not wired; **n/a** = not a presage concern.

| Presage API (representative) | Java surface | Rust status |
|-----------------------------|--------------|-------------|
| `Manager::register`, `confirm_verification_code` | `register`, `verify` | wired (`verify` redirects to register flow) |
| `link_secondary_device`, `link_secondary`, `unlink_secondary` | `link`, `addDevice`, `removeDevice` | wired |
| `unlink_self`, `unlink_secondary`, `devices` | `unregister`, `removeDevice` | wired (primary unlinks secondaries; secondary uses `unlink_self`) |
| `submit_recaptcha_challenge` | `submitRateLimitChallenge` | wired |
| `whoami` | `whoami` | wired (CLI + JSON-RPC) |
| `clear_sessions`, `send_session_reset` | `clearSessions`, `sendSessionReset` | wired |
| `retrieve_profile_by_uuid` | `listContacts`, `getUserStatus` (detailed) | wired (profile fields when profile key known) |
| `thread_title` | `receive` envelopes | wired (`threadTitle`, `groupId`) |
| `send_message`, `send_message_to_group`, `receive_messages` | `send`, `receive`, daemon | wired (many flags compat-only) |
| `request_contacts` | `sendSyncRequest` | wired |
| `install_sticker_pack`, `remove_sticker_pack`, sticker getters | `addStickerPack`, `removeStickerPack`, `listStickerPacks`, `getSticker` | wired |
| `update_profile`, `save_contact`, store contacts/groups | `updateProfile`, `updateContact`, `listContacts`, `listGroups` | wired / partial JSON |
| `discover_contacts_by_phone_number`, `lookup_username` | `getUserStatus` | wired |
| Group join/admin/leave | `joinGroup`, `updateGroup`, `quitGroup` | stub (not on `Manager`) |
| Block list | `block`, `unblock` | stub |
| Account delete / PIN / settings | `unregister --delete-account`, `setPin`, `updateAccount`, … | stub |
| `upload_sticker_pack` (CDN) | `uploadStickerPack` | stub |
| Polls / payments / pin messages | various `send*` | stub |
| Manual identity trust | `listIdentities`, `trust` | stub (`OnNewIdentity` on store open only) |
| D-Bus daemon | `--dbus` | n/a (JSON-RPC port) |

Reference implementation for library behavior: **`presage-cli`** in the same repository.
