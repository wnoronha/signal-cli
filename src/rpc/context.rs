//! Multi-account daemon: resolve `params.account` or CLI `-a`, cache managers per DB.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};
use presage::Manager;
use presage::manager::Registered;
use presage_store_sqlite::SqliteStore;
use serde_json::Value;
use tokio::sync::Mutex;

use crate::commands::load_registered_and_receive;
use crate::utils::{find_local_accounts, open_sqlite_store};

pub type RegisteredManager = Manager<SqliteStore, Registered>;
pub type ManagerHandle = Arc<Mutex<RegisteredManager>>;

pub fn account_from_params(params: &Option<Value>) -> Option<String> {
    params
        .as_ref()?
        .get("account")?
        .as_str()
        .map(str::to_string)
}

pub struct DaemonContext {
    pub config_dir: PathBuf,
    /// Set when daemon started with `-a` (single-account mode).
    pub default_account: Option<String>,
    managers: Mutex<HashMap<String, ManagerHandle>>,
}

impl DaemonContext {
    pub fn new(config_dir: PathBuf, default_account: Option<String>) -> Arc<Self> {
        Arc::new(Self {
            config_dir,
            default_account,
            managers: Mutex::new(HashMap::new()),
        })
    }

    pub fn accounts_for_receive(&self) -> Result<Vec<String>> {
        if let Some(account) = &self.default_account {
            return Ok(vec![account.clone()]);
        }
        find_local_accounts(&self.config_dir)
    }

    pub fn resolve_account(&self, method: &str, params: &Option<Value>) -> Result<String> {
        if method == "listAccounts" {
            return Err(anyhow!("listAccounts does not use an account manager"));
        }
        account_from_params(params)
            .or_else(|| self.default_account.clone())
            .ok_or_else(|| {
                anyhow!(
                    "Account not specified: start daemon with -a/--account or set params.account (E.164, e.g. +1234567890)"
                )
            })
    }

    pub async fn manager_for(&self, account: &str) -> Result<ManagerHandle> {
        if !account.starts_with('+') {
            anyhow::bail!("Account must be E.164 format starting with '+'");
        }
        let mut map = self.managers.lock().await;
        if let Some(existing) = map.get(account) {
            return Ok(existing.clone());
        }
        let store = open_sqlite_store(&self.config_dir, account)
            .await
            .with_context(|| format!("Failed to open account {account}"))?;
        let manager = load_registered_and_receive(store).await?;
        let handle = Arc::new(Mutex::new(manager));
        map.insert(account.to_string(), handle.clone());
        Ok(handle)
    }

    pub async fn list_accounts_json(&self) -> Result<Value> {
        let accounts = find_local_accounts(&self.config_dir)?;
        Ok(serde_json::to_value(accounts)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_from_params_reads_account_field() {
        let params = Some(serde_json::json!({ "account": "+15551234567", "message": "x" }));
        assert_eq!(
            account_from_params(&params).as_deref(),
            Some("+15551234567")
        );
        assert!(account_from_params(&None).is_none());
    }
}
