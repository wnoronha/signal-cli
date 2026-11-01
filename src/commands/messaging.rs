use anyhow::{Context, anyhow};
use base64::prelude::*;
use futures::{StreamExt, pin_mut};
use std::path::{Path, PathBuf};

use presage::Manager;
use presage::libsignal_service::content::{ContentBody, DataMessage, GroupContextV2};
use presage::libsignal_service::prelude::Uuid;
use presage::libsignal_service::proto::{
    BodyRange, Preview, ReceiptMessage, TypingMessage, body_range, data_message, receipt_message,
    typing_message,
};
use presage::libsignal_service::protocol::ServiceId;
use presage::model::messages::Received;

use crate::output::{ReceiveEnvelope, SendResult, SuccessResult};
use crate::utils::{now_millis, open_sqlite_store, resolve_recipient, u64_from_i64_nonnegative};

use super::{build_receive_envelope, load_registered, parse_group_id};
use crate::app::RegisteredManager;

/// Fully-typed `send` arguments. Parsed once at the CLI/RPC boundary;
/// `core_send_full` trusts them.
#[derive(Debug, Default)]
pub(crate) struct SendArgs {
    pub note_to_self: bool,
    pub recipient: Vec<String>,
    pub username: Option<String>,
    pub group_id: Option<String>,
    pub body_text: Option<String>,
    pub attachment: Vec<String>,
    pub attachment_dir: Option<PathBuf>,
    pub view_once: bool,
    pub sticker: Option<String>,
    pub mention: Vec<String>,
    pub text_style: Vec<String>,
    pub quote_timestamp: Option<i64>,
    pub quote_author: Option<String>,
    pub quote_message: Option<String>,
    pub quote_mention: Vec<String>,
    pub quote_text_style: Vec<String>,
    pub quote_attachment: Vec<String>,
    pub preview_url: Option<String>,
    pub preview_title: Option<String>,
    pub preview_description: Option<String>,
    pub preview_image: Option<String>,
    pub story_timestamp: Option<i64>,
    pub story_author: Option<String>,
    pub edit_timestamp: Option<i64>,
}

/// Resolve any recipient string to raw ACI bytes for mention/quote/pin targets.
async fn resolve_aci_bytes(
    manager: &mut RegisteredManager,
    recipient: &str,
) -> anyhow::Result<[u8; 16]> {
    let sid = resolve_recipient(manager, recipient).await?;
    let uuid = match sid {
        ServiceId::Aci(u) => Uuid::from(u),
        ServiceId::Pni(u) => Uuid::from(u),
    };
    Ok(*uuid.as_bytes())
}

/// Parse `start:length:recipient` (UTF-16 code units) into a mention range.
async fn parse_mention(manager: &mut RegisteredManager, spec: &str) -> anyhow::Result<BodyRange> {
    let mut parts = spec.splitn(3, ':');
    let (start_s, len_s, recipient) = match (parts.next(), parts.next(), parts.next()) {
        (Some(s), Some(l), Some(r)) => (s, l, r),
        _ => anyhow::bail!("Invalid --mention '{spec}'; expected start:length:recipient"),
    };
    let start: u32 = start_s
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid --mention '{spec}': bad start"))?;
    let length: u32 = len_s
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid --mention '{spec}': bad length"))?;
    let aci = resolve_aci_bytes(manager, recipient).await?;
    Ok(BodyRange {
        start: Some(start),
        length: Some(length),
        associated_value: Some(body_range::AssociatedValue::MentionAciBinary(aci.to_vec())),
    })
}

/// Parse `start:length:STYLE` into a style range.
fn parse_text_style(spec: &str) -> anyhow::Result<BodyRange> {
    let mut parts = spec.splitn(3, ':');
    let (start_s, len_s, style) = match (parts.next(), parts.next(), parts.next()) {
        (Some(s), Some(l), Some(st)) => (s, l, st),
        _ => anyhow::bail!("Invalid --text-style '{spec}'; expected start:length:STYLE"),
    };
    let start: u32 = start_s
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid --text-style '{spec}': bad start"))?;
    let length: u32 = len_s
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid --text-style '{spec}': bad length"))?;
    let style = match style.to_ascii_uppercase().as_str() {
        "BOLD" => body_range::Style::Bold,
        "ITALIC" => body_range::Style::Italic,
        "SPOILER" => body_range::Style::Spoiler,
        "STRIKETHROUGH" => body_range::Style::Strikethrough,
        "MONOSPACE" => body_range::Style::Monospace,
        _ => anyhow::bail!(
            "Invalid --text-style '{spec}': unknown style (BOLD, ITALIC, SPOILER, STRIKETHROUGH, MONOSPACE)"
        ),
    };
    Ok(BodyRange {
        start: Some(start),
        length: Some(length),
        associated_value: Some(body_range::AssociatedValue::Style(style as i32)),
    })
}

#[tracing::instrument(skip(manager, args))]
pub(crate) async fn core_send_full(
    manager: &mut RegisteredManager,
    mut args: SendArgs,
) -> anyhow::Result<SendResult> {
    use tracing::warn;
    let dir = args.attachment_dir.or_else(|| {
        std::env::var("SIGNAL_CLI_ATTACHMENT_DIR")
            .ok()
            .map(PathBuf::from)
    });
    let attachments =
        upload_attachments_in_dir(std::mem::take(&mut args.attachment), manager, dir.clone())
            .await?;

    // Mentions and text styles resolve against the live manager (network for
    // unknown phone numbers), so they are built before the send loop.
    let mut body_ranges = Vec::new();
    for spec in &args.mention {
        body_ranges.push(parse_mention(manager, spec).await?);
    }
    for spec in &args.text_style {
        body_ranges.push(parse_text_style(spec)?);
    }

    // Quote assembly (optional).
    let quote = if args.quote_timestamp.is_some()
        || args.quote_author.is_some()
        || args.quote_message.is_some()
    {
        let ts = args
            .quote_timestamp
            .ok_or_else(|| anyhow::anyhow!("--quote-timestamp is required when quoting"))?;
        let author = args
            .quote_author
            .clone()
            .ok_or_else(|| anyhow::anyhow!("--quote-author is required when quoting"))?;
        let author_aci = resolve_aci_bytes(manager, &author).await?;
        let mut quote_ranges = Vec::new();
        for spec in &args.quote_mention {
            quote_ranges.push(parse_mention(manager, spec).await?);
        }
        for spec in &args.quote_text_style {
            quote_ranges.push(parse_text_style(spec)?);
        }
        let mut quoted_attachments = Vec::new();
        for spec in &args.quote_attachment {
            // contentType[:filename[:previewFile]]; the preview thumbnail is
            // not re-uploaded (documented deviation).
            let mut parts = spec.splitn(3, ':');
            let content_type = parts.next().unwrap_or("").to_string();
            if content_type.is_empty() {
                anyhow::bail!(
                    "Invalid --quote-attachment '{spec}'; expected contentType[:filename]"
                );
            }
            quoted_attachments.push(data_message::quote::QuotedAttachment {
                content_type: Some(content_type),
                file_name: parts.next().map(str::to_string),
                thumbnail: None,
            });
        }
        Some(data_message::Quote {
            id: Some(crate::utils::u64_from_i64_nonnegative(
                ts,
                "quote_timestamp",
            )?),
            author_aci: None,
            author_aci_binary: Some(author_aci.to_vec()),
            text: args.quote_message.clone(),
            attachments: quoted_attachments,
            body_ranges: quote_ranges,
            r#type: None,
        })
    } else {
        None
    };

    // Sticker (packId:stickerId, hex pack id).
    let sticker = args
        .sticker
        .as_deref()
        .map(|spec| -> anyhow::Result<data_message::Sticker> {
            let (pack, id) = spec.split_once(':').ok_or_else(|| {
                anyhow::anyhow!("Invalid --sticker '{spec}'; expected packId:stickerId")
            })?;
            let pack_id = hex::decode(pack.trim_start_matches("0x"))
                .map_err(|e| anyhow::anyhow!("Invalid --sticker pack id: {e:?}"))?;
            let sticker_id: u32 = id
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid --sticker '{spec}': bad sticker id"))?;
            Ok(data_message::Sticker {
                pack_id: Some(pack_id),
                pack_key: None,
                sticker_id: Some(sticker_id),
                data: None,
                emoji: None,
            })
        })
        .transpose()?;

    // Explicit link preview (all local; no network fetch).
    let preview = match (args.preview_url.clone(), args.preview_title.clone()) {
        (None, None) => None,
        (Some(url), Some(title)) => {
            if let Some(ref body) = args.body_text
                && !body.contains(&url)
            {
                warn!(
                    "Preview URL does not appear in the message body; clients may ignore the preview"
                );
            }
            let image = match args.preview_image.clone() {
                Some(path) => {
                    let pointers =
                        upload_attachments_in_dir(vec![path], manager, dir.clone()).await?;
                    pointers.into_iter().next()
                }
                None => None,
            };
            Some(Preview {
                url: Some(url),
                title: Some(title),
                description: args.preview_description.clone(),
                image,
                date: None,
            })
        }
        _ => anyhow::bail!("--preview-url and --preview-title must be given together"),
    };

    // Story reply context.
    let story_context = match (args.story_timestamp, args.story_author.clone()) {
        (None, None) => None,
        (Some(ts), Some(author)) => {
            let aci = resolve_aci_bytes(manager, &author).await?;
            Some(data_message::StoryContext {
                author_aci: None,
                sent_timestamp: Some(crate::utils::u64_from_i64_nonnegative(
                    ts,
                    "story_timestamp",
                )?),
                author_aci_binary: Some(aci.to_vec()),
            })
        }
        _ => anyhow::bail!("--story-timestamp and --story-author must be given together"),
    };

    if args.edit_timestamp.is_some() {
        warn!(
            "--edit-timestamp is not sent: this build cannot emit EditMessage (sync-message path)"
        );
    }

    let timestamp = now_millis()?;

    // Resolve recipients, including username lookup.
    let mut resolved_recipients = Vec::new();
    if args.note_to_self {
        resolved_recipients.push(ServiceId::Aci(
            manager.registration_data().service_ids.aci.into(),
        ));
    }
    if let Some(ref username) = args.username {
        match manager.lookup_username(username).await {
            Ok(Some(aci)) => {
                let uuid: Uuid = aci.into();
                resolved_recipients.push(ServiceId::Aci(uuid.into()));
            }
            Ok(None) => anyhow::bail!("Username not found: {username}"),
            Err(e) => anyhow::bail!("Username lookup failed for {username}: {e:?}"),
        }
    }
    for rec in &args.recipient {
        match resolve_recipient(manager, rec).await {
            Ok(resolved) => resolved_recipients.push(resolved),
            Err(e) => warn!("Error resolving recipient {}: {:?}", rec, e),
        }
    }
    if resolved_recipients.is_empty() && args.group_id.is_none() {
        anyhow::bail!("No valid recipients specified.");
    }

    let data_message = DataMessage {
        body: args.body_text.clone(),
        attachments,
        body_ranges,
        quote,
        sticker,
        preview: preview.into_iter().collect(),
        story_context,
        is_view_once: Some(args.view_once).filter(|v| *v),
        ..Default::default()
    };

    if let Some(ref gid) = args.group_id {
        let master_key = parse_group_id(gid)?;
        let mut grouped = data_message;
        grouped.group_v2 = Some(GroupContextV2 {
            master_key: Some(master_key.to_vec()),
            revision: Some(0),
            ..Default::default()
        });
        let content_body = ContentBody::DataMessage(grouped);
        manager
            .send_message_to_group(&master_key, content_body, timestamp)
            .await?;
    } else {
        let content_body = ContentBody::DataMessage(data_message);
        for r in resolved_recipients {
            manager
                .send_message(r, content_body.clone(), timestamp)
                .await?;
        }
    }
    Ok(SendResult { timestamp })
}

#[tracing::instrument(skip(manager, body_text))]
pub(crate) async fn core_send(
    manager: &mut RegisteredManager,
    note_to_self: bool,
    recipient: Vec<String>,
    group_id: Option<String>,
    body_text: Option<String>,
    attachment: Vec<String>,
    attachment_dir: Option<PathBuf>,
) -> anyhow::Result<SendResult> {
    core_send_full(
        manager,
        SendArgs {
            note_to_self,
            recipient,
            group_id,
            body_text,
            attachment,
            attachment_dir,
            ..Default::default()
        },
    )
    .await
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
pub(crate) async fn core_receive(
    manager: &mut RegisteredManager,
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

async fn upload_attachments_in_dir(
    attachment_filepath: Vec<String>,
    manager: &RegisteredManager,
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
    let mut manager = load_registered(store).await?;

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
pub(crate) async fn core_send_reaction(
    manager: &mut RegisteredManager,
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
    let mut manager = load_registered(store).await?;

    core_send_receipt(&mut manager, recipient, target_timestamp, r#type).await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_send_receipt(
    manager: &mut RegisteredManager,
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
    let mut manager = load_registered(store).await?;

    core_send_typing(&mut manager, recipient, group_id, stop).await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_send_typing(
    manager: &mut RegisteredManager,
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
    let mut manager = load_registered(store).await?;
    core_remote_delete(&mut manager, recipient, group_id, target_timestamp).await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_remote_delete(
    manager: &mut RegisteredManager,
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

/// Shared recipient + group resolution for content-message senders.
/// Returns `(direct_recipients, group_master_key)`.
async fn resolve_send_targets(
    manager: &mut RegisteredManager,
    note_to_self: bool,
    recipient: Vec<String>,
    username: Option<String>,
    group_id: Option<String>,
) -> anyhow::Result<(Vec<ServiceId>, Option<[u8; 32]>)> {
    if let Some(ref gid) = group_id {
        return Ok((Vec::new(), Some(parse_group_id(gid)?)));
    }
    let mut resolved = Vec::new();
    if note_to_self {
        resolved.push(ServiceId::Aci(
            manager.registration_data().service_ids.aci.into(),
        ));
    }
    if let Some(ref name) = username {
        match manager.lookup_username(name).await {
            Ok(Some(aci)) => {
                let uuid: Uuid = aci.into();
                resolved.push(ServiceId::Aci(uuid.into()));
            }
            Ok(None) => anyhow::bail!("Username not found: {name}"),
            Err(e) => anyhow::bail!("Username lookup failed for {name}: {e:?}"),
        }
    }
    for rec in recipient {
        resolved.push(resolve_recipient(manager, &rec).await?);
    }
    if resolved.is_empty() {
        anyhow::bail!("No valid recipients specified.");
    }
    Ok((resolved, None))
}

async fn deliver_content(
    manager: &mut RegisteredManager,
    targets: (Vec<ServiceId>, Option<[u8; 32]>),
    content_body: ContentBody,
) -> anyhow::Result<u64> {
    let timestamp = now_millis()?;
    match targets.1 {
        Some(master_key) => {
            manager
                .send_message_to_group(&master_key, content_body, timestamp)
                .await?;
        }
        None => {
            for r in targets.0 {
                manager
                    .send_message(r, content_body.clone(), timestamp)
                    .await?;
            }
        }
    }
    Ok(timestamp)
}

#[tracing::instrument(skip(manager))]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn core_send_poll_create(
    manager: &mut RegisteredManager,
    recipient: Vec<String>,
    username: Option<String>,
    note_to_self: bool,
    group_id: Option<String>,
    question: String,
    options: Vec<String>,
    allow_multiple: bool,
) -> anyhow::Result<SendResult> {
    if options.len() < 2 || options.len() > 10 {
        anyhow::bail!("Poll requires between 2 and 10 options.");
    }
    for opt in &options {
        if opt.is_empty() || opt.len() > 100 {
            anyhow::bail!("Each poll option must be between 1 and 100 characters.");
        }
    }
    let data_message = DataMessage {
        poll_create: Some(data_message::PollCreate {
            question: Some(question),
            allow_multiple: Some(allow_multiple),
            options,
        }),
        ..Default::default()
    };
    let targets =
        resolve_send_targets(manager, note_to_self, recipient, username, group_id).await?;
    let timestamp =
        deliver_content(manager, targets, ContentBody::DataMessage(data_message)).await?;
    Ok(SendResult { timestamp })
}

#[tracing::instrument(skip(manager))]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn core_send_poll_vote(
    manager: &mut RegisteredManager,
    recipient: Vec<String>,
    username: Option<String>,
    note_to_self: bool,
    group_id: Option<String>,
    poll_author: String,
    poll_timestamp: i64,
    choices: Vec<u32>,
    vote_count: u32,
) -> anyhow::Result<SendResult> {
    let author_aci = resolve_aci_bytes(manager, &poll_author).await?;
    let data_message = DataMessage {
        poll_vote: Some(data_message::PollVote {
            target_author_aci_binary: Some(author_aci.to_vec()),
            target_sent_timestamp: Some(u64_from_i64_nonnegative(
                poll_timestamp,
                "poll_timestamp",
            )?),
            option_indexes: choices,
            vote_count: Some(vote_count),
        }),
        ..Default::default()
    };
    let targets =
        resolve_send_targets(manager, note_to_self, recipient, username, group_id).await?;
    let timestamp =
        deliver_content(manager, targets, ContentBody::DataMessage(data_message)).await?;
    Ok(SendResult { timestamp })
}

#[tracing::instrument(skip(manager))]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn core_send_poll_terminate(
    manager: &mut RegisteredManager,
    recipient: Vec<String>,
    username: Option<String>,
    note_to_self: bool,
    group_id: Option<String>,
    poll_timestamp: i64,
) -> anyhow::Result<SendResult> {
    let data_message = DataMessage {
        poll_terminate: Some(data_message::PollTerminate {
            target_sent_timestamp: Some(u64_from_i64_nonnegative(
                poll_timestamp,
                "poll_timestamp",
            )?),
        }),
        ..Default::default()
    };
    let targets =
        resolve_send_targets(manager, note_to_self, recipient, username, group_id).await?;
    let timestamp =
        deliver_content(manager, targets, ContentBody::DataMessage(data_message)).await?;
    Ok(SendResult { timestamp })
}

#[tracing::instrument(skip(manager))]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn core_send_pin_message(
    manager: &mut RegisteredManager,
    recipient: Vec<String>,
    username: Option<String>,
    note_to_self: bool,
    group_id: Option<String>,
    target_author: String,
    target_timestamp: i64,
    pin_duration_secs: Option<i64>,
) -> anyhow::Result<SendResult> {
    let author_aci = resolve_aci_bytes(manager, &target_author).await?;
    // CLI default is -1 (forever).
    let pin_duration = match pin_duration_secs.unwrap_or(-1) {
        d if d < 0 => Some(data_message::pin_message::PinDuration::PinDurationForever(
            true,
        )),
        d => Some(data_message::pin_message::PinDuration::PinDurationSeconds(
            crate::utils::u32_from_i64_nonnegative(d, "pin_duration")?,
        )),
    };
    let data_message = DataMessage {
        pin_message: Some(data_message::PinMessage {
            target_author_aci_binary: Some(author_aci.to_vec()),
            target_sent_timestamp: Some(u64_from_i64_nonnegative(
                target_timestamp,
                "target_timestamp",
            )?),
            pin_duration,
        }),
        ..Default::default()
    };
    let targets =
        resolve_send_targets(manager, note_to_self, recipient, username, group_id).await?;
    let timestamp =
        deliver_content(manager, targets, ContentBody::DataMessage(data_message)).await?;
    Ok(SendResult { timestamp })
}

#[tracing::instrument(skip(manager))]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn core_send_unpin_message(
    manager: &mut RegisteredManager,
    recipient: Vec<String>,
    username: Option<String>,
    note_to_self: bool,
    group_id: Option<String>,
    target_author: String,
    target_timestamp: i64,
) -> anyhow::Result<SendResult> {
    let author_aci = resolve_aci_bytes(manager, &target_author).await?;
    let data_message = DataMessage {
        unpin_message: Some(data_message::UnpinMessage {
            target_author_aci_binary: Some(author_aci.to_vec()),
            target_sent_timestamp: Some(u64_from_i64_nonnegative(
                target_timestamp,
                "target_timestamp",
            )?),
        }),
        ..Default::default()
    };
    let targets =
        resolve_send_targets(manager, note_to_self, recipient, username, group_id).await?;
    let timestamp =
        deliver_content(manager, targets, ContentBody::DataMessage(data_message)).await?;
    Ok(SendResult { timestamp })
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_send_payment_notification(
    manager: &mut RegisteredManager,
    recipient: String,
    receipt_b64: String,
    note: Option<String>,
) -> anyhow::Result<SendResult> {
    use base64::Engine;
    let receipt = BASE64_STANDARD
        .decode(receipt_b64.trim())
        .map_err(|e| anyhow::anyhow!("Invalid base64 --receipt: {e:?}"))?;
    let service_id = resolve_recipient(manager, &recipient).await?;
    let data_message = DataMessage {
        payment: Some(data_message::Payment {
            item: Some(data_message::payment::Item::Notification(
                data_message::payment::Notification {
                    transaction: Some(
                        data_message::payment::notification::Transaction::MobileCoin(
                            data_message::payment::notification::MobileCoin {
                                receipt: Some(receipt),
                            },
                        ),
                    ),
                    note,
                },
            )),
        }),
        ..Default::default()
    };
    let timestamp = now_millis()?;
    manager
        .send_message(
            service_id,
            ContentBody::DataMessage(data_message),
            timestamp,
        )
        .await?;
    Ok(SendResult { timestamp })
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_send_admin_delete(
    manager: &mut RegisteredManager,
    group_id: String,
    target_author: String,
    target_timestamp: i64,
) -> anyhow::Result<SendResult> {
    let master_key = parse_group_id(&group_id)?;
    let author_aci = resolve_aci_bytes(manager, &target_author).await?;
    let data_message = DataMessage {
        admin_delete: Some(data_message::AdminDelete {
            target_author_aci_binary: Some(author_aci.to_vec()),
            target_sent_timestamp: Some(u64_from_i64_nonnegative(
                target_timestamp,
                "target_timestamp",
            )?),
        }),
        ..Default::default()
    };
    let timestamp = now_millis()?;
    manager
        .send_message_to_group(
            &master_key,
            ContentBody::DataMessage(data_message),
            timestamp,
        )
        .await?;
    Ok(SendResult { timestamp })
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_send_poll_create_full(
    config_dir: &Path,
    active_account: Option<String>,
    recipient: Vec<String>,
    username: Option<String>,
    note_to_self: bool,
    group_id: Option<String>,
    question: String,
    options: Vec<String>,
    allow_multiple: bool,
) -> anyhow::Result<SendResult> {
    let account = crate::utils::require_account(active_account)?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered(store).await?;
    core_send_poll_create(
        &mut manager,
        recipient,
        username,
        note_to_self,
        group_id,
        question,
        options,
        allow_multiple,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_send_poll_vote_full(
    config_dir: &Path,
    active_account: Option<String>,
    recipient: Vec<String>,
    username: Option<String>,
    note_to_self: bool,
    group_id: Option<String>,
    poll_author: String,
    poll_timestamp: i64,
    choices: Vec<u32>,
    vote_count: u32,
) -> anyhow::Result<SendResult> {
    let account = crate::utils::require_account(active_account)?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered(store).await?;
    core_send_poll_vote(
        &mut manager,
        recipient,
        username,
        note_to_self,
        group_id,
        poll_author,
        poll_timestamp,
        choices,
        vote_count,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_send_poll_terminate_full(
    config_dir: &Path,
    active_account: Option<String>,
    recipient: Vec<String>,
    username: Option<String>,
    note_to_self: bool,
    group_id: Option<String>,
    poll_timestamp: i64,
) -> anyhow::Result<SendResult> {
    let account = crate::utils::require_account(active_account)?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered(store).await?;
    core_send_poll_terminate(
        &mut manager,
        recipient,
        username,
        note_to_self,
        group_id,
        poll_timestamp,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_send_pin_message(
    config_dir: &Path,
    active_account: Option<String>,
    recipient: Vec<String>,
    username: Option<String>,
    note_to_self: bool,
    group_id: Option<String>,
    pin_duration: Option<i64>,
    target_author: String,
    target_timestamp: i64,
) -> anyhow::Result<SuccessResult> {
    let account = crate::utils::require_account(active_account)?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered(store).await?;
    core_send_pin_message(
        &mut manager,
        recipient,
        username,
        note_to_self,
        group_id,
        target_author,
        target_timestamp,
        pin_duration,
    )
    .await?;
    Ok(SuccessResult {
        success: true,
        message: "Pin message sent successfully.".to_string(),
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_send_unpin_message(
    config_dir: &Path,
    active_account: Option<String>,
    recipient: Vec<String>,
    username: Option<String>,
    note_to_self: bool,
    group_id: Option<String>,
    target_author: String,
    target_timestamp: i64,
) -> anyhow::Result<SuccessResult> {
    let account = crate::utils::require_account(active_account)?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered(store).await?;
    core_send_unpin_message(
        &mut manager,
        recipient,
        username,
        note_to_self,
        group_id,
        target_author,
        target_timestamp,
    )
    .await?;
    Ok(SuccessResult {
        success: true,
        message: "Unpin message sent successfully.".to_string(),
    })
}

pub(crate) async fn run_send_payment_notification(
    config_dir: &Path,
    active_account: Option<String>,
    recipient: String,
    receipt: String,
    note: Option<String>,
) -> anyhow::Result<SendResult> {
    let account = crate::utils::require_account(active_account)?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered(store).await?;
    core_send_payment_notification(&mut manager, recipient, receipt, note).await
}

pub(crate) async fn run_send_admin_delete(
    config_dir: &Path,
    active_account: Option<String>,
    group_id: String,
    target_author: String,
    target_timestamp: i64,
) -> anyhow::Result<SendResult> {
    let account = crate::utils::require_account(active_account)?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered(store).await?;
    core_send_admin_delete(&mut manager, group_id, target_author, target_timestamp).await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_send_full(
    config_dir: &Path,
    active_account: Option<String>,
    args: SendArgs,
    message_from_stdin: bool,
    attachment_dir: Option<PathBuf>,
) -> anyhow::Result<SendResult> {
    let account = crate::utils::require_account(active_account)?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered(store).await?;
    let mut args = args;
    if message_from_stdin {
        use tokio::io::AsyncReadExt;
        let mut buffer = String::new();
        tokio::io::stdin().read_to_string(&mut buffer).await?;
        args.body_text = Some(buffer.trim().to_string());
    }
    if args.attachment_dir.is_none() {
        args.attachment_dir = attachment_dir;
    }
    core_send_full(&mut manager, args).await
}

#[cfg(test)]
mod send_args_tests {
    use super::parse_text_style;

    #[test]
    fn text_style_parses_bold_range() {
        let range = parse_text_style("10:3:BOLD").unwrap();
        assert_eq!(range.start, Some(10));
        assert_eq!(range.length, Some(3));
    }

    #[test]
    fn text_style_rejects_unknown_style() {
        assert!(parse_text_style("0:9:UNDERLINE").is_err());
    }

    #[test]
    fn text_style_rejects_bad_shape() {
        assert!(parse_text_style("10:BOLD").is_err());
        assert!(parse_text_style("x:3:BOLD").is_err());
    }
}
