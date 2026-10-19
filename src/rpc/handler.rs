//! JSON-RPC method dispatch. Implemented methods use `commands::core_*`; stubs use `commands::core_*` in `stubs.rs`.
//! Upstream API reference: `docs/signal-cli-jsonrpc.5.adoc`.

use crate::commands;
use crate::errors::SignalError;
use crate::rpc::models::*;
use anyhow::{Result, anyhow};
use presage::Manager;
use presage::manager::Registered;
use presage::store::Store;
use serde_json::Value;

pub async fn handle_request<S: Store + Send + 'static>(
    request: JsonRpcRequest,
    manager: &mut Manager<S, Registered>,
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
        "listStickerPacks" => handle_list_sticker_packs(manager).await,
        "addStickerPack" => handle_add_sticker_pack(request.params, manager).await,
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
            let (code, message) = if let Some(signal_err) = e.downcast_ref::<SignalError>() {
                (signal_err.json_rpc_code(), signal_err.to_string())
            } else {
                (-32603, format!("{:?}", e))
            };
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

async fn handle_send<S: Store + Send + 'static>(
    params: Option<Value>,
    manager: &mut Manager<S, Registered>,
) -> Result<Value> {
    let params: SendParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;

    let result = commands::core_send(
        manager,
        false, // note_to_self
        params.recipient.unwrap_or_default(),
        params.group_id,
        params.message,
        params.attachments.unwrap_or_default(),
    )
    .await?;

    Ok(serde_json::to_value(result)?)
}

async fn handle_receive<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
) -> Result<Value> {
    let result = commands::core_receive(manager, None).await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_list_groups<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
) -> Result<Value> {
    // Always detailed=true (upstream daemon default differs; documented in daemon-rust-port.adoc).
    let result = commands::core_list_groups(manager, true, Vec::new()).await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_update_profile<S: Store + Send + 'static>(
    params: Option<Value>,
    manager: &mut Manager<S, Registered>,
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

async fn handle_update_contact<S: Store + Send + 'static>(
    params: Option<Value>,
    manager: &mut Manager<S, Registered>,
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

async fn handle_list_devices<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
) -> Result<Value> {
    let result = commands::core_list_devices(manager).await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_remove_device<S: Store + Send + 'static>(
    params: Option<Value>,
    manager: &mut Manager<S, Registered>,
) -> Result<Value> {
    let params: RemoveDeviceParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result = commands::core_remove_device(manager, params.device_id).await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_send_receipt<S: Store + Send + 'static>(
    params: Option<Value>,
    manager: &mut Manager<S, Registered>,
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

async fn handle_send_typing<S: Store + Send + 'static>(
    params: Option<Value>,
    manager: &mut Manager<S, Registered>,
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

async fn handle_get_user_status<S: Store + Send + 'static>(
    params: Option<Value>,
    manager: &mut Manager<S, Registered>,
) -> Result<Value> {
    let params: GetUserStatusParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result = commands::core_get_user_status(manager, params.recipient, params.username).await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_send_reaction<S: Store + Send + 'static>(
    params: Option<Value>,
    manager: &mut Manager<S, Registered>,
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

async fn handle_remote_delete<S: Store + Send + 'static>(
    params: Option<Value>,
    manager: &mut Manager<S, Registered>,
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

async fn handle_list_contacts<S: Store + Send + 'static>(
    params: Option<Value>,
    manager: &mut Manager<S, Registered>,
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
    Ok(serde_json::to_value(result)?)
}

async fn handle_list_identities<S: Store + Send + 'static>(
    _params: Option<Value>,
    _manager: &mut Manager<S, Registered>,
) -> Result<Value> {
    let result = commands::core_list_identities().await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_trust<S: Store + Send + 'static>(
    _params: Option<Value>,
    _manager: &mut Manager<S, Registered>,
) -> Result<Value> {
    let result = commands::core_trust().await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_remove_contact<S: Store + Send + 'static>(
    params: Option<Value>,
    manager: &mut Manager<S, Registered>,
) -> Result<Value> {
    let params: RemoveContactParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result = commands::core_remove_contact(manager, params.recipient).await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_block<S: Store + Send + 'static>(
    _params: Option<Value>,
    _manager: &mut Manager<S, Registered>,
) -> Result<Value> {
    let result = commands::core_block().await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_unblock<S: Store + Send + 'static>(
    _params: Option<Value>,
    _manager: &mut Manager<S, Registered>,
) -> Result<Value> {
    let result = commands::core_unblock().await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_send_contacts<S: Store + Send + 'static>(
    _manager: &mut Manager<S, Registered>,
) -> Result<Value> {
    let result = commands::core_send_contacts().await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_send_sync_request<S: Store + Send + 'static>(
    _manager: &mut Manager<S, Registered>,
) -> Result<Value> {
    let result = commands::core_send_sync_request().await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_send_poll_create<S: Store + Send + 'static>(
    _params: Option<Value>,
    _manager: &mut Manager<S, Registered>,
) -> Result<Value> {
    let result = commands::core_send_poll_create().await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_send_poll_vote<S: Store + Send + 'static>(
    _params: Option<Value>,
    _manager: &mut Manager<S, Registered>,
) -> Result<Value> {
    let result = commands::core_send_poll_vote().await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_send_poll_terminate<S: Store + Send + 'static>(
    _params: Option<Value>,
    _manager: &mut Manager<S, Registered>,
) -> Result<Value> {
    let result = commands::core_send_poll_terminate().await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_list_sticker_packs<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
) -> Result<Value> {
    let result = commands::core_list_sticker_packs(manager).await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_add_sticker_pack<S: Store + Send + 'static>(
    params: Option<Value>,
    manager: &mut Manager<S, Registered>,
) -> Result<Value> {
    let params: AddStickerPackParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result = commands::core_add_sticker_pack(manager, params.pack_id, params.pack_key).await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_get_sticker<S: Store + Send + 'static>(
    params: Option<Value>,
    manager: &mut Manager<S, Registered>,
) -> Result<Value> {
    let params: GetStickerParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result = commands::core_get_sticker(manager, params.pack_id, params.sticker_id).await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_upload_sticker_pack<S: Store + Send + 'static>(
    _params: Option<Value>,
    _manager: &mut Manager<S, Registered>,
) -> Result<Value> {
    let result = commands::core_upload_sticker_pack().await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_get_attachment<S: Store + Send + 'static>(
    params: Option<Value>,
    manager: &mut Manager<S, Registered>,
) -> Result<Value> {
    let params: GetAttachmentParams =
        serde_json::from_value(params.ok_or_else(|| anyhow!("Missing params"))?)?;
    let result =
        commands::core_get_attachment(manager, params.id, params.recipient, params.group_id)
            .await?;
    Ok(serde_json::to_value(result)?)
}

async fn handle_get_avatar<S: Store + Send + 'static>(
    params: Option<Value>,
    manager: &mut Manager<S, Registered>,
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
}
