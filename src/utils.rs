use anyhow::{Result, anyhow};
use presage::Manager;
use presage::libsignal_service::prelude::phonenumber::PhoneNumber;
use presage::libsignal_service::protocol::ServiceId;
use presage::manager::Registered;
use presage::model::identity::OnNewIdentity;
use presage::store::Store;
use presage_store_sqlite::SqliteStore;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

use crate::errors::SignalError;

/// Initialize the tracing subscriber for structured logging.
pub fn init_tracing(verbose: bool) {
    let filter = if verbose {
        EnvFilter::new("debug,presage=debug,libsignal_service=debug")
    } else {
        EnvFilter::from_default_env()
            .add_directive(tracing::Level::INFO.into())
            .add_directive("presage=warn".parse().unwrap())
            .add_directive("libsignal_service=warn".parse().unwrap())
    };

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();
}

/// Get the configuration directory, either from the provided path or the default location.
pub fn get_config_dir(args_config: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = args_config {
        Ok(path)
    } else {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .map_err(|_| anyhow!("Could not determine home directory"))?;
        Ok(home.join(".local").join("share").join("signal-cli"))
    }
}

/// Find all local accounts in the configuration directory.
/// Accounts are expected to be named `+<phone_number>.db`.
pub fn find_local_accounts(config_dir: &Path) -> Result<Vec<String>> {
    let mut accounts = Vec::new();
    if config_dir.exists() {
        for entry in std::fs::read_dir(config_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file()
                && let Some(ext) = path.extension()
                && ext == "db"
                && let Some(file_stem) = path.file_stem()
            {
                let stem = file_stem.to_string_lossy().to_string();
                if stem.starts_with('+') {
                    accounts.push(stem);
                }
            }
        }
    }
    Ok(accounts)
}

/// Open a SQLite store for the given account.
pub async fn open_sqlite_store(
    config_dir: &Path,
    account: &str,
) -> Result<SqliteStore, SignalError> {
    let db_path = config_dir.join(format!("{}.db", account));
    if !db_path.exists() {
        return Err(SignalError::AccountNotFound(account.to_string()));
    }

    info!("Opening account database: {:?}", db_path);

    let store =
        SqliteStore::open_with_passphrase(db_path.to_str().unwrap(), None, OnNewIdentity::Trust)
            .await
            .map_err(|e| SignalError::StoreError(e.to_string()))?;
    Ok(store)
}

/// Resolve a recipient string (phone number or UUID) to a ServiceId.
pub async fn resolve_recipient<S: Store + Send>(
    manager: &mut Manager<S, Registered>,
    recipient: &str,
) -> Result<ServiceId, SignalError> {
    if let Ok(uuid) = uuid::Uuid::parse_str(recipient) {
        return Ok(ServiceId::Aci(uuid.into()));
    }
    if recipient.starts_with('+') {
        if let Ok(contacts) = manager.store().contacts().await {
            for contact_res in contacts {
                if let Ok(contact) = contact_res
                    && contact.phone_number.as_ref().map(|p| p.to_string())
                        == Some(recipient.to_string())
                {
                    return Ok(ServiceId::Aci(contact.uuid.into()));
                }
            }
        }

        match PhoneNumber::from_str(recipient) {
            Ok(phone) => match manager.discover_contacts_by_phone_number(vec![phone]).await {
                Ok(discovered) => {
                    if let Some((_, Some(identity))) = discovered.into_iter().next() {
                        return Ok(identity);
                    } else {
                        return Err(SignalError::RecipientNotFound(recipient.to_string()));
                    }
                }
                Err(e) => {
                    return Err(SignalError::ServerError(format!(
                        "Discovery failed: {:?}",
                        e
                    )));
                }
            },
            Err(_) => return Err(SignalError::InvalidPhoneNumber(recipient.to_string())),
        }
    }
    Err(SignalError::RecipientNotFound(recipient.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn test_get_config_dir_provided() {
        let path = PathBuf::from("/tmp/signal-test");
        let result = get_config_dir(Some(path.clone())).unwrap();
        assert_eq!(result, path);
    }

    #[test]
    fn test_find_local_accounts() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path();

        // Create some dummy files
        File::create(dir_path.join("+1234567890.db")).unwrap();
        File::create(dir_path.join("+0987654321.db")).unwrap();
        File::create(dir_path.join("other.txt")).unwrap();
        File::create(dir_path.join("not_an_account.db")).unwrap();

        let accounts = find_local_accounts(dir_path).unwrap();
        assert_eq!(accounts.len(), 2);
        assert!(accounts.contains(&"+1234567890".to_string()));
        assert!(accounts.contains(&"+0987654321".to_string()));
    }
}
