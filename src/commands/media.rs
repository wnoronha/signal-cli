use anyhow::anyhow;
use base64::prelude::*;
use std::path::Path;

use presage::Manager;
use presage::libsignal_service::content::{ContentBody, GroupContextV2};
use presage::libsignal_service::prelude::Uuid;
use presage::libsignal_service::prelude::phonenumber::PhoneNumber;
use presage::libsignal_service::proto::attachment_pointer::AttachmentIdentifier;
use presage::libsignal_service::protocol::ServiceId;
use presage::libsignal_service::zkgroup::profiles::ProfileKey;
use presage::manager::Registered;
use presage::store::{Store, Thread};
use std::str::FromStr;

use crate::output::MediaData;
use crate::utils::{open_sqlite_store, resolve_recipient};

use super::{load_registered_and_receive, parse_group_id};

pub(crate) async fn run_get_avatar(
    config_dir: &Path,
    active_account: Option<String>,
    contact: Option<String>,
    profile: Option<String>,
    group_id: Option<String>,
) -> anyhow::Result<MediaData> {
    let account = crate::utils::require_account(active_account)?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;
    core_get_avatar(&mut manager, contact, profile, group_id).await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_get_avatar<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    contact: Option<String>,
    profile: Option<String>,
    group_id: Option<String>,
) -> anyhow::Result<MediaData> {
    let avatar_bytes = if let Some(gid) = group_id {
        let master_key = parse_group_id(&gid)?;
        let group = manager
            .store()
            .group(master_key)
            .await?
            .ok_or_else(|| anyhow!("Group not found in local database."))?;

        let context = GroupContextV2 {
            master_key: Some(master_key.to_vec()),
            revision: Some(group.revision),
            ..Default::default()
        };

        manager.retrieve_group_avatar(context).await?
    } else if let Some(ident) = contact.or(profile) {
        let service_id = if let Ok(u) = Uuid::parse_str(&ident) {
            ServiceId::Aci(u.into())
        } else if ident.starts_with('+') {
            let phone = PhoneNumber::from_str(&ident)?;
            let discovered = manager
                .discover_contacts_by_phone_number(vec![phone])
                .await?;
            if let Some(id) = discovered.into_iter().filter_map(|(_, id)| id).next() {
                id
            } else {
                anyhow::bail!("Could not resolve phone number to Signal ID.");
            }
        } else {
            anyhow::bail!("Invalid contact identifier. Use UUID or phone number.");
        };

        let contact_details = manager
            .store()
            .contact_by_id(&service_id)
            .await?
            .ok_or_else(|| anyhow!("Contact not found in local database."))?;

        let uuid = match service_id {
            ServiceId::Aci(u) => Uuid::from(u),
            ServiceId::Pni(u) => Uuid::from(u),
        };

        let profile_key_bytes: [u8; 32] = contact_details
            .profile_key
            .as_slice()
            .try_into()
            .map_err(|_| anyhow!("Invalid profile key length for contact."))?;
        let profile_key = ProfileKey::create(profile_key_bytes);

        manager
            .retrieve_profile_avatar_by_uuid(uuid, profile_key)
            .await?
    } else {
        anyhow::bail!("Either --contact, --profile, or --group-id must be specified.");
    };

    if let Some(bytes) = avatar_bytes {
        Ok(MediaData {
            data: BASE64_STANDARD.encode(bytes),
        })
    } else {
        anyhow::bail!("No avatar found.");
    }
}

pub(crate) async fn run_get_attachment(
    config_dir: &Path,
    active_account: Option<String>,
    id: String,
    recipient: Option<String>,
    group_id: Option<String>,
) -> anyhow::Result<MediaData> {
    let account = crate::utils::require_account(active_account)?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;
    core_get_attachment(&mut manager, id, recipient, group_id).await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_get_attachment<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    id: String,
    recipient: Option<String>,
    group_id: Option<String>,
) -> anyhow::Result<MediaData> {
    let thread = if let Some(gid) = group_id {
        Thread::Group(parse_group_id(&gid)?)
    } else if let Some(rec) = recipient {
        let service_id = resolve_recipient(manager, &rec).await?;
        Thread::Contact(service_id)
    } else {
        anyhow::bail!(
            "Either --recipient or --group-id must be specified to locate the attachment."
        );
    };

    let messages = manager.store().messages(&thread, ..).await?;
    let mut attachment_pointer = None;

    // Bound the scan: huge threads must not be fully loaded to find one id.
    const MAX_THREAD_SCAN: usize = 1000;
    let mut scanned = 0usize;
    for content_res in messages {
        if scanned >= MAX_THREAD_SCAN {
            break;
        }
        scanned += 1;
        let content = content_res.map_err(|e| anyhow!("Store error: {:?}", e))?;
        if let ContentBody::DataMessage(dm) = content.body {
            for att in dm.attachments {
                let matches = match &att.attachment_identifier {
                    Some(AttachmentIdentifier::CdnId(cid)) => cid.to_string() == id,
                    Some(AttachmentIdentifier::CdnKey(key)) => key == &id,
                    _ => false,
                };
                if matches {
                    attachment_pointer = Some(att);
                    break;
                }
            }
        }
        if attachment_pointer.is_some() {
            break;
        }
    }

    let pointer = attachment_pointer.ok_or_else(|| {
        anyhow!(
            "Attachment with ID {} not found in the most recent {scanned} thread message(s).",
            id
        )
    })?;
    let bytes = manager.get_attachment(&pointer).await?;

    Ok(MediaData {
        data: BASE64_STANDARD.encode(bytes),
    })
}
