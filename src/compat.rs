//! Warnings for Java signal-cli flags not implemented in this Rust port.

use crate::cli::{Cli, Command};

fn warn_ignored(names: &[&str]) {
    if names.is_empty() {
        return;
    }
    eprintln!(
        "warning: ignored in this Rust port (no effect): {}",
        names.join(", ")
    );
}

fn warn_receive_flags(
    timeout: &Option<i64>,
    ignore_attachments: bool,
    ignore_stories: bool,
    ignore_avatars: bool,
    ignore_stickers: bool,
    send_read_receipts: bool,
) {
    let mut names = Vec::new();
    if timeout.is_some() {
        names.push("--timeout");
    }
    if ignore_attachments {
        names.push("--ignore-attachments");
    }
    if ignore_stories {
        names.push("--ignore-stories");
    }
    if ignore_avatars {
        names.push("--ignore-avatars");
    }
    if ignore_stickers {
        names.push("--ignore-stickers");
    }
    if send_read_receipts {
        names.push("--send-read-receipts");
    }
    warn_ignored(&names);
}

#[allow(clippy::too_many_arguments)]
fn warn_json_rpc_sidecar_flags(
    ignore_attachments: bool,
    ignore_stories: bool,
    ignore_avatars: bool,
    ignore_stickers: bool,
    send_read_receipts: bool,
    receive_mode: &Option<String>,
    no_receive_stdout: bool,
    dbus: bool,
    dbus_system: bool,
    bus_name: &Option<String>,
) {
    let mut names = Vec::new();
    if ignore_attachments {
        names.push("--ignore-attachments");
    }
    if ignore_stories {
        names.push("--ignore-stories");
    }
    if ignore_avatars {
        names.push("--ignore-avatars");
    }
    if ignore_stickers {
        names.push("--ignore-stickers");
    }
    if send_read_receipts {
        names.push("--send-read-receipts");
    }
    if receive_mode.is_some() {
        names.push("--receive-mode");
    }
    if no_receive_stdout {
        names.push("--no-receive-stdout");
    }
    if dbus {
        names.push("--dbus");
    }
    if dbus_system {
        names.push("--dbus-system");
    }
    if bus_name.is_some() {
        names.push("--bus-name");
    }
    warn_ignored(&names);
}

/// Warn when global or subcommand flags are accepted but not implemented.
pub fn warn_unimplemented_cli_flags(cli: &Cli) {
    let mut global = Vec::new();
    if cli.dbus {
        global.push("global --dbus");
    }
    if cli.dbus_system {
        global.push("global --dbus-system");
    }
    if cli.bus_name.is_some() {
        global.push("global --bus-name");
    }
    if cli.disable_send_log {
        global.push("global --disable-send-log");
    }
    if cli.scrub_log {
        global.push("global --scrub-log (debug metadata logs suppressed)");
    }
    warn_ignored(&global);

    let is_daemon = matches!(
        cli.command,
        Command::Daemon { .. } | Command::JsonRpc { .. }
    );
    if (cli.dbus || cli.dbus_system) && !is_daemon {
        eprintln!(
            "warning: global --dbus / --dbus-system have no effect outside daemon mode; use `daemon --http` or `--socket` JSON-RPC"
        );
    }

    match &cli.command {
        Command::Receive {
            timeout,
            ignore_attachments,
            ignore_stories,
            ignore_avatars,
            ignore_stickers,
            send_read_receipts,
            ..
        } => warn_receive_flags(
            timeout,
            *ignore_attachments,
            *ignore_stories,
            *ignore_avatars,
            *ignore_stickers,
            *send_read_receipts,
        ),
        Command::Send {
            notify_self,
            end_session,
            edit_timestamp,
            no_urgent,
            ..
        } => {
            let mut names = Vec::new();
            if *notify_self {
                names.push("--notify-self");
            }
            if *end_session {
                names.push("--end-session (no wire representation; sent normally)");
            }
            if edit_timestamp.is_some() {
                names.push("--edit-timestamp (sync EditMessage not emitted)");
            }
            if *no_urgent {
                names.push("--no-urgent");
            }
            warn_ignored(&names);
        }
        Command::Daemon {
            ignore_attachments,
            ignore_stories,
            ignore_avatars,
            ignore_stickers,
            send_read_receipts,
            no_receive_stdout,
            receive_mode,
            dbus,
            dbus_system,
            bus_name,
            ..
        } => warn_json_rpc_sidecar_flags(
            *ignore_attachments,
            *ignore_stories,
            *ignore_avatars,
            *ignore_stickers,
            *send_read_receipts,
            receive_mode,
            *no_receive_stdout,
            *dbus,
            *dbus_system,
            bus_name,
        ),
        Command::JsonRpc {
            ignore_attachments,
            ignore_stories,
            ignore_avatars,
            ignore_stickers,
            send_read_receipts,
            receive_mode,
        } => warn_json_rpc_sidecar_flags(
            *ignore_attachments,
            *ignore_stories,
            *ignore_avatars,
            *ignore_stickers,
            *send_read_receipts,
            receive_mode,
            false,
            false,
            false,
            &None,
        ),
        Command::ListContacts {
            recipient,
            all_recipients,
            blocked,
            internal,
            ..
        } => {
            let mut names = Vec::new();
            if !recipient.is_empty() {
                names.push("--recipient (filter ignored, all contacts listed)");
            }
            if *all_recipients {
                names.push("--all-recipients (ignored)");
            }
            if blocked.is_some() {
                names.push("--blocked (ignored)");
            }
            if *internal {
                names.push("--internal (ignored)");
            }
            warn_ignored(&names);
        }
        Command::UpdateContact {
            nick_given_name,
            nick_family_name,
            note,
            ..
        } => {
            let mut names = Vec::new();
            if nick_given_name.is_some() {
                names.push("--nick-given-name (ignored)");
            }
            if nick_family_name.is_some() {
                names.push("--nick-family-name (ignored)");
            }
            if note.is_some() {
                names.push("--note (ignored)");
            }
            warn_ignored(&names);
        }
        Command::SendReaction {
            username,
            notify_self,
            story,
            ..
        } => {
            let mut names = Vec::new();
            if username.is_some() {
                names.push("--username (ignored)");
            }
            if *notify_self {
                names.push("--notify-self (ignored)");
            }
            if *story {
                names.push("--story (ignored)");
            }
            warn_ignored(&names);
        }
        Command::SendReceipt { username, .. } if username.is_some() => {
            warn_ignored(&["--username (ignored)"]);
        }
        Command::Trust {
            trust_all_known_keys,
            verified_safety_number,
            ..
        } => {
            let mut names = Vec::new();
            if *trust_all_known_keys {
                names.push("--trust-all-known-keys (ignored: stub always errors)");
            }
            if verified_safety_number.is_some() {
                names.push("--verified-safety-number (ignored: stub always errors)");
            }
            warn_ignored(&names);
        }
        Command::UpdateProfile {
            avatar,
            remove_avatar,
            mobile_coin_address,
            ..
        } => {
            let mut names = Vec::new();
            if avatar.is_some() {
                names.push("--avatar (ignored)");
            }
            if *remove_avatar {
                names.push("--remove-avatar (ignored)");
            }
            if mobile_coin_address.is_some() {
                names.push("--mobile-coin-address (ignored)");
            }
            warn_ignored(&names);
        }
        _ => {}
    }
}
