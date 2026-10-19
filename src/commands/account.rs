use anyhow::anyhow;
use futures::channel::oneshot;
use std::path::Path;
use std::str::FromStr;

use presage::Manager;
use presage::libsignal_service::configuration::SignalServers;
use presage::libsignal_service::prelude::phonenumber::PhoneNumber;
use presage::manager::{Registered, RegistrationOptions, RegistrationType};
use presage::store::Store;

use crate::errors::SignalError;
use crate::output::{AccountList, SuccessResult};
use crate::utils::{find_local_accounts, open_or_create_sqlite_store, open_sqlite_store};

use super::load_registered;

pub(crate) async fn remove_link_db_files(config_dir: &Path, db_name: &str) {
    let _ = tokio::fs::remove_file(config_dir.join(format!("{db_name}.db"))).await;
    let _ = tokio::fs::remove_file(config_dir.join(format!("{db_name}.db-wal"))).await;
    let _ = tokio::fs::remove_file(config_dir.join(format!("{db_name}.db-shm"))).await;
    let _ = tokio::fs::remove_file(config_dir.join(format!("{db_name}.db-journal"))).await;
}

pub(crate) fn new_link_temp_db_name() -> String {
    format!(
        "link_temp_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    )
}

pub(crate) async fn finalize_link_database(
    config_dir: &Path,
    db_name: &str,
    phone_number: &str,
) -> anyhow::Result<()> {
    if db_name == phone_number {
        return Ok(());
    }
    let db_path = config_dir.join(format!("{db_name}.db"));
    let final_db_path = config_dir.join(format!("{phone_number}.db"));
    if tokio::fs::try_exists(&final_db_path).await.unwrap_or(false) {
        remove_link_db_files(config_dir, phone_number).await;
    }
    tokio::fs::rename(&db_path, &final_db_path).await?;
    for suffix in ["-wal", "-shm", "-journal"] {
        let src = config_dir.join(format!("{db_name}.db{suffix}"));
        if tokio::fs::try_exists(&src).await.unwrap_or(false) {
            tokio::fs::rename(&src, config_dir.join(format!("{phone_number}.db{suffix}"))).await?;
        }
    }
    Ok(())
}

pub(crate) async fn run_list_accounts(config_dir: &Path) -> anyhow::Result<AccountList> {
    let accounts = find_local_accounts(config_dir)?;
    Ok(AccountList { accounts })
}

pub(crate) async fn run_register(
    config_dir: &Path,
    active_account: Option<String>,
    voice: bool,
    captcha: Option<String>,
    reregister: bool,
    service_env: crate::cli::ServiceEnvironment,
) -> anyhow::Result<()> {
    let account = active_account.ok_or_else(|| {
        anyhow!("Account phone number (-a / --account) must be specified for registration.")
    })?;
    let servers = match service_env {
        crate::cli::ServiceEnvironment::Live => SignalServers::Production,
        crate::cli::ServiceEnvironment::Staging => SignalServers::Staging,
    };

    let store = open_or_create_sqlite_store(config_dir, &account)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    let phone_number = PhoneNumber::from_str(&account)
        .map_err(|e| anyhow!("Invalid E.164 phone number: {:?}", e))?;

    println!("Registering account {}...", account);
    let manager = Manager::register(
        store,
        RegistrationOptions {
            signal_servers: servers,
            phone_number,
            use_voice_call: voice,
            captcha: captcha.as_deref(),
            force: reregister,
        },
    )
    .await?;

    println!("Registration successful! An SMS/voice code has been requested.");
    println!("Please enter the verification code received (followed by enter):");

    let mut code = String::new();
    {
        use tokio::io::{AsyncBufReadExt, BufReader};
        BufReader::new(tokio::io::stdin())
            .read_line(&mut code)
            .await?;
    }
    let code = code.trim();

    if code.is_empty() {
        anyhow::bail!("Verification code cannot be empty.");
    }

    println!("Verifying registration (code hidden)...");
    let registered_manager = manager.confirm_verification_code(code).await?;
    println!(
        "Successfully verified and registered account: {}",
        registered_manager.registration_data().phone_number
    );
    Ok(())
}

pub(crate) async fn run_link(
    config_dir: &Path,
    active_account: Option<String>,
    name: Option<String>,
    service_env: crate::cli::ServiceEnvironment,
) -> anyhow::Result<()> {
    let servers = match service_env {
        crate::cli::ServiceEnvironment::Live => SignalServers::Production,
        crate::cli::ServiceEnvironment::Staging => SignalServers::Staging,
    };
    let device_name = name.unwrap_or_else(|| "signal-cli-rust".to_string());

    // Use active_account if provided, otherwise use a temporary file
    let db_name = active_account.clone().unwrap_or_else(new_link_temp_db_name);
    let db_path = config_dir.join(format!("{}.db", db_name));

    if tokio::fs::try_exists(&db_path).await.unwrap_or(false) {
        remove_link_db_files(config_dir, &db_name).await;
    }

    let store = open_or_create_sqlite_store(config_dir, &db_name)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    let (provisioning_link_tx, provisioning_link_rx) = oneshot::channel();
    println!("Linking secondary device...");
    let manager_fut =
        Manager::link_secondary_device(store, servers, device_name, provisioning_link_tx);

    let link_rx_fut = async move {
        match provisioning_link_rx.await {
            Ok(url) => {
                use qrcode::QrCode;
                use qrcode::render::unicode;
                println!("Please scan the QR code or use this URL to link your device:");
                match QrCode::new(url.as_str()) {
                    Ok(code) => {
                        println!(
                            "{}",
                            code.render::<unicode::Dense1x2>()
                                .quiet_zone(false)
                                .module_dimensions(1, 1)
                                .build()
                        );
                    }
                    Err(e) => eprintln!("Failed to generate QR code: {:?}", e),
                }
                println!("{}", url);
            }
            Err(error) => {
                eprintln!("Linking was cancelled: {:?}", error);
            }
        }
    };

    let (manager_res, _) = futures::future::join(manager_fut, link_rx_fut).await;
    let manager = manager_res?;
    let phone_number = manager.registration_data().phone_number.to_string();
    println!("Successfully linked device for account: {}", phone_number);

    // Drop manager to release file locks and ensure flush
    drop(manager);

    finalize_link_database(config_dir, &db_name, &phone_number).await?;
    println!(
        "Account database saved to: {:?}",
        config_dir.join(format!("{phone_number}.db"))
    );

    Ok(())
}

pub(crate) async fn run_delete_local_account_data(
    config_dir: &std::path::Path,
    active_account: Option<String>,
    ignore_registered: bool,
) -> anyhow::Result<()> {
    let account = crate::utils::require_account(active_account)?;
    let db_path = config_dir.join(format!("{}.db", account));

    if !tokio::fs::try_exists(&db_path).await.unwrap_or(false) {
        println!("No local data found for account: {}", account);
        return Ok(());
    }

    if !ignore_registered {
        println!(
            "Warning: Deleting local data for a registered account. This will NOT unregister the account from the Signal server."
        );
        println!(
            "If you want to unregister, use the 'unregister' command (currently not implemented)."
        );
    }

    remove_link_db_files(config_dir, &account).await;
    println!("Deleted local data for account: {}", account);
    Ok(())
}

pub(crate) async fn run_unregister(
    config_dir: &std::path::Path,
    active_account: Option<String>,
    delete_account: bool,
) -> anyhow::Result<()> {
    if delete_account {
        return Err(SignalError::NotImplemented(
            "Server-side account deletion is not exposed by presage; omit --delete-account or use the primary Java client."
                .to_string(),
        )
        .into());
    }

    let account = crate::utils::require_account(active_account)?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let manager = load_registered(store).await?;

    if manager.registration_type() == RegistrationType::Primary {
        let current = manager.device_id();
        for device in manager.devices().await? {
            if device.id != current {
                manager.unlink_secondary(device.id).await?;
            }
        }
        println!(
            "Unlinked all secondary devices from this account. This primary device remains registered. Local data stays in {}; use deleteLocalAccountData to remove it.",
            config_dir.display()
        );
        return Ok(());
    }

    manager.unlink_self().await?;
    println!(
        "Unlinked this device from the account. Local data remains in {}; use deleteLocalAccountData to remove it.",
        config_dir.display()
    );
    Ok(())
}

pub(crate) async fn run_submit_rate_limit_challenge(
    config_dir: &std::path::Path,
    active_account: Option<String>,
    challenge: String,
    captcha: String,
) -> anyhow::Result<SuccessResult> {
    let account = crate::utils::require_account(active_account)?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let manager = load_registered(store).await?;
    core_submit_rate_limit_challenge(&manager, &challenge, &captcha).await
}

pub(crate) async fn core_submit_rate_limit_challenge<S: Store + Send + 'static>(
    manager: &Manager<S, Registered>,
    challenge: &str,
    captcha: &str,
) -> anyhow::Result<SuccessResult> {
    manager
        .submit_recaptcha_challenge(challenge, captcha)
        .await?;
    Ok(SuccessResult {
        success: true,
        message: "Rate limit challenge submitted.".to_string(),
    })
}

pub(crate) async fn run_set_pin(
    _config_dir: &std::path::Path,
    _active_account: Option<String>,
    _pin: String,
) -> anyhow::Result<()> {
    Err(SignalError::NotImplemented("setPin is not yet implemented.".to_string()).into())
}

pub(crate) async fn run_remove_pin(
    _config_dir: &std::path::Path,
    _active_account: Option<String>,
) -> anyhow::Result<()> {
    Err(SignalError::NotImplemented("removePin is not yet implemented.".to_string()).into())
}
