//! Multi-account daemon: resolve `params.account` or CLI `-a`, cache managers per DB.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow};
use futures::channel::oneshot;
use presage::Manager;
use presage::libsignal_service::configuration::SignalServers;
use presage::manager::Registered;
use presage_store_sqlite::SqliteStore;
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::commands::{
    finalize_link_database, load_registered_and_receive, new_link_temp_db_name,
    remove_link_db_files,
};
use crate::utils::{find_local_accounts, open_or_create_sqlite_store, open_sqlite_store};

pub type RegisteredManager = Manager<SqliteStore, Registered>;
pub type ManagerHandle = Arc<Mutex<RegisteredManager>>;

pub fn account_from_params(params: &Option<Value>) -> Option<String> {
    params
        .as_ref()?
        .get("account")?
        .as_str()
        .map(str::to_string)
}

fn accountless_rpc_method(method: &str) -> bool {
    matches!(
        method,
        "listAccounts" | "startLink" | "finishLink" | "subscribeReceive" | "unsubscribeReceive"
    )
}

struct PendingLink {
    device_link_uri: String,
    done_rx: oneshot::Receiver<anyhow::Result<String>>,
}

pub struct DaemonContext {
    pub config_dir: PathBuf,
    /// Set when daemon started with `-a` (single-account mode).
    pub default_account: Option<String>,
    managers: Mutex<HashMap<String, ManagerHandle>>,
    link_session: Mutex<Option<PendingLink>>,
}

impl DaemonContext {
    pub fn new(config_dir: PathBuf, default_account: Option<String>) -> Arc<Self> {
        Arc::new(Self {
            config_dir,
            default_account,
            managers: Mutex::new(HashMap::new()),
            link_session: Mutex::new(None),
        })
    }

    pub fn accounts_for_receive(&self) -> Result<Vec<String>> {
        if let Some(account) = &self.default_account {
            return Ok(vec![account.clone()]);
        }
        find_local_accounts(&self.config_dir)
    }

    pub fn resolve_account(&self, method: &str, params: &Option<Value>) -> Result<String> {
        if accountless_rpc_method(method) {
            return Err(anyhow!("{method} does not use an account manager"));
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
        crate::utils::validate_account_name(account).map_err(|e| anyhow!(e))?;
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

    pub async fn rpc_start_link(&self, device_name: Option<String>) -> Result<Value> {
        let mut session = self.link_session.lock().await;
        if session.is_some() {
            anyhow::bail!("A link session is already in progress; call finishLink first.");
        }

        let db_name = new_link_temp_db_name();
        remove_link_db_files(&self.config_dir, &db_name).await;

        let store = open_or_create_sqlite_store(&self.config_dir, &db_name)
            .await
            .map_err(|e| anyhow!(e))?;

        let device_name = device_name.unwrap_or_else(|| "signal-cli-rust".to_string());
        let (uri_tx, uri_rx) = oneshot::channel();
        let (done_tx, done_rx) = oneshot::channel();

        let config_dir = self.config_dir.clone();
        let db_name_spawn = db_name.clone();
        tokio::task::spawn_local(async move {
            let result = async {
                let manager = Manager::link_secondary_device(
                    store,
                    SignalServers::Production,
                    device_name,
                    uri_tx,
                )
                .await?;
                let phone_number = manager.registration_data().phone_number.to_string();
                drop(manager);
                finalize_link_database(&config_dir, &db_name_spawn, &phone_number).await?;
                Ok(phone_number)
            }
            .await;
            let _ = done_tx.send(result);
        });

        let link_url = tokio::time::timeout(Duration::from_secs(120), uri_rx)
            .await
            .map_err(|_| anyhow!("Timed out waiting for link URI from Signal"))?
            .map_err(|_| anyhow!("Link provisioning channel closed before URI was sent"))?;
        let device_link_uri = link_url.to_string();

        *session = Some(PendingLink {
            device_link_uri: device_link_uri.clone(),
            done_rx,
        });

        Ok(json!({ "deviceLinkUri": device_link_uri }))
    }

    pub async fn rpc_finish_link(
        &self,
        device_link_uri: String,
        _device_name: Option<String>,
    ) -> Result<Value> {
        let mut session = self.link_session.lock().await;
        let pending = session
            .take()
            .ok_or_else(|| anyhow!("No link session in progress; call startLink first."))?;

        if pending.device_link_uri != device_link_uri {
            *session = Some(pending);
            anyhow::bail!("deviceLinkUri does not match the active startLink session");
        }

        let phone_number = tokio::time::timeout(Duration::from_secs(600), pending.done_rx)
            .await
            .map_err(|_| anyhow!("Timed out waiting for primary device to approve linking"))?
            .map_err(|_| anyhow!("Link task ended unexpectedly"))??;

        // New account may need a receive loop; existing daemon loops only cover accounts at startup.
        let _ = phone_number;

        Ok(json!({ "deviceLinkUri": device_link_uri }))
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

    #[test]
    fn resolve_account_requires_account_in_multi_account_mode() {
        let ctx = DaemonContext::new(PathBuf::from("/tmp/signal-cli-test"), None);
        let err = ctx
            .resolve_account("send", &None)
            .expect_err("expected missing account");
        assert!(err.to_string().contains("Account not specified"));
    }

    #[test]
    fn resolve_account_uses_default_when_daemon_started_with_a() {
        let ctx = DaemonContext::new(
            PathBuf::from("/tmp/signal-cli-test"),
            Some("+15551234567".into()),
        );
        assert_eq!(ctx.resolve_account("send", &None).unwrap(), "+15551234567");
    }

    #[test]
    fn list_accounts_is_accountless_method() {
        let ctx = DaemonContext::new(PathBuf::from("/tmp"), None);
        assert!(ctx.resolve_account("listAccounts", &None).is_err());
    }
}
