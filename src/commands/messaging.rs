use anyhow::{Context, anyhow};
use base64::prelude::*;
use futures::{StreamExt, pin_mut};
use std::path::{Path, PathBuf};

use presage::Manager;
use presage::libsignal_service::content::{ContentBody, DataMessage, GroupContextV2};
use presage::libsignal_service::prelude::Uuid;
use presage::libsignal_service::proto::{
    ReceiptMessage, TypingMessage, data_message, receipt_message, typing_message,
};
use presage::libsignal_service::protocol::ServiceId;
use presage::manager::Registered;
use presage::model::messages::Received;
use presage::store::Store;

use crate::output::{ReceiveEnvelope, SendResult, SuccessResult};
use crate::utils::{now_millis, open_sqlite_store, resolve_recipient, u64_from_i64_nonnegative};

use super::{build_receive_envelope, load_registered_and_receive, parse_group_id};

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_send(
    config_dir: &Path,
    active_account: Option<String>,
    note_to_self: bool,
    recipient: Vec<String>,
    group_id: Option<String>,
    message: Option<String>,
    message_from_stdin: bool,
    attachment: Vec<String>,
    attachment_dir: Option<PathBuf>,
) -> anyhow::Result<SendResult> {
    let account = crate::utils::require_account(active_account)?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;

    let mut body_text = message;
    if message_from_stdin {
        use tokio::io::AsyncReadExt;
        let mut buffer = String::new();
        tokio::io::stdin().read_to_string(&mut buffer).await?;
        body_text = Some(buffer.trim().to_string());
    }

    core_send(
        &mut manager,
        note_to_self,
        recipient,
        group_id,
        body_text,
        attachment,
        attachment_dir,
    )
    .await
}

#[tracing::instrument(skip(manager, body_text))]
pub(crate) async fn core_send<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    note_to_self: bool,
    recipient: Vec<String>,
    group_id: Option<String>,
    body_text: Option<String>,
    attachment: Vec<String>,
    attachment_dir: Option<PathBuf>,
) -> anyhow::Result<SendResult> {
    use tracing::warn;
    let dir = attachment_dir.or_else(|| {
        std::env::var("SIGNAL_CLI_ATTACHMENT_DIR")
            .ok()
            .map(PathBuf::from)
    });
    let attachments = upload_attachments_in_dir(attachment, manager, dir).await?;

    let timestamp = now_millis()?;

    if let Some(ref gid) = group_id {
        let master_key = parse_group_id(gid)?;
        let data_message = DataMessage {
            body: body_text,
            attachments,
            group_v2: Some(GroupContextV2 {
                master_key: Some(master_key.to_vec()),
                revision: Some(0),
                ..Default::default()
            }),
            ..Default::default()
        };
        let content_body = ContentBody::DataMessage(data_message);
        manager
            .send_message_to_group(&master_key, content_body, timestamp)
            .await?;
    } else {
        let mut resolved_recipients = Vec::new();
        if note_to_self {
            resolved_recipients.push(ServiceId::Aci(
                manager.registration_data().service_ids.aci.into(),
            ));
        }
        for rec in recipient {
            match resolve_recipient(manager, &rec).await {
                Ok(resolved) => resolved_recipients.push(resolved),
                Err(e) => warn!("Error resolving recipient {}: {:?}", rec, e),
            }
        }

        if resolved_recipients.is_empty() {
            anyhow::bail!("No valid recipients specified.");
        }

        let data_message = DataMessage {
            body: body_text,
            attachments,
            ..Default::default()
        };
        let content_body = ContentBody::DataMessage(data_message);

        for r in resolved_recipients {
            manager
                .send_message(r, content_body.clone(), timestamp)
                .await?;
        }
    }
    Ok(SendResult { timestamp })
}

pub(crate) async fn run_receive(
    config_dir: &Path,
    active_account: Option<String>,
    max_messages: Option<usize>,
) -> anyhow::Result<Vec<ReceiveEnvelope>> {
    let account = crate::utils::require_account(active_account)?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = Manager::load_registered(store).await?;

    core_receive(&mut manager, max_messages).await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_receive<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    max_messages: Option<usize>,
) -> anyhow::Result<Vec<ReceiveEnvelope>> {
    use tracing::{debug, info};
    info!("Querying server for new messages...");
    let messages = manager
        .receive_messages()
        .await
        .context("failed to receive messages")?;
    pin_mut!(messages);

    let mut count = 0;
    let mut envelopes = Vec::new();
    while let Some(content) = messages.next().await {
        match content {
            Received::QueueEmpty => break,
            Received::Contacts => {}
            Received::DecryptionError(sid) => {
                debug!("Decryption error from {}", sid.service_id_string());
            }
            Received::Content(content) => {
                debug!(
                    "Processing received message from {}",
                    content.metadata.sender.service_id_string()
                );
                let envelope = build_receive_envelope(&*manager, &content).await;
                envelopes.push(envelope);

                count += 1;
                if let Some(max) = max_messages
                    && count >= max
                {
                    break;
                }
            }
        }
    }
    Ok(envelopes)
}

async fn upload_attachments<S: Store>(
    attachment_filepath: Vec<String>,
    manager: &Manager<S, Registered>,
) -> anyhow::Result<Vec<presage::proto::AttachmentPointer>> {
    upload_attachments_in_dir(attachment_filepath, manager, None).await
}

async fn upload_attachments_in_dir<S: Store>(
    attachment_filepath: Vec<String>,
    manager: &Manager<S, Registered>,
    allowed_dir: Option<PathBuf>,
) -> anyhow::Result<Vec<presage::proto::AttachmentPointer>> {
    use mime_guess::mime::APPLICATION_OCTET_STREAM;
    use presage::libsignal_service::sender::AttachmentSpec;

    const MAX_ATTACHMENT_BYTES: usize = 100 * 1024 * 1024;
    const MAX_ATTACHMENTS: usize = 10;

    if attachment_filepath.len() > MAX_ATTACHMENTS {
        anyhow::bail!(
            "Too many attachments ({} > {MAX_ATTACHMENTS})",
            attachment_filepath.len()
        );
    }
    let mut attachment_specs = Vec::new();
    for path_or_uri in attachment_filepath {
        if let Some(data_uri) = path_or_uri.strip_prefix("data:") {
            // Handle Data URI: data:[<mediatype>][;base64],<data>
            let (metadata, base64_data) = data_uri
                .split_once(',')
                .ok_or_else(|| anyhow!("Invalid data URI format"))?;

            let mut parts = metadata.split(';');
            let content_type = parts
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("application/octet-stream");

            let mut is_base64 = false;
            let mut file_name = None;
            for part in parts {
                if part == "base64" {
                    is_base64 = true;
                } else if let Some(name) = part.strip_prefix("filename=") {
                    file_name = Some(name.to_string());
                }
            }

            if !is_base64 {
                anyhow::bail!("Only base64 encoded data URIs are supported.");
            }

            let data = BASE64_STANDARD
                .decode(base64_data)
                .map_err(|e| anyhow!("Failed to decode base64 in data URI: {:?}", e))?;
            if data.len() > MAX_ATTACHMENT_BYTES {
                anyhow::bail!("Attachment too large (> {MAX_ATTACHMENT_BYTES} bytes)");
            }

            attachment_specs.push((
                AttachmentSpec {
                    content_type: content_type.to_string(),
                    length: data.len(),
                    file_name,
                    preview: None,
                    voice_note: None,
                    borderless: None,
                    width: None,
                    height: None,
                    caption: None,
                    blur_hash: None,
                },
                data,
            ));
        } else {
            // Handle local file path (async: do not block the executor).
            // Reject symlinks so a daemon client cannot escape an
            // --attachment-dir confinement via link swap.
            let path = PathBuf::from(path_or_uri);
            let meta = tokio::fs::symlink_metadata(&path).await?;
            if meta.file_type().is_symlink() {
                anyhow::bail!("Attachment must not be a symlink: {}", path.display());
            }
            if let Some(ref dir) = allowed_dir {
                let canonical_file = tokio::fs::canonicalize(&path).await?;
                let canonical_dir = tokio::fs::canonicalize(dir).await?;
                if !canonical_file.starts_with(&canonical_dir) {
                    anyhow::bail!("Attachment outside --attachment-dir: {}", path.display());
                }
            }
            let meta = tokio::fs::metadata(&path).await?;
            if !meta.is_file() {
                anyhow::bail!("Attachment is not a regular file: {}", path.display());
            }
            if meta.len() > MAX_ATTACHMENT_BYTES as u64 {
                anyhow::bail!("Attachment too large (> {MAX_ATTACHMENT_BYTES} bytes)");
            }
            let data = tokio::fs::read(&path).await?;
            attachment_specs.push((
                AttachmentSpec {
                    content_type: mime_guess::from_path(&path)
                        .first()
                        .unwrap_or(APPLICATION_OCTET_STREAM)
                        .to_string(),
                    length: data.len(),
                    file_name: path.file_name().map(|s| s.to_string_lossy().to_string()),
                    preview: None,
                    voice_note: None,
                    borderless: None,
                    width: None,
                    height: None,
                    caption: None,
                    blur_hash: None,
                },
                data,
            ));
        }
    }

    if attachment_specs.is_empty() {
        return Ok(Vec::new());
    }

    let attachments: Result<Vec<_>, _> = manager
        .upload_attachments(attachment_specs)
        .await?
        .into_iter()
        .collect();

    Ok(attachments?)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_send_reaction(
    config_dir: &Path,
    active_account: Option<String>,
    recipient: Vec<String>,
    group_id: Option<String>,
    note_to_self: bool,
    emoji: String,
    target_author: String,
    target_timestamp: i64,
    remove: bool,
) -> anyhow::Result<SuccessResult> {
    let account = crate::utils::require_account(active_account)?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;

    core_send_reaction(
        &mut manager,
        recipient,
        group_id,
        note_to_self,
        emoji,
        target_author,
        target_timestamp,
        remove,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip(manager))]
pub(crate) async fn core_send_reaction<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    recipient: Vec<String>,
    group_id: Option<String>,
    note_to_self: bool,
    emoji: String,
    target_author: String,
    target_timestamp: i64,
    remove: bool,
) -> anyhow::Result<SuccessResult> {
    let target_author_sid = resolve_recipient(manager, &target_author).await?;
    let target_author_aci = match target_author_sid {
        ServiceId::Aci(u) => Uuid::from(u),
        ServiceId::Pni(u) => Uuid::from(u),
    };

    let reaction = data_message::Reaction {
        emoji: Some(emoji),
        remove: Some(remove),
        target_author_aci: Some(target_author_aci.to_string()),
        target_sent_timestamp: Some(u64_from_i64_nonnegative(
            target_timestamp,
            "target_timestamp",
        )?),
        target_author_aci_binary: Some(target_author_aci.as_bytes().to_vec()),
    };

    let data_message = DataMessage {
        reaction: Some(reaction),
        ..Default::default()
    };
    let content_body = ContentBody::DataMessage(data_message);

    let timestamp = now_millis()?;

    if let Some(gid) = group_id {
        let master_key = parse_group_id(&gid)?;
        manager
            .send_message_to_group(&master_key, content_body, timestamp)
            .await?;
    } else {
        let mut recipients = Vec::new();
        if note_to_self {
            recipients.push(ServiceId::Aci(
                manager.registration_data().service_ids.aci.into(),
            ));
        }
        for rec in recipient {
            recipients.push(resolve_recipient(manager, &rec).await?);
        }
        for r in recipients {
            manager
                .send_message(r, content_body.clone(), timestamp)
                .await?;
        }
    }

    Ok(SuccessResult {
        success: true,
        message: "Reaction sent successfully.".to_string(),
    })
}

pub(crate) async fn run_send_receipt(
    config_dir: &Path,
    active_account: Option<String>,
    recipient: String,
    target_timestamp: i64,
    r#type: Option<String>,
) -> anyhow::Result<SuccessResult> {
    let account = crate::utils::require_account(active_account)?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;

    core_send_receipt(&mut manager, recipient, target_timestamp, r#type).await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_send_receipt<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    recipient: String,
    target_timestamp: i64,
    r#type: Option<String>,
) -> anyhow::Result<SuccessResult> {
    let service_id = resolve_recipient(manager, &recipient).await?;

    let receipt_type = match r#type.as_deref() {
        Some("read") => receipt_message::Type::Read,
        Some("viewed") => receipt_message::Type::Viewed,
        _ => receipt_message::Type::Read, // Default to read
    };

    let receipt_message = ReceiptMessage {
        r#type: Some(receipt_type as i32),
        timestamp: vec![u64_from_i64_nonnegative(
            target_timestamp,
            "target_timestamp",
        )?],
    };

    let content_body = ContentBody::ReceiptMessage(receipt_message);
    let timestamp = now_millis()?;

    manager
        .send_message(service_id, content_body, timestamp)
        .await?;

    Ok(SuccessResult {
        success: true,
        message: "Receipt sent successfully.".to_string(),
    })
}

pub(crate) async fn run_send_typing(
    config_dir: &Path,
    active_account: Option<String>,
    recipient: Vec<String>,
    group_id: Option<String>,
    stop: bool,
) -> anyhow::Result<SuccessResult> {
    let account = crate::utils::require_account(active_account)?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;

    core_send_typing(&mut manager, recipient, group_id, stop).await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_send_typing<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    recipient: Vec<String>,
    group_id: Option<String>,
    stop: bool,
) -> anyhow::Result<SuccessResult> {
    let action = if stop {
        typing_message::Action::Stopped
    } else {
        typing_message::Action::Started
    };

    let timestamp = now_millis()?;

    let typing_message = TypingMessage {
        timestamp: Some(timestamp),
        action: Some(action as i32),
        group_id: None, // Will be set if group_id is provided
    };

    if let Some(gid) = group_id {
        let master_key = parse_group_id(&gid)?;
        let mut msg = typing_message.clone();
        msg.group_id = Some(master_key.to_vec());
        let content_body = ContentBody::TypingMessage(msg);
        manager
            .send_message_to_group(&master_key, content_body, timestamp)
            .await?;
    } else {
        let content_body = ContentBody::TypingMessage(typing_message);
        for rec in recipient {
            let service_id = resolve_recipient(manager, &rec).await?;
            manager
                .send_message(service_id, content_body.clone(), timestamp)
                .await?;
        }
    }

    Ok(SuccessResult {
        success: true,
        message: "Typing indicator sent successfully.".to_string(),
    })
}

pub(crate) async fn run_remote_delete(
    config_dir: &Path,
    active_account: Option<String>,
    recipient: Vec<String>,
    group_id: Option<String>,
    _username: Option<String>,
    target_timestamp: i64,
) -> anyhow::Result<SuccessResult> {
    let account = crate::utils::require_account(active_account)?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;
    core_remote_delete(&mut manager, recipient, group_id, target_timestamp).await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_remote_delete<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    recipient: Vec<String>,
    group_id: Option<String>,
    target_timestamp: i64,
) -> anyhow::Result<SuccessResult> {
    let delete = data_message::Delete {
        target_sent_timestamp: Some(u64_from_i64_nonnegative(
            target_timestamp,
            "target_timestamp",
        )?),
    };

    let data_message = DataMessage {
        delete: Some(delete),
        ..Default::default()
    };
    let content_body = ContentBody::DataMessage(data_message);

    let timestamp = now_millis()?;

    if let Some(gid) = group_id {
        let master_key = parse_group_id(&gid)?;
        manager
            .send_message_to_group(&master_key, content_body, timestamp)
            .await?;
    } else {
        for rec in recipient {
            let service_id = resolve_recipient(manager, &rec).await?;
            manager
                .send_message(service_id, content_body.clone(), timestamp)
                .await?;
        }
    }

    Ok(SuccessResult {
        success: true,
        message: "Remote delete sent successfully.".to_string(),
    })
}
