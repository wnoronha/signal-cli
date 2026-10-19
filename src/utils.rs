use anyhow::{Result, anyhow};
use presage::Manager;
use presage::libsignal_service::prelude::phonenumber::PhoneNumber;
use presage::libsignal_service::proto::attachment_pointer::AttachmentIdentifier;
use presage::libsignal_service::protocol::ServiceId;
use presage::manager::Registered;
use presage::model::identity::OnNewIdentity;
use presage::store::Store;
use presage_store_sqlite::SqliteStore;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

use crate::cli::TrustMode;
use crate::errors::SignalError;

thread_local! {
    static ACTIVE_STORE_IDENTITY: RefCell<Option<OnNewIdentity>> = const { RefCell::new(None) };
}

/// RAII scope for `--trust-new-identities` while opening account stores on this thread.
pub struct StoreIdentityGuard;

impl Drop for StoreIdentityGuard {
    fn drop(&mut self) {
        ACTIVE_STORE_IDENTITY.with(|c| *c.borrow_mut() = None);
    }
}

/// Apply trust policy for subsequent [`open_sqlite_store`] calls on this thread.
pub fn push_store_identity(mode: TrustMode) -> StoreIdentityGuard {
    ACTIVE_STORE_IDENTITY.with(|c| {
        *c.borrow_mut() = Some(on_new_identity_from_trust_mode(mode));
    });
    StoreIdentityGuard
}

fn active_store_identity() -> OnNewIdentity {
    ACTIVE_STORE_IDENTITY.with(|c| c.borrow().clone().unwrap_or(OnNewIdentity::Trust))
}

/// Current store identity policy (for registration/link paths that open SQLite directly).
pub fn current_store_identity() -> OnNewIdentity {
    active_store_identity()
}

/// Initialize the tracing subscriber for structured logging.
pub fn init_tracing(verbose: bool, log_file: Option<PathBuf>, scrub_log: bool) {
    if scrub_log {
        eprintln!(
            "warning: --scrub-log is not implemented yet; log lines may contain phone numbers or message metadata when using -v"
        );
    }

    let filter = if verbose {
        EnvFilter::new("debug,presage=debug,libsignal_service=debug")
    } else {
        EnvFilter::from_default_env()
            .add_directive(tracing::Level::INFO.into())
            .add_directive("presage=warn".parse().unwrap())
            .add_directive("libsignal_service=warn".parse().unwrap())
    };

    let builder = fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false);

    if let Some(path) = log_file {
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            Ok(file) => {
                builder.with_writer(file).init();
            }
            Err(e) => {
                eprintln!("warning: could not open --log-file: {e}; using stderr");
                builder.with_writer(std::io::stderr).init();
            }
        }
    } else {
        builder.with_writer(std::io::stderr).init();
    }
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
    accounts.sort();
    Ok(accounts)
}

/// Map CLI trust policy to presage's store identity handling.
pub fn on_new_identity_from_trust_mode(mode: TrustMode) -> OnNewIdentity {
    match mode {
        TrustMode::Never => OnNewIdentity::Reject,
        TrustMode::OnFirstUse | TrustMode::Always => OnNewIdentity::Trust,
    }
}

/// Parse a Signal sticker pack share URI (`…#pack_id=…&pack_key=…`).
pub fn parse_sticker_pack_uri(uri: &str) -> Result<(String, String)> {
    let fragment = uri.split('#').nth(1).ok_or_else(|| {
        anyhow!("Invalid sticker pack URI: missing #pack_id=…&pack_key=… fragment")
    })?;
    let mut pack_id = None;
    let mut pack_key = None;
    for pair in fragment.split('&') {
        if let Some(val) = pair.strip_prefix("pack_id=") {
            pack_id = Some(val.to_string());
        } else if let Some(val) = pair.strip_prefix("pack_key=") {
            pack_key = Some(val.to_string());
        }
    }
    let pack_id = pack_id.ok_or_else(|| anyhow!("Invalid sticker pack URI: missing pack_id"))?;
    let pack_key = pack_key.ok_or_else(|| anyhow!("Invalid sticker pack URI: missing pack_key"))?;
    if pack_id.is_empty() || pack_key.is_empty() {
        anyhow::bail!("Invalid sticker pack URI: pack_id and pack_key must be non-empty");
    }
    Ok((pack_id, pack_key))
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
    open_sqlite_at_path(&db_path).await
}

/// Open or create the account database (registration and linking).
pub async fn open_or_create_sqlite_store(
    config_dir: &Path,
    account: &str,
) -> Result<SqliteStore, SignalError> {
    let db_path = config_dir.join(format!("{}.db", account));
    open_sqlite_at_path(&db_path).await
}

async fn open_sqlite_at_path(db_path: &Path) -> Result<SqliteStore, SignalError> {
    let on_new_identity = active_store_identity();
    info!("Opening account database: {:?}", db_path);
    SqliteStore::open_with_passphrase(db_path.to_str().unwrap(), None, on_new_identity)
        .await
        .map_err(|e| SignalError::StoreError(e.to_string()))
}

/// Render an attachment identifier as the plain ID string that
/// `getAttachment --id` accepts (no Rust debug wrappers).
pub fn attachment_id_str(id: &AttachmentIdentifier) -> String {
    match id {
        AttachmentIdentifier::CdnId(cid) => cid.to_string(),
        AttachmentIdentifier::CdnKey(key) => key.clone(),
    }
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

    #[test]
    fn test_find_local_accounts_sorted() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path();
        File::create(dir_path.join("+999.db")).unwrap();
        File::create(dir_path.join("+111.db")).unwrap();
        let accounts = find_local_accounts(dir_path).unwrap();
        assert_eq!(accounts, vec!["+111".to_string(), "+999".to_string()]);
    }

    #[test]
    fn test_parse_sticker_pack_uri() {
        let uri = "https://signal.art/addstickers/#pack_id=abc&pack_key=def";
        let (id, key) = parse_sticker_pack_uri(uri).unwrap();
        assert_eq!(id, "abc");
        assert_eq!(key, "def");
        assert!(parse_sticker_pack_uri("https://example.com/no-fragment").is_err());
    }

    #[test]
    fn test_on_new_identity_from_trust_mode() {
        assert!(matches!(
            on_new_identity_from_trust_mode(TrustMode::Never),
            OnNewIdentity::Reject
        ));
        assert!(matches!(
            on_new_identity_from_trust_mode(TrustMode::Always),
            OnNewIdentity::Trust
        ));
    }
}
