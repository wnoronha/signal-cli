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
    ACTIVE_STORE_IDENTITY.with(|c| c.borrow().clone().unwrap_or(OnNewIdentity::Reject))
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
            .add_directive("presage=warn".parse().expect("static tracing directive"))
            .add_directive(
                "libsignal_service=warn"
                    .parse()
                    .expect("static tracing directive"),
            )
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

/// Current time as millis since UNIX_EPOCH, without panicking on clock skew.
pub fn now_millis() -> Result<u64, SignalError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .map_err(|e| SignalError::Internal(format!("system clock is before UNIX_EPOCH: {e}")))
}

/// Saturate an i64 millis timestamp (e.g. from chrono) at zero for display.
pub fn saturating_millis(v: i64) -> u64 {
    u64::try_from(v).unwrap_or(0)
}

/// Checked conversions for CLI-provided timestamps/ids (reject negatives/overflow).
pub fn u64_from_i64_nonnegative(v: i64, field: &str) -> Result<u64, SignalError> {
    u64::try_from(v).map_err(|_| SignalError::Internal(format!("{field} must be >= 0, got {v}")))
}

pub fn u32_from_i64_nonnegative(v: i64, field: &str) -> Result<u32, SignalError> {
    u32::try_from(v)
        .map_err(|_| SignalError::Internal(format!("{field} must fit in u32 and be >= 0, got {v}")))
}

pub fn i32_from_i64_checked(v: i64, field: &str) -> Result<i32, SignalError> {
    i32::try_from(v).map_err(|_| SignalError::Internal(format!("{field} must fit in i32, got {v}")))
}

/// Require an active account, mapping the missing case to `AccountNotFound`
/// (JSON-RPC `-32001`) instead of a generic internal error.
pub fn require_account(active_account: Option<String>) -> Result<String, SignalError> {
    active_account.ok_or_else(|| SignalError::AccountNotFound("(unspecified)".to_string()))
}

/// Validate an E.164-ish account string before it touches the filesystem.
/// Rejects path separators, traversal, NUL, and non-`+` names.
pub fn validate_account_name(account: &str) -> Result<(), SignalError> {
    if !account.starts_with('+') || account.len() < 2 {
        return Err(SignalError::AccountNotFound(account.to_string()));
    }
    if account.contains(['/', '\\', '\0']) || account.contains("..") || account.trim() != account {
        return Err(SignalError::AccountNotFound(account.to_string()));
    }
    Ok(())
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
    validate_account_name(account)?;
    let db_path = config_dir.join(format!("{account}.db"));
    if tokio::fs::try_exists(&db_path)
        .await
        .map_err(|e| SignalError::StoreError(e.to_string()))?
    {
        open_sqlite_at_path(&db_path).await
    } else {
        Err(SignalError::AccountNotFound(account.to_string()))
    }
}

/// Open or create the account database (registration and linking).
/// `account` may be an E.164 name or an internal `link_temp_*` name.
pub async fn open_or_create_sqlite_store(
    config_dir: &Path,
    account: &str,
) -> Result<SqliteStore, SignalError> {
    validate_db_name(account)?;
    let db_path = config_dir.join(format!("{account}.db"));
    open_sqlite_at_path(&db_path).await
}

/// Validate internal DB names (`+E.164` or `link_temp_*` with safe charset).
pub fn validate_db_name(name: &str) -> Result<(), SignalError> {
    if name.starts_with('+') {
        return validate_account_name(name);
    }
    if let Some(rest) = name.strip_prefix("link_temp")
        && (rest.is_empty() || rest.starts_with('_'))
        && !name.contains(['/', '\\', '\0'])
        && !name.contains("..")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Ok(());
    }
    Err(SignalError::AccountNotFound(name.to_string()))
}

async fn open_sqlite_at_path(db_path: &Path) -> Result<SqliteStore, SignalError> {
    let on_new_identity = active_store_identity();
    tracing::debug!("Opening account database");
    let path_str = db_path.to_str().ok_or_else(|| {
        SignalError::StoreError("account database path is not valid UTF-8".to_string())
    })?;
    SqliteStore::open_with_passphrase(path_str, None, on_new_identity)
        .await
        .map_err(|e| SignalError::StoreError(e.to_string()))
}

/// Close SQLite pool connections (presage-store-sqlite); safe before process exit.
pub async fn close_sqlite_store(store: &SqliteStore) {
    store.close().await;
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
                    // Take the first *resolved* identity, not just the first
                    // map entry (which may be `(phone, None)`).
                    if let Some(identity) = discovered.into_iter().filter_map(|(_, id)| id).next() {
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

    #[test]
    fn test_validate_account_name_rejects_traversal() {
        assert!(validate_account_name("+15551234567").is_ok());
        assert!(validate_account_name("+../../evil").is_err());
        assert!(validate_account_name("15551234567").is_err());
        assert!(validate_account_name("+/abs").is_err());
        assert!(validate_account_name("+a..b").is_err());
        assert!(validate_db_name("link_temp_123_abc").is_ok());
        assert!(validate_db_name("link_temp_../../x").is_err());
        assert!(validate_db_name("other.db").is_err());
    }

    #[test]
    fn test_checked_conversions_reject_negatives() {
        assert!(u64_from_i64_nonnegative(-1, "ts").is_err());
        assert_eq!(u64_from_i64_nonnegative(5, "ts").unwrap(), 5);
        assert!(u32_from_i64_nonnegative(-1, "id").is_err());
        assert!(i32_from_i64_checked(i64::from(i32::MAX) + 1, "opt").is_err());
        assert_eq!(saturating_millis(-10), 0);
        assert!(now_millis().is_ok());
    }
}
