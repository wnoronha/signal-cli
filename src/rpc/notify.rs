//! Single receive-notification pipeline for daemon modes.
//!
//! Previously `rpc/server.rs` and `rpc/context.rs` each owned a ~40-line
//! receive loop that built the same `receive` notification. Both now call
//! [`build_receive_notification`] and [`spawn_receive_loop`].

use futures::{StreamExt as _, pin_mut};
use presage::model::messages::Received;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::warn;

use crate::app::ManagerHandle;
use crate::commands::build_receive_envelope;
use crate::rpc::models::JsonRpcNotification;

/// Build one `receive` push notification for `account`.
pub async fn build_receive_notification(
    manager: &crate::app::RegisteredManager,
    account: &str,
    content: &presage::libsignal_service::content::Content,
) -> JsonRpcNotification {
    let envelope = build_receive_envelope(manager, content).await;
    let mut params = serde_json::Map::new();
    params.insert(
        "envelope".to_string(),
        serde_json::to_value(envelope).unwrap_or(serde_json::Value::Null),
    );
    params.insert(
        "account".to_string(),
        serde_json::Value::String(account.to_string()),
    );
    JsonRpcNotification {
        jsonrpc: "2.0".to_string(),
        method: "receive".to_string(),
        params: Some(serde_json::Value::Object(params)),
    }
}

/// Spawn a background receive loop for `account`. Clones the manager under a
/// short lock so RPC handlers are not blocked by `receive_messages().await`.
pub fn spawn_receive_loop(
    manager_handle: ManagerHandle,
    account: String,
    notification_tx: broadcast::Sender<String>,
) {
    tokio::task::spawn_local(async move {
        let mut receive_manager = manager_handle.lock().await.clone();
        let messages = match receive_manager.receive_messages().await {
            Ok(s) => s,
            Err(e) => {
                warn!(%account, error = %e, "receive loop: failed to start message stream");
                return;
            }
        };
        pin_mut!(messages);
        while let Some(received_res) = messages.next().await {
            match received_res {
                Received::Content(content) => {
                    let notification =
                        build_receive_notification(&receive_manager, &account, &content).await;
                    if let Ok(json) = serde_json::to_string(&notification) {
                        let _ = notification_tx.send(json);
                    }
                }
                Received::QueueEmpty => {}
                _ => {}
            }
        }
        warn!(%account, "Message stream ended for account");
    });
}

/// Resolve-then-spawn helper for contexts that own an `AccountStore`-like
/// `manager_for`. Kept here so server and context share one call site.
pub async fn spawn_for_account(
    ctx: Arc<crate::rpc::context::DaemonContext>,
    account: String,
    notification_tx: broadcast::Sender<String>,
) {
    match ctx.manager_for(&account).await {
        Ok(handle) => spawn_receive_loop(handle, account, notification_tx),
        Err(e) => warn!(%account, error = %e, "receive loop: failed to load account"),
    }
}
