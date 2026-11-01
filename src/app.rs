//! Shared application context and account-store ownership.
//!
//! `AppContext` carries CLI-wide policy (config dir, trust, attachment dir).
//! `AccountStore` is the single owner of `.db` files and cached managers.
//! Both one-shot CLI commands and the long-lived daemon use it, replacing
//! per-command store opens and the daemon-only manager cache.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context as _;
use presage::Manager;
use presage::manager::Registered;
use presage_store_sqlite::SqliteStore;
use tokio::sync::Mutex;

use crate::commands::load_registered;
use crate::domain::AccountId;
use crate::utils::{
    find_local_accounts, on_new_identity_from_trust_mode, open_sqlite_store_with_trust,
};

pub type RegisteredManager = Manager<SqliteStore, Registered>;
pub type ManagerHandle = Arc<Mutex<RegisteredManager>>;

/// CLI-wide policy resolved once at startup. Trust travels as a value.
#[derive(Debug, Clone)]
pub struct AppContext {
    pub config_dir: PathBuf,
    pub trust: crate::cli::TrustMode,
    pub attachment_dir: Option<PathBuf>,
}

impl AppContext {
    pub fn new(
        config_dir: PathBuf,
        trust: crate::cli::TrustMode,
        attachment_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            config_dir,
            trust,
            attachment_dir,
        }
    }

    pub fn store_identity(&self) -> presage::model::identity::OnNewIdentity {
        on_new_identity_from_trust_mode(self.trust)
    }
}

/// Single owner of account databases and cached managers.
pub struct AccountStore {
    pub config_dir: PathBuf,
    pub trust: crate::cli::TrustMode,
    managers: Mutex<HashMap<String, ManagerHandle>>,
}

impl AccountStore {
    pub fn new(config_dir: PathBuf, trust: crate::cli::TrustMode) -> Self {
        Self {
            config_dir,
            trust,
            managers: Mutex::new(HashMap::new()),
        }
    }

    pub fn from_context(ctx: &AppContext) -> Self {
        Self::new(ctx.config_dir.clone(), ctx.trust)
    }

    pub fn list_accounts(&self) -> anyhow::Result<Vec<AccountId>> {
        Ok(find_local_accounts(&self.config_dir)?
            .into_iter()
            .map(AccountId::from_validated)
            .collect())
    }

    /// Cached manager for long-lived use (daemon). Avoids holding the map
    /// lock across store open and network load.
    pub async fn manager_for(&self, account: &AccountId) -> anyhow::Result<ManagerHandle> {
        if let Some(existing) = self.managers.lock().await.get(account.as_str()).cloned() {
            return Ok(existing);
        }
        let store = open_sqlite_store_with_trust(
            &self.config_dir,
            account.as_str(),
            on_new_identity_from_trust_mode(self.trust),
        )
        .await
        .with_context(|| format!("Failed to open account {account}"))?;
        let manager = load_registered(store).await?;
        let handle = Arc::new(Mutex::new(manager));
        self.managers
            .lock()
            .await
            .insert(account.as_str().to_string(), handle.clone());
        Ok(handle)
    }

    /// Fresh manager for one-shot CLI commands. No caching, no background loop.
    pub async fn fresh_manager(&self, account: &AccountId) -> anyhow::Result<RegisteredManager> {
        let store = open_sqlite_store_with_trust(
            &self.config_dir,
            account.as_str(),
            on_new_identity_from_trust_mode(self.trust),
        )
        .await
        .with_context(|| format!("Failed to open account {account}"))?;
        load_registered(store).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_context_carries_trust_as_value() {
        let ctx = AppContext::new(
            PathBuf::from("/tmp/signal-test"),
            crate::cli::TrustMode::Never,
            None,
        );
        assert!(matches!(
            ctx.store_identity(),
            presage::model::identity::OnNewIdentity::Reject
        ));
    }
}
