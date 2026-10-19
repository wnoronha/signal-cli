use crate::cli::OutputMode;
use crate::errors::SignalError;
use serde::{Deserialize, Serialize};

/// Marker: [`render_command_error`] already wrote the failure to stdout/stderr.
#[derive(Debug)]
pub struct ErrorReported;

impl std::fmt::Display for ErrorReported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("command error already reported")
    }
}

impl std::error::Error for ErrorReported {}

#[derive(Serialize)]
struct CliErrorBody {
    error: String,
    code: i32,
}

/// Emit a command failure in plain text or JSON (aligned with JSON-RPC error codes).
pub fn render_command_error(err: &anyhow::Error, mode: OutputMode) {
    let (code, message) = if let Some(signal_err) = err.downcast_ref::<SignalError>() {
        (signal_err.json_rpc_code(), signal_err.to_string())
    } else {
        (-32603, err.to_string())
    };
    match mode {
        OutputMode::PlainText => eprintln!("{message}"),
        OutputMode::Json => {
            let body = CliErrorBody {
                error: message,
                code,
            };
            println!(
                "{}",
                serde_json::to_string(&body).unwrap_or_else(|_| "{\"error\":\"unknown\"}".into())
            );
        }
    }
}

/// A trait for types that can be rendered to the console in multiple formats.
pub trait Render: Serialize {
    fn render_text(&self);
    fn render_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    fn render(&self, mode: OutputMode) {
        match mode {
            OutputMode::PlainText => self.render_text(),
            OutputMode::Json => println!("{}", self.render_json()),
        }
    }
}

#[derive(Serialize)]
pub struct AccountList {
    pub accounts: Vec<String>,
}

impl Render for AccountList {
    fn render_text(&self) {
        if self.accounts.is_empty() {
            println!("No accounts registered.");
        } else {
            for account in &self.accounts {
                println!("{}", account);
            }
        }
    }
}

#[derive(Serialize)]
pub struct SendResult {
    pub timestamp: u64,
}

impl Render for SendResult {
    fn render_text(&self) {
        println!("Message sent successfully.");
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GroupInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub members_count: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GroupList {
    pub groups: Vec<GroupInfo>,
    pub detailed: bool,
}

impl Render for GroupList {
    fn render_text(&self) {
        for g in &self.groups {
            if self.detailed {
                println!("Group ID: {}", g.id);
                println!("Title: {}", g.name);
                if let Some(ref desc) = g.description {
                    println!("Description: {}", desc);
                }
                println!("Members count: {}", g.members_count);
                println!("-------------------------------------------");
            } else {
                println!("{} - {}", g.id, g.name);
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ContactInfo {
    pub uuid: String,
    pub phone: Option<String>,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ContactList {
    pub contacts: Vec<ContactInfo>,
    pub detailed: bool,
}

impl Render for ContactList {
    fn render_text(&self) {
        for c in &self.contacts {
            if self.detailed {
                println!("UUID: {}", c.uuid);
                println!("Phone: {:?}", c.phone);
                println!("Name: {}", c.name);
                println!("-------------------------------------------");
            } else {
                println!("{} / {:?} / {}", c.uuid, c.phone, c.name);
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserStatus {
    pub recipient: String,
    pub is_registered: bool,
    pub aci: Option<String>,
}

impl Render for Vec<UserStatus> {
    fn render_text(&self) {
        for s in self {
            if s.is_registered {
                println!(
                    "Recipient: {} is registered on Signal as Aci: {}",
                    s.recipient,
                    s.aci.as_deref().unwrap_or("unknown")
                );
            } else {
                println!("Recipient: {} is NOT registered on Signal.", s.recipient);
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IdentityInfo {
    pub recipient: String,
    pub uuid: String,
    pub fingerprint: String,
    pub trust_level: String,
    pub added: u64,
}

impl Render for Vec<IdentityInfo> {
    fn render_text(&self) {
        for identity in self {
            println!("Recipient: {}", identity.recipient);
            println!("UUID: {}", identity.uuid);
            println!("Fingerprint: {}", identity.fingerprint);
            println!("Trust level: {}", identity.trust_level);
            println!("Added: {}", identity.added);
            println!("-------------------------------------------");
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DeviceInfo {
    pub id: u32,
    pub name: Option<String>,
    pub created_at: u64,
    pub last_seen: u64,
    pub is_this_device: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DeviceList {
    pub devices: Vec<DeviceInfo>,
}

impl Render for DeviceList {
    fn render_text(&self) {
        for device in &self.devices {
            let device_name = device.name.as_deref().unwrap_or("(no device name)");
            let current_marker = if device.is_this_device {
                "(this device)"
            } else {
                ""
            };
            println!(
                "- Device {} {}\n  Name: {}\n  Created: {}\n  Last seen: {}",
                device.id, current_marker, device_name, device.created_at, device.last_seen
            );
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProfileInfo {
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub about: Option<String>,
    pub about_emoji: Option<String>,
}

impl Render for ProfileInfo {
    fn render_text(&self) {
        println!("Profile updated successfully.");
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SuccessResult {
    pub success: bool,
    pub message: String,
}

impl Render for SuccessResult {
    fn render_text(&self) {
        println!("{}", self.message);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MediaData {
    pub data: String, // Base64 encoded
}

impl Render for MediaData {
    fn render_text(&self) {
        use base64::Engine;
        use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
        use std::io::Write;

        if let Ok(bytes) = BASE64_STANDARD.decode(&self.data) {
            let _ = std::io::stdout().write_all(&bytes);
            let _ = std::io::stdout().flush();
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StickerInfo {
    pub id: u32,
    pub emoji: Option<String>,
    pub content_type: Option<String>,
    pub data: Option<String>, // Base64 encoded
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StickerPackInfo {
    pub id: String,  // Hex encoded
    pub key: String, // Hex encoded
    pub title: String,
    pub author: String,
    pub stickers: Vec<StickerInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StickerPackList {
    pub packs: Vec<StickerPackInfo>,
}

impl Render for StickerPackList {
    fn render_text(&self) {
        if self.packs.is_empty() {
            println!("No sticker packs installed.");
        } else {
            for pack in &self.packs {
                println!("Pack: {} by {}", pack.title, pack.author);
                println!("ID: {}", pack.id);
                println!("Stickers: {}", pack.stickers.len());
                println!("-------------------------------------------");
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReceiveEnvelope {
    pub source: Option<String>,
    pub source_uuid: Option<String>,
    pub source_name: Option<String>,
    pub source_device: Option<u32>,
    pub timestamp: u64,
    pub server_received_timestamp: Option<u64>,
    pub server_delivered_timestamp: Option<u64>,
    pub data_message: Option<DataMessageEnvelope>,
    pub typing_message: Option<TypingMessageEnvelope>,
    pub call_message: Option<CallMessageEnvelope>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TypingMessageEnvelope {
    pub action: String,
    pub timestamp: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CallMessageEnvelope {
    pub offer_message: Option<serde_json::Value>,
    pub ice_update_messages: Option<Vec<serde_json::Value>>,
    pub hangup_message: Option<serde_json::Value>,
    pub busy_message: Option<serde_json::Value>,
}

impl Render for Vec<ReceiveEnvelope> {
    fn render_text(&self) {
        if self.is_empty() {
            println!("No new messages.");
        } else {
            for envelope in self {
                println!(
                    "Envelope from: {}",
                    envelope.source_uuid.as_deref().unwrap_or("unknown")
                );
                println!("Timestamp: {}", envelope.timestamp);
                if let Some(ref dm) = envelope.data_message
                    && let Some(ref body) = dm.message
                {
                    println!("Message body: {}", body);
                }
                println!("-------------------------------------------");
            }
        }
    }

    fn render_json(&self) -> String {
        // For receive, signal-cli often prints each envelope on a new line as a separate JSON object
        let mut output = String::new();
        for envelope in self {
            if let Ok(json) = serde_json::to_string(envelope) {
                output.push_str(&json);
                output.push('\n');
            }
        }
        output.trim_end().to_string()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DataMessageEnvelope {
    pub message: Option<String>,
    pub attachments: Option<Vec<AttachmentEnvelope>>,
    pub sticker: Option<StickerEnvelope>,
    pub reaction: Option<ReactionEnvelope>,
    pub expires_in_seconds: u32,
    pub is_expiration_update: bool,
    pub view_once: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StickerEnvelope {
    pub pack_id: String,
    pub sticker_id: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReactionEnvelope {
    pub emoji: String,
    pub target_author: Option<String>,
    pub target_author_uuid: Option<String>,
    pub target_sent_timestamp: u64,
    pub is_remove: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentEnvelope {
    pub id: String,
    pub content_type: String,
    pub filename: Option<String>,
    pub size: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub caption: Option<String>,
    pub upload_timestamp: Option<u64>,
}
