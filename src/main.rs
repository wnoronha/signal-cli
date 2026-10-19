pub mod cli;
pub mod commands;
pub mod errors;
pub mod output;
pub mod rpc;
pub mod utils;

use clap::Parser;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    let args = cli::Cli::parse();
    utils::init_tracing(args.verbose);

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            if let Err(err) = run(args).await {
                error!("{err:#}");
                std::process::exit(1);
            }
        })
        .await;
}

async fn run(args: cli::Cli) -> anyhow::Result<()> {
    // 1. Resolve configuration directory
    let config_dir = utils::get_config_dir(args.config.clone())?;

    // Ensure config directory exists
    std::fs::create_dir_all(&config_dir)?;

    // 2. Identify available accounts
    let mut local_accounts = utils::find_local_accounts(&config_dir)?;

    // 3. Resolve active account (if required by subcommand)
    let active_account = if !args.command.requires_account() {
        // Command does not require an active account to start
        None
    } else if args.command.name() == "register" {
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
                anyhow::bail!(
                    "No registered accounts found in {:?}. Please register or link first.",
                    config_dir
                );
            }
            1 => {
                let default_account = local_accounts.remove(0);
                info!("Automatically selected active account: {}", default_account);
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
