pub mod cli;
pub mod commands;

use clap::Parser;
use std::path::PathBuf;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("Error: {:?}", err);
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    let args = cli::Cli::parse();

    // 1. Resolve configuration directory
    let config_dir = if let Some(ref path) = args.config {
        path.clone()
    } else {
        let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .map_err(|_| anyhow::anyhow!("Could not determine home directory"))?;
        home.join(".local").join("share").join("signal-cli")
    };

    // Ensure config directory exists
    std::fs::create_dir_all(&config_dir)?;

    // 2. Identify available accounts
    let mut local_accounts = find_local_accounts(&config_dir)?;

    // 3. Resolve active account (if required by subcommand)
    let command_name = get_command_name(&args.command);
    let is_link_cmd = command_name == "link";
    let is_register_cmd = command_name == "register";
    let is_list_accounts_cmd = command_name == "listAccounts";
    let is_version_cmd = command_name == "version";

    let active_account = if is_link_cmd || is_list_accounts_cmd || is_version_cmd {
        // Link, listAccounts, and version commands do not require an active account to start
        None
    } else if is_register_cmd {
        // Register requires the active account to be specified via -a
        let account = args.account.clone().ok_or_else(|| {
            anyhow::anyhow!("The register command requires the --account / -a parameter to specify your phone number.")
        })?;
        if !account.starts_with('+') {
            anyhow::bail!("Account phone number must start with '+' (E.164 format).");
        }
        Some(account)
    } else if let Some(ref account) = args.account {
        if !account.starts_with('+') {
            anyhow::bail!("Account phone number must start with '+' (E.164 format).");
        }
        Some(account.clone())
    } else {
        // Check local accounts count
        match local_accounts.len() {
            0 => {
                anyhow::bail!("No registered accounts found in {:?}. Please register or link first.", config_dir);
            }
            1 => {
                let default_account = local_accounts.remove(0);
                if args.verbose {
                    eprintln!("Automatically selected active account: {}", default_account);
                }
                Some(default_account)
            }
            _ => {
                anyhow::bail!(
                    "Multiple accounts found in {:?} ({:?}). Please specify one using --account / -a.",
                    config_dir,
                    local_accounts
                );
            }
        }
    };

    // 4. Dispatch the command
    commands::execute_command(args, config_dir, active_account).await?;

    Ok(())
}

fn find_local_accounts(config_dir: &std::path::Path) -> Result<Vec<String>, anyhow::Error> {
    let mut accounts = Vec::new();
    if config_dir.exists() {
        for entry in std::fs::read_dir(config_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "db" {
                        if let Some(file_stem) = path.file_stem() {
                            let stem = file_stem.to_string_lossy().to_string();
                            if stem.starts_with('+') {
                                accounts.push(stem);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(accounts)
}

fn get_command_name(command: &cli::Command) -> &'static str {
    match command {
        cli::Command::Register { .. } => "register",
        cli::Command::Verify { .. } => "verify",
        cli::Command::Unregister { .. } => "unregister",
        cli::Command::DeleteLocalAccountData { .. } => "deleteLocalAccountData",
        cli::Command::UpdateAccount { .. } => "updateAccount",
        cli::Command::StartChangeNumber { .. } => "startChangeNumber",
        cli::Command::FinishChangeNumber { .. } => "finishChangeNumber",
        cli::Command::UpdateConfiguration { .. } => "updateConfiguration",
        cli::Command::SetPin { .. } => "setPin",
        cli::Command::RemovePin => "removePin",
        cli::Command::Link { .. } => "link",
        cli::Command::AddDevice { .. } => "addDevice",
        cli::Command::ListAccounts => "listAccounts",
        cli::Command::ListDevices => "listDevices",
        cli::Command::RemoveDevice { .. } => "removeDevice",
        cli::Command::GetUserStatus { .. } => "getUserStatus",
        cli::Command::SendAdminDelete { .. } => "sendAdminDelete",
        cli::Command::Send { .. } => "send",
        cli::Command::SendPollCreate { .. } => "sendPollCreate",
        cli::Command::SendPollVote { .. } => "sendPollVote",
        cli::Command::SendPollTerminate { .. } => "sendPollTerminate",
        cli::Command::SendMessageRequestResponse { .. } => "sendMessageRequestResponse",
        cli::Command::SendPaymentNotification { .. } => "sendPaymentNotification",
        cli::Command::SendPinMessage { .. } => "sendPinMessage",
        cli::Command::SendUnpinMessage { .. } => "sendUnpinMessage",
        cli::Command::SendReaction { .. } => "sendReaction",
        cli::Command::SendReceipt { .. } => "sendReceipt",
        cli::Command::SendTyping { .. } => "sendTyping",
        cli::Command::RemoteDelete { .. } => "remoteDelete",
        cli::Command::Receive { .. } => "receive",
        cli::Command::JoinGroup { .. } => "joinGroup",
        cli::Command::UpdateGroup { .. } => "updateGroup",
        cli::Command::QuitGroup { .. } => "quitGroup",
        cli::Command::ListGroups { .. } => "listGroups",
        cli::Command::ListContacts { .. } => "listContacts",
        cli::Command::ListIdentities { .. } => "listIdentities",
        cli::Command::Trust { .. } => "trust",
        cli::Command::UpdateProfile { .. } => "updateProfile",
        cli::Command::UpdateContact { .. } => "updateContact",
        cli::Command::UpdateDevice { .. } => "updateDevice",
        cli::Command::RemoveContact { .. } => "removeContact",
        cli::Command::Block { .. } => "block",
        cli::Command::Unblock { .. } => "unblock",
        cli::Command::SendContacts => "sendContacts",
        cli::Command::SendSyncRequest => "sendSyncRequest",
        cli::Command::UploadStickerPack { .. } => "uploadStickerPack",
        cli::Command::ListStickerPacks => "listStickerPacks",
        cli::Command::AddStickerPack { .. } => "addStickerPack",
        cli::Command::GetAttachment { .. } => "getAttachment",
        cli::Command::GetAvatar { .. } => "getAvatar",
        cli::Command::GetSticker { .. } => "getSticker",
        cli::Command::Daemon { .. } => "daemon",
        cli::Command::JsonRpc { .. } => "jsonRpc",
        cli::Command::SubmitRateLimitChallenge { .. } => "submitRateLimitChallenge",
        cli::Command::Version => "version",
    }
}
