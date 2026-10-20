use anyhow::anyhow;
use futures::{StreamExt, pin_mut};
use std::path::Path;
use std::str::FromStr;

use presage::Manager;
use presage::libsignal_service::prelude::Uuid;
use presage::libsignal_service::prelude::phonenumber::PhoneNumber;
use presage::libsignal_service::protocol::ServiceId;
use presage::libsignal_service::zkgroup::profiles::ProfileKey;
use presage::manager::Registered;
use presage::model::contacts::Contact;
use presage::model::messages::Received;
use presage::store::Store;

use crate::errors::SignalError;
use crate::output::{ContactInfo, ContactList, ProfileInfo, SuccessResult};
use crate::utils::{open_sqlite_store, resolve_recipient, u32_from_i64_nonnegative};

use super::{load_registered, load_registered_and_receive};

pub(crate) async fn run_list_contacts(
    config_dir: &Path,
    active_account: Option<String>,
    name: Option<String>,
    detailed: bool,
) -> anyhow::Result<ContactList> {
    let account = crate::utils::require_account(active_account)?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;
    core_list_contacts(&mut manager, name, detailed).await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_list_contacts<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    name: Option<String>,
    detailed: bool,
) -> anyhow::Result<ContactList> {
    let mut contacts = Vec::new();
    for contact in manager.store().contacts().await?.flatten() {
        if let Some(ref n) = name
            && !contact.name.contains(n)
        {
            continue;
        }
        let mut info = ContactInfo {
            uuid: contact.uuid.to_string(),
            phone: contact.phone_number.map(|p| p.to_string()),
            name: contact.name.clone(),
            given_name: None,
            family_name: None,
            about: None,
            about_emoji: None,
        };
        if detailed
            && contact.profile_key.len() == 32
            && let Ok(key_bytes) = contact.profile_key.as_slice().try_into()
        {
            let profile_key = ProfileKey::create(key_bytes);
            // Bound worst-case latency: one slow profile fetch must not stall
            // the whole list (N+1 lookups); failures stay non-fatal.
            let lookup = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                manager.retrieve_profile_by_uuid(contact.uuid, profile_key),
            )
            .await;
            if let Ok(Ok(profile)) = lookup {
                if let Some(name) = profile.name {
                    info.given_name = Some(name.given_name);
                    info.family_name = name.family_name;
                }
                info.about = profile.about;
                info.about_emoji = profile.about_emoji;
            }
        }
        contacts.push(info);
    }
    Ok(ContactList { contacts, detailed })
}

pub(crate) async fn run_update_profile(
    config_dir: &Path,
    active_account: Option<String>,
    given_name: Option<String>,
    family_name: Option<String>,
    about: Option<String>,
    about_emoji: Option<String>,
) -> anyhow::Result<ProfileInfo> {
    let account = crate::utils::require_account(active_account)?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;

    core_update_profile(&mut manager, given_name, family_name, about, about_emoji).await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_update_profile<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    given_name: Option<String>,
    family_name: Option<String>,
    about: Option<String>,
    about_emoji: Option<String>,
) -> anyhow::Result<ProfileInfo> {
    if let Some(first_name) = given_name.clone() {
        let name = presage::libsignal_service::profile_name::ProfileName {
            given_name: first_name,
            family_name: family_name.clone(),
        };
        manager
            .update_profile(name, about.clone(), about_emoji.clone())
            .await?;
        Ok(ProfileInfo {
            given_name,
            family_name,
            about,
            about_emoji,
        })
    } else {
        Err(SignalError::Internal(
            "given_name (--given-name) is required to update the profile.".to_string(),
        )
        .into())
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_update_contact(
    config_dir: &Path,
    active_account: Option<String>,
    recipient: String,
    given_name: Option<String>,
    family_name: Option<String>,
    _nick_given_name: Option<String>,
    _nick_family_name: Option<String>,
    _note: Option<String>,
    expiration: Option<i64>,
    phone_number: Option<String>,
) -> anyhow::Result<SuccessResult> {
    let account = crate::utils::require_account(active_account)?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;

    core_update_contact(
        &mut manager,
        recipient,
        given_name,
        family_name,
        expiration,
        phone_number,
    )
    .await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_update_contact<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    recipient: String,
    given_name: Option<String>,
    family_name: Option<String>,
    expiration: Option<i64>,
    phone_number: Option<String>,
) -> anyhow::Result<SuccessResult> {
    let mut store = manager.store().clone();
    let uuid = if let Ok(u) = Uuid::parse_str(&recipient) {
        u
    } else {
        let service_id = resolve_recipient(manager, &recipient).await?;
        match service_id {
            ServiceId::Aci(u) => Uuid::from(u),
            ServiceId::Pni(u) => Uuid::from(u),
        }
    };

    let mut contact = match store.contact_by_id(&ServiceId::Aci(uuid.into())).await? {
        Some(c) => c,
        None => {
            Contact {
                uuid,
                phone_number: None,
                name: String::new(),
                verified: Default::default(),
                // Empty (not zero) so detailed profile lookups skip it
                // until a real profile key is synced.
                profile_key: Vec::new(),
                expire_timer: 0,
                expire_timer_version: 0,
                inbox_position: 0,
                avatar: None,
            }
        }
    };

    if let Some(name) = given_name {
        contact.name = name;
        if let Some(f_name) = family_name {
            contact.name.push(' ');
            contact.name.push_str(&f_name);
        }
    }

    if let Some(e) = expiration {
        contact.expire_timer = u32_from_i64_nonnegative(e, "expiration")?;
    }

    if let Some(number) = phone_number {
        contact.phone_number = Some(PhoneNumber::from_str(&number)?);
    }

    store.save_contact(&contact).await?;

    Ok(SuccessResult {
        success: true,
        message: format!("Contact {} updated successfully.", recipient),
    })
}

pub(crate) async fn run_send_sync_request(
    config_dir: &Path,
    active_account: Option<String>,
) -> anyhow::Result<SuccessResult> {
    let account = crate::utils::require_account(active_account)?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered(store).await?;
    core_send_sync_request(&mut manager).await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_send_sync_request<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
) -> anyhow::Result<SuccessResult> {
    manager.request_contacts().await?;

    let messages = manager
        .receive_messages()
        .await
        .map_err(|e| anyhow!("Failed to start receive stream after sync request: {e}"))?;
    pin_mut!(messages);

    while let Some(content) = messages.next().await {
        match content {
            Received::Contacts => {
                return Ok(SuccessResult {
                    success: true,
                    message: "Contact sync completed.".to_string(),
                });
            }
            Received::QueueEmpty => {
                return Ok(SuccessResult {
                    success: true,
                    message: "Sync request sent; contact list may update on the next receive."
                        .to_string(),
                });
            }
            Received::Content(_) | Received::DecryptionError(_) => {}
        }
    }

    Ok(SuccessResult {
        success: true,
        message: "Sync request sent.".to_string(),
    })
}
