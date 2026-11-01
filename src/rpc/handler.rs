//! JSON-RPC method dispatch. Implemented methods use `commands::core_*`; stubs use `commands::core_*` in `stubs.rs`.
//! Upstream API reference: `docs/signal-cli-jsonrpc.5.adoc`.

use crate::app::RegisteredManager;
use crate::commands;
use crate::errors::SignalError;
use crate::rpc::context::DaemonContext;
use crate::rpc::models::*;
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::sync::Arc;

/// JSON-RPC entry for daemon modes (optional CLI `-a`, per-request `params.account`).
pub async fn handle_daemon_request(
    request: JsonRpcRequest,
    ctx: Arc<DaemonContext>,
) -> JsonRpcResponse {
    let id = request.id.clone().unwrap_or(Value::Null);
    match request.method.as_str() {
        "listAccounts" => {
            return match ctx.list_accounts_json().await {
                Ok(accounts) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: Some(accounts),
                    error: None,
                    id,
                },
                Err(e) => json_rpc_error(id, -32603, e.to_string()),
            };
        }
        "startLink" => {
            let device_name = request
                .params
                .as_ref()
                .and_then(|p| p.get("deviceName"))
                .and_then(|v| v.as_str())
                .map(str::to_string);
            return match ctx.rpc_start_link(device_name).await {
                Ok(result) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: Some(result),
                    error: None,
                    id,
                },
                Err(e) => json_rpc_error(id, -32603, e.to_string()),
            };
        }
        "finishLink" => {
            let params_value = match request.params.clone() {
                Some(p) => p,
                None => {
                    return json_rpc_error(id, -32602, "Missing params".to_string());
                }
            };
            let params: FinishLinkParams = match serde_json::from_value(params_value) {
                Ok(p) => p,
                Err(e) => {
                    return json_rpc_error(
                        id,
                        -32602,
                        format!("Invalid params for finishLink: {e}"),
                    );
                }
            };
            if params.device_link_uri.is_empty() {
                return json_rpc_error(id, -32602, "deviceLinkUri is required".to_string());
            }
            return match ctx
                .rpc_finish_link(params.device_link_uri, params.device_name)
                .await
            {
                Ok(result) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: Some(result),
                    error: None,
                    id,
                },
                Err(e) => json_rpc_error(id, -32603, e.to_string()),
            };
        }
        "subscribeReceive" => {
            return match commands::core_subscribe_receive().await {
                Ok(sub_id) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: Some(json!(sub_id)),
                    error: None,
                    id,
                },
                Err(e) => rpc_result_to_response(id, Err(e)),
            };
        }
        "unsubscribeReceive" => {
            let subscription = request
                .params
                .as_ref()
                .and_then(|p| p.get("subscription"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            return match commands::core_unsubscribe_receive(subscription).await {
                Ok(result) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: Some(serde_json::to_value(result).unwrap_or(json!({}))),
                    error: None,
                    id,
                },
                Err(e) => rpc_result_to_response(id, Err(e)),
            };
        }
        _ => {}
    }

    let account = match ctx.resolve_account(&request.method, &request.params) {
        Ok(a) => a,
        Err(e) => return json_rpc_error(id, -32602, e.to_string()),
    };

    let manager_handle = match ctx.manager_for(&account).await {
        Ok(h) => h,
        Err(e) => {
            let (code, message) = crate::errors::json_rpc_code_for(&e);
            return json_rpc_error(id, code, message);
        }
    };

    let mut manager = manager_handle.lock().await;
    handle_request(request, &mut manager).await
}

fn rpc_result_to_response(id: Value, result: Result<Value>) -> JsonRpcResponse {
    match result {
        Ok(res) => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(res),
            error: None,
            id,
        },
        Err(e) => {
            let (code, message) = crate::errors::json_rpc_code_for(&e);
            json_rpc_error(id, code, message)
        }
    }
}

fn json_rpc_error(id: Value, code: i32, message: String) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        result: None,
        error: Some(JsonRpcError {
            code,
            message,
            data: None,
        }),
        id,
    }
}

pub async fn handle_request(
    request: JsonRpcRequest,
    manager: &mut RegisteredManager,
) -> JsonRpcResponse {
    let id = request.id.unwrap_or(Value::Null);
    let result = match request.method.as_str() {
        "send" => handle_send(request.params, manager).await,
        "receive" => handle_receive(manager).await,
        "listGroups" => handle_list_groups(manager).await,
        "updateProfile" => handle_update_profile(request.params, manager).await,
        "updateContact" => handle_update_contact(request.params, manager).await,
        "listDevices" => handle_list_devices(manager).await,
        "removeDevice" => handle_remove_device(request.params, manager).await,
        "whoami" => handle_whoami(manager).await,
        "clearSessions" => handle_clear_sessions(request.params, manager).await,
        "sendSessionReset" => handle_send_session_reset(request.params, manager).await,
        "sendReceipt" => handle_send_receipt(request.params, manager).await,
        "sendTyping" => handle_send_typing(request.params, manager).await,
        "getUserStatus" => handle_get_user_status(request.params, manager).await,
        "sendReaction" => handle_send_reaction(request.params, manager).await,
        "remoteDelete" => handle_remote_delete(request.params, manager).await,
        "listContacts" => handle_list_contacts(request.params, manager).await,
        "listIdentities" => handle_list_identities(request.params, manager).await,
        "trust" => handle_trust(request.params, manager).await,
        "removeContact" => handle_remove_contact(request.params, manager).await,
        "block" => handle_block(request.params, manager).await,
        "unblock" => handle_unblock(request.params, manager).await,
        "sendContacts" => handle_send_contacts(manager).await,
        "sendSyncRequest" => handle_send_sync_request(manager).await,
        "sendPollCreate" => handle_send_poll_create(request.params, manager).await,
        "sendPollVote" => handle_send_poll_vote(request.params, manager).await,
        "sendPollTerminate" => handle_send_poll_terminate(request.params, manager).await,
        "sendPinMessage" => handle_send_pin_message(request.params, manager).await,
        "sendUnpinMessage" => handle_send_unpin_message(request.params, manager).await,
        "sendPaymentNotification" => {
            handle_send_payment_notification(request.params, manager).await
        }
        "sendAdminDelete" => handle_send_admin_delete(request.params, manager).await,
        "listStickerPacks" => handle_list_sticker_packs(manager).await,
        "addStickerPack" => handle_add_sticker_pack(request.params, manager).await,
        "removeStickerPack" => handle_remove_sticker_pack(request.params, manager).await,
        "getSticker" => handle_get_sticker(request.params, manager).await,
        "uploadStickerPack" => handle_upload_sticker_pack(request.params, manager).await,
        "getAttachment" => handle_get_attachment(request.params, manager).await,
        "getAvatar" => handle_get_avatar(request.params, manager).await,
        // Account settings update. presage exposes no account-settings API,
        // so this is an honest stub.
        "updateConfiguration" => Err(SignalError::LibraryLimitation(
            "updateConfiguration is not yet supported by presage; account settings are unchanged."
                .to_string(),
        )
        .into()),
        "joinGroup" => commands::core_join_group().await.map(|_| json!({})),
        "updateGroup" => commands::core_update_group()
            .await
            .and_then(|r| serde_json::to_value(r).map_err(|e| anyhow!(e))),
        "quitGroup" => commands::core_quit_group()
            .await
            .and_then(|r| serde_json::to_value(r).map_err(|e| anyhow!(e))),
        "addDevice" => handle_add_device(request.params, manager).await,
        _ => {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: "Method not implemented".to_string(),
                    data: None,
                }),
                id,
            };
        }
    };

    match result {
        Ok(res) => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(res),
            error: None,
            id,
        },
        Err(e) => {
            let (code, message) = crate::errors::json_rpc_code_for(&e);
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: Some(JsonRpcError {
                    code,
                    message,
                    data: None,
                }),
                id,
            }
        }
    }
}

async fn handle_send(params: Option<Value>, manager: &mut RegisteredManager) -> Result<Value> {
    let params: SendParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;

    let result = commands::core_send(
        manager,
        false, // note_to_self
        params.recipient.unwrap_or_default(),
        params.group_id,
        params.message,
        params.attachments.unwrap_or_default(),
        None, // daemon confinement via SIGNAL_CLI_ATTACHMENT_DIR env
    )
    .await?;

    Ok(serde_json::to_value(result)?)
}

async fn handle_receive(manager: &mut RegisteredManager) -> Result<Value> {
    let result = commands::core_receive(manager, None).await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_list_groups(manager: &mut RegisteredManager) -> Result<Value> {
    // Always detailed=true (upstream daemon default differs; documented in daemon-rust-port.adoc).
    let result = commands::core_list_groups(manager, true, Vec::new()).await?;
    Ok(serde_json::to_value(result.groups)?)
}

async fn handle_update_profile(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: UpdateProfileParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result = commands::core_update_profile(
        manager,
        params.given_name,
        params.family_name,
        params.about,
        params.about_emoji,
    )
    .await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_update_contact(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: UpdateContactParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result = commands::core_update_contact(
        manager,
        params.recipient,
        params.given_name,
        params.family_name,
        params.expiration,
        params.phone_number,
    )
    .await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_list_devices(manager: &mut RegisteredManager) -> Result<Value> {
    let result = commands::core_list_devices(manager).await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_remove_device(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: RemoveDeviceParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result = commands::core_remove_device(manager, params.device_id).await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_send_receipt(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: SendReceiptParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result = commands::core_send_receipt(
        manager,
        params.recipient,
        params.target_timestamp,
        params.r#type,
    )
    .await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_send_typing(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: SendTypingParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result = commands::core_send_typing(
        manager,
        params.recipient.unwrap_or_default(),
        params.group_id,
        params.stop.unwrap_or(false),
    )
    .await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_get_user_status(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: GetUserStatusParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result = commands::core_get_user_status(manager, params.recipient, params.username).await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_send_reaction(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: SendReactionParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result = commands::core_send_reaction(
        manager,
        params.recipient,
        params.group_id,
        false, // note_to_self
        params.emoji,
        params.target_author,
        params.target_timestamp,
        params.remove.unwrap_or(false),
    )
    .await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_remote_delete(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: RemoteDeleteParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result = commands::core_remote_delete(
        manager,
        params.recipient,
        params.group_id,
        params.target_timestamp,
    )
    .await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_list_contacts(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: ListContactsParams = if let Some(p) = params {
        serde_json::from_value(p)?
    } else {
        ListContactsParams { name: None }
    };
    let result = commands::core_list_contacts(
        manager,
        params.name,
        true, // detailed
    )
    .await?;
    Ok(serde_json::to_value(result.contacts)?)
}

async fn handle_whoami(manager: &mut RegisteredManager) -> Result<Value> {
    let result = commands::core_whoami(manager).await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_clear_sessions(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: RemoveContactParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result = commands::core_clear_sessions(manager, params.recipient).await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_send_session_reset(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: SendSessionResetParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result =
        commands::core_send_session_reset(manager, params.recipient, params.timestamp).await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_list_identities(
    _params: Option<Value>,
    _manager: &mut RegisteredManager,
) -> Result<Value> {
    let result = commands::core_list_identities().await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_trust(_params: Option<Value>, _manager: &mut RegisteredManager) -> Result<Value> {
    let result = commands::core_trust().await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_remove_contact(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: RemoveContactParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result = commands::core_remove_contact(manager, params.recipient).await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_block(_params: Option<Value>, _manager: &mut RegisteredManager) -> Result<Value> {
    let result = commands::core_block().await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_unblock(_params: Option<Value>, _manager: &mut RegisteredManager) -> Result<Value> {
    let result = commands::core_unblock().await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_send_contacts(_manager: &mut RegisteredManager) -> Result<Value> {
    let result = commands::core_send_contacts().await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_send_sync_request(manager: &mut RegisteredManager) -> Result<Value> {
    let result = commands::core_send_sync_request(manager).await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_send_poll_create(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: SendPollCreateParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    if params.options.len() < 2 || params.options.len() > 10 {
        anyhow::bail!("Poll requires between 2 and 10 options.");
    }
    let result = commands::core_send_poll_create(
        manager,
        params.recipient,
        None,
        false,
        params.group_id,
        params.question,
        params.options,
        params.multiple_answers.unwrap_or(true),
    )
    .await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_send_poll_vote(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: SendPollVoteParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result = commands::core_send_poll_vote(
        manager,
        params.recipient,
        None,
        false,
        params.group_id,
        params.target_author,
        params.target_timestamp,
        params
            .choice
            .into_iter()
            .map(|c| u32::try_from(c).map_err(|_| anyhow!("choice index must fit in u32, got {c}")))
            .collect::<Result<Vec<_>, _>>()?,
        u32::try_from(params.vote_count.unwrap_or(1))
            .map_err(|_| anyhow!("vote_count must fit in u32"))?,
    )
    .await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_send_poll_terminate(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: SendPollTerminateParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result = commands::core_send_poll_terminate(
        manager,
        params.recipient,
        None,
        false,
        params.group_id,
        params.target_timestamp,
    )
    .await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_send_pin_message(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: SendPinMessageParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result = commands::core_send_pin_message(
        manager,
        params.recipient,
        None,
        false,
        params.group_id,
        params.target_author,
        params.target_timestamp,
        params.pin_duration,
    )
    .await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_send_unpin_message(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: SendUnpinMessageParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result = commands::core_send_unpin_message(
        manager,
        params.recipient,
        None,
        false,
        params.group_id,
        params.target_author,
        params.target_timestamp,
    )
    .await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_send_payment_notification(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: SendPaymentNotificationParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result = commands::core_send_payment_notification(
        manager,
        params.recipient,
        params.receipt,
        params.note,
    )
    .await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_send_admin_delete(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: SendAdminDeleteParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result = commands::core_send_admin_delete(
        manager,
        params.group_id,
        params.target_author,
        params.target_timestamp,
    )
    .await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_list_sticker_packs(manager: &mut RegisteredManager) -> Result<Value> {
    let result = commands::core_list_sticker_packs(manager).await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_add_sticker_pack(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: AddStickerPackParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result = commands::core_add_sticker_pack(manager, params.pack_id, params.pack_key).await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_remove_sticker_pack(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: AddStickerPackParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result =
        commands::core_remove_sticker_pack(manager, params.pack_id, params.pack_key).await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_get_sticker(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: GetStickerParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result = commands::core_get_sticker(manager, params.pack_id, params.sticker_id).await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_upload_sticker_pack(
    _params: Option<Value>,
    _manager: &mut RegisteredManager,
) -> Result<Value> {
    let result = commands::core_upload_sticker_pack().await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_get_attachment(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: GetAttachmentParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result =
        commands::core_get_attachment(manager, params.id, params.recipient, params.group_id)
            .await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_add_device(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: AddDeviceParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let uri = params
        .device_link_uri
        .or(params.uri)
        .ok_or_else(|| anyhow!("deviceLinkUri or uri is required"))?;
    let result = commands::core_add_device(manager, uri).await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_get_avatar(
    params: Option<Value>,
    manager: &mut RegisteredManager,
) -> Result<Value> {
    let params: GetAvatarParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result =
        commands::core_get_avatar(manager, params.contact, params.profile, params.group_id).await?;
    Ok(serde_json::to_value(result)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::SignalError;

    #[tokio::test]
    async fn rpc_stub_returns_library_limitation_not_account_error() {
        let err = commands::core_block().await.unwrap_err();
        let signal_err = err.downcast_ref::<SignalError>().expect("SignalError");
        assert!(matches!(signal_err, SignalError::LibraryLimitation(_)));
        assert_eq!(signal_err.json_rpc_code(), -32005);
        assert!(!signal_err.to_string().contains("Account not specified"));
    }

    #[tokio::test]
    async fn rpc_unblock_stub_is_distinct_from_block() {
        let block = commands::core_block().await.unwrap_err().to_string();
        let unblock = commands::core_unblock().await.unwrap_err().to_string();
        assert!(block.contains("Blocking"));
        assert!(unblock.contains("Unblocking"));
    }

    #[tokio::test]
    async fn rpc_update_group_returns_library_limitation() {
        let result = commands::core_update_group().await;
        assert!(result.is_err());
        let err = result.err().unwrap();
        let signal_err = err.downcast_ref::<SignalError>().expect("SignalError");
        assert_eq!(signal_err.json_rpc_code(), -32005);
    }

    #[tokio::test]
    async fn daemon_send_without_account_returns_32602() {
        use crate::rpc::context::DaemonContext;
        use std::path::PathBuf;

        let ctx = DaemonContext::new(
            PathBuf::from("/tmp/signal-cli-test"),
            None,
            crate::cli::TrustMode::OnFirstUse,
        );
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "send".to_string(),
            params: Some(json!({ "message": "hi" })),
            id: Some(json!(1)),
        };
        let response = handle_daemon_request(request, ctx).await;
        let error = response.error.expect("error response");
        assert_eq!(error.code, -32602);
    }

    #[tokio::test]
    async fn daemon_list_accounts_does_not_require_account() {
        use crate::rpc::context::DaemonContext;
        use std::path::PathBuf;

        let ctx = DaemonContext::new(
            PathBuf::from("/tmp/signal-cli-test"),
            None,
            crate::cli::TrustMode::OnFirstUse,
        );
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "listAccounts".to_string(),
            params: None,
            id: Some(json!(1)),
        };
        let response = handle_daemon_request(request, ctx).await;
        assert!(response.error.is_none());
        assert!(response.result.is_some());
    }
}
