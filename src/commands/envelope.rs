//! Build Java-shaped `ReceiveEnvelope` values from presage `Content`.

use presage::Manager;
use presage::libsignal_service::content::{Content, ContentBody};
use presage::store::{Store, Thread};

use crate::output::{
    AttachmentEnvelope, CallMessageEnvelope, DataMessageEnvelope, ReactionEnvelope,
    ReceiveEnvelope, StickerEnvelope, TypingMessageEnvelope,
};

use super::group_id::encode_group_id_b64;
use crate::utils::saturating_millis;

pub async fn build_receive_envelope<S: Store>(
    manager: &Manager<S, presage::manager::Registered>,
    content: &Content,
) -> ReceiveEnvelope {
    let source_uuid = content.metadata.sender.service_id_string();
    let timestamp = saturating_millis(content.metadata.server_timestamp.timestamp_millis());

    let (thread_title, group_id) = match Thread::try_from(content) {
        Ok(thread) => {
            let title = manager.thread_title(&thread).await.ok();
            let gid = match thread {
                Thread::Group(key) => Some(encode_group_id_b64(&key)),
                Thread::Contact(_) => None,
            };
            (title, gid)
        }
        Err(_) => (None, None),
    };

    ReceiveEnvelope {
        source: None,
        source_number: None,
        source_uuid: Some(source_uuid),
        source_name: thread_title.clone(),
        thread_title,
        group_id,
        source_device: Some(content.metadata.sender_device.into()),
        timestamp,
        server_received_timestamp: None,
        server_delivered_timestamp: None,
        data_message: data_message_envelope(&content.body, timestamp),
        typing_message: typing_envelope(&content.body),
        call_message: call_envelope(&content.body),
    }
}

fn data_message_envelope(body: &ContentBody, timestamp: u64) -> Option<DataMessageEnvelope> {
    let ContentBody::DataMessage(dm) = body else {
        return None;
    };
    Some(DataMessageEnvelope {
        timestamp,
        message: dm.body.clone(),
        attachments: Some(
            dm.attachments
                .iter()
                .map(|a| AttachmentEnvelope {
                    id: a
                        .attachment_identifier
                        .as_ref()
                        .map(crate::utils::attachment_id_str)
                        .unwrap_or_default(),
                    content_type: a.content_type.clone().unwrap_or_default(),
                    filename: a.file_name.clone(),
                    size: a.size.map(|s| s as u64),
                    width: a.width,
                    height: a.height,
                    caption: a.caption.clone(),
                    upload_timestamp: a.upload_timestamp,
                })
                .collect(),
        ),
        sticker: dm.sticker.as_ref().map(|s| StickerEnvelope {
            pack_id: s.pack_id.as_ref().map(hex::encode).unwrap_or_default(),
            sticker_id: s.sticker_id.unwrap_or_default(),
        }),
        reaction: dm.reaction.as_ref().map(|r| ReactionEnvelope {
            emoji: r.emoji.clone().unwrap_or_default(),
            target_author: None,
            target_author_uuid: r.target_author_aci.clone(),
            target_sent_timestamp: r.target_sent_timestamp.unwrap_or_default(),
            is_remove: r.remove.unwrap_or_default(),
        }),
        expires_in_seconds: dm.expire_timer.unwrap_or_default(),
        is_expiration_update: false,
        view_once: dm.is_view_once.unwrap_or_default(),
        mentions: Vec::new(),
        contacts: Vec::new(),
    })
}

fn typing_envelope(body: &ContentBody) -> Option<TypingMessageEnvelope> {
    let ContentBody::TypingMessage(tm) = body else {
        return None;
    };
    Some(TypingMessageEnvelope {
        action: match tm.action {
            Some(a)
                if a == presage::libsignal_service::proto::typing_message::Action::Started
                    as i32 =>
            {
                "STARTED".to_string()
            }
            Some(a)
                if a == presage::libsignal_service::proto::typing_message::Action::Stopped
                    as i32 =>
            {
                "STOPPED".to_string()
            }
            _ => "UNKNOWN".to_string(),
        },
        timestamp: tm.timestamp.unwrap_or_default(),
    })
}

fn call_envelope(body: &ContentBody) -> Option<CallMessageEnvelope> {
    let ContentBody::CallMessage(cm) = body else {
        return None;
    };
    let null = || serde_json::Value::Null;
    Some(CallMessageEnvelope {
        offer_message: cm
            .offer
            .as_ref()
            .map(|o| serde_json::to_value(format!("{o:?}")).unwrap_or_else(|_| null())),
        ice_update_messages: Some(
            cm.ice_update
                .iter()
                .map(|i| serde_json::to_value(format!("{i:?}")).unwrap_or_else(|_| null()))
                .collect(),
        ),
        hangup_message: cm
            .hangup
            .as_ref()
            .map(|h| serde_json::to_value(format!("{h:?}")).unwrap_or_else(|_| null())),
        busy_message: cm
            .busy
            .as_ref()
            .map(|b| serde_json::to_value(format!("{b:?}")).unwrap_or_else(|_| null())),
    })
}

#[cfg(test)]
mod tests {
    use crate::output::DataMessageEnvelope;

    #[test]
    fn data_message_json_includes_mentions_and_contacts_arrays() {
        let dm = DataMessageEnvelope {
            timestamp: 1,
            message: Some("hi".into()),
            attachments: Some(vec![]),
            sticker: None,
            reaction: None,
            expires_in_seconds: 0,
            is_expiration_update: false,
            view_once: false,
            mentions: Vec::new(),
            contacts: Vec::new(),
        };
        let json: serde_json::Value = serde_json::to_value(&dm).unwrap();
        assert!(json.get("mentions").unwrap().is_array());
        assert!(json.get("contacts").unwrap().is_array());
        assert_eq!(json.get("timestamp").unwrap(), 1);
    }
}
