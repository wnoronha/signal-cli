//! Multi-account daemon: resolve `params.account` or CLI `-a`, cache managers per DB.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow};
use futures::{StreamExt, channel::oneshot, pin_mut};
use presage::Manager;
use presage::libsignal_service::configuration::SignalServers;
use presage::manager::Registered;
use presage::model::messages::Received;
use presage_store_sqlite::SqliteStore;
use serde_json::{Value, json};
use tokio::sync::{Mutex, broadcast};
use tracing::warn;

use crate::commands::{
    build_receive_envelope, finalize_link_database, load_registered_and_receive,
    new_link_temp_db_name, remove_link_db_files,
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
    trust_mode: crate::cli::TrustMode,
    managers: Mutex<HashMap<String, ManagerHandle>>,
    link_session: Mutex<Option<PendingLink>>,
    notifier: Mutex<Option<broadcast::Sender<String>>>,
}

impl DaemonContext {
    pub fn new(
        config_dir: PathBuf,
        default_account: Option<String>,
        trust_mode: crate::cli::TrustMode,
    ) -> Arc<Self> {
        Arc::new(Self {
            config_dir,
            default_account,
            trust_mode,
            managers: Mutex::new(HashMap::new()),
            link_session: Mutex::new(None),
            notifier: Mutex::new(None),
        })
    }

    /// Register the broadcast channel used for `receive` push notifications.
    /// Called once at daemon startup so later-linked accounts get live loops.
    pub async fn set_notifier(&self, tx: broadcast::Sender<String>) {
        *self.notifier.lock().await = Some(tx);
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
        // Enforce --trust-new-identities on this thread even when the caller
        // runs on a different Tokio worker (HTTP) than the CLI entry thread.
        let _guard = crate::utils::push_store_identity(self.trust_mode);
        // Avoid holding the map lock across store open / network load.
        if let Some(existing) = self.managers.lock().await.get(account).cloned() {
            return Ok(existing);
        }
        let store = open_sqlite_store(&self.config_dir, account)
            .await
            .with_context(|| format!("Failed to open account {account}"))?;
        let manager = load_registered_and_receive(store).await?;
        let handle = Arc::new(Mutex::new(manager));
        self.managers
            .lock()
            .await
            .insert(account.to_string(), handle.clone());
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

        let _guard = crate::utils::push_store_identity(self.trust_mode);
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

        // Warm the manager cache so the new account is usable immediately,
        // and spawn a live push loop when the daemon has a notifier.
        drop(session);
        if let Ok(handle) = self.manager_for(&phone_number).await
            && let Some(tx) = self.notifier.lock().await.clone()
        {
            let mut receive_manager = handle.lock().await.clone();
            let account = phone_number.clone();
            tokio::task::spawn_local(async move {
                let messages = match receive_manager.receive_messages().await {
                    Ok(s) => s,
                    Err(e) => {
                        warn!(%account, error = %e, "receive loop: failed to start message stream");
                        return;
                    }
                };
                pin_mut!(messages);
                while let Some(res) = messages.next().await {
                    if let Received::Content(content) = res {
                        let envelope = build_receive_envelope(&receive_manager, &content).await;
                        let mut params = serde_json::Map::new();
                        params.insert(
                            "envelope".to_string(),
                            serde_json::to_value(envelope).unwrap_or(serde_json::Value::Null),
                        );
                        params.insert(
                            "account".to_string(),
                            serde_json::Value::String(account.clone()),
                        );
                        let notification = crate::rpc::models::JsonRpcNotification {
                            jsonrpc: "2.0".to_string(),
                            method: "receive".to_string(),
                            params: Some(serde_json::Value::Object(params)),
                        };
                        if let Ok(json) = serde_json::to_string(&notification) {
                            let _ = tx.send(json);
                        }
                    }
                }
                warn!(%account, "Message stream ended for account");
            });
        }

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
        let ctx = DaemonContext::new(
            PathBuf::from("/tmp/signal-cli-test"),
            None,
            crate::cli::TrustMode::OnFirstUse,
        );
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
            crate::cli::TrustMode::OnFirstUse,
        );
        assert_eq!(ctx.resolve_account("send", &None).unwrap(), "+15551234567");
    }

    #[test]
    fn list_accounts_is_accountless_method() {
        let ctx = DaemonContext::new(
            PathBuf::from("/tmp"),
            None,
            crate::cli::TrustMode::OnFirstUse,
        );
        assert!(ctx.resolve_account("listAccounts", &None).is_err());
    }
}
