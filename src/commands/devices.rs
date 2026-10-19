use anyhow::anyhow;
use std::path::Path;

use presage::Manager;
use presage::libsignal_service::prelude::phonenumber::PhoneNumber;
use presage::manager::Registered;
use presage::store::Store;
use std::str::FromStr;

use crate::output::{DeviceInfo, DeviceList, SuccessResult, UserStatus, WhoAmIInfo};
use crate::utils::{open_sqlite_store, resolve_recipient};
use presage::libsignal_service::protocol::ServiceId;
use presage::libsignal_service::zkgroup::profiles::ProfileKey;

use super::{load_registered, load_registered_and_receive};

pub(crate) async fn run_add_device(
    config_dir: &Path,
    active_account: Option<String>,
    uri: String,
) -> anyhow::Result<()> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;
    let url = url::Url::parse(&uri)?;
    manager.link_secondary(url).await?;
    println!("Added new secondary device.");
    Ok(())
}

pub(crate) async fn run_list_devices(
    config_dir: &Path,
    active_account: Option<String>,
) -> anyhow::Result<DeviceList> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;
    core_list_devices(&mut manager).await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_list_devices<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
) -> anyhow::Result<DeviceList> {
    let devices = manager.devices().await?;
    let current_device_id = manager.device_id();

    let device_infos = devices
        .into_iter()
        .map(|device| DeviceInfo {
            id: device.id.into(),
            name: device.name,
            created_at: device.created_at.timestamp_millis() as u64,
            last_seen: device.last_seen.timestamp_millis() as u64,
            is_this_device: device.id == current_device_id,
        })
        .collect();

    Ok(DeviceList {
        devices: device_infos,
    })
}

pub(crate) async fn run_remove_device(
    config_dir: &Path,
    active_account: Option<String>,
    device_id: i64,
) -> anyhow::Result<SuccessResult> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;
    core_remove_device(&mut manager, device_id as u32).await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_remove_device<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    device_id: u32,
) -> anyhow::Result<SuccessResult> {
    manager.unlink_secondary(device_id).await?;
    Ok(SuccessResult {
        success: true,
        message: format!("Unlinked device with id: {}", device_id),
    })
}

pub(crate) async fn run_get_user_status(
    config_dir: &Path,
    active_account: Option<String>,
    recipients: Vec<String>,
    usernames: Vec<String>,
) -> anyhow::Result<Vec<UserStatus>> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;
    core_get_user_status(&mut manager, recipients, usernames).await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_get_user_status<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    recipients: Vec<String>,
    usernames: Vec<String>,
) -> anyhow::Result<Vec<UserStatus>> {
    let mut statuses = Vec::new();
    for rec in recipients {
        if let Ok(phone) = PhoneNumber::from_str(&rec) {
            match manager.discover_contacts_by_phone_number(vec![phone]).await {
                Ok(discovered) => {
                    for (p, identity) in discovered {
                        statuses.push(UserStatus {
                            recipient: p.to_string(),
                            is_registered: identity.is_some(),
                            aci: identity.map(|id| id.service_id_string()),
                            given_name: None,
                            family_name: None,
                            about: None,
                            about_emoji: None,
                        });
                    }
                }
                Err(_) => statuses.push(UserStatus {
                    recipient: rec,
                    is_registered: false,
                    aci: None,
                    given_name: None,
                    family_name: None,
                    about: None,
                    about_emoji: None,
                }),
            }
        }
    }

    for user in usernames {
        match manager.lookup_username(&user).await {
            Ok(Some(service_id)) => statuses.push(UserStatus {
                recipient: user,
                is_registered: true,
                aci: Some(service_id.service_id_string()),
                given_name: None,
                family_name: None,
                about: None,
                about_emoji: None,
            }),
            Ok(None) => statuses.push(UserStatus {
                recipient: user,
                is_registered: false,
                aci: None,
                given_name: None,
                family_name: None,
                about: None,
                about_emoji: None,
            }),
            Err(_) => statuses.push(UserStatus {
                recipient: user,
                is_registered: false,
                aci: None,
                given_name: None,
                family_name: None,
                about: None,
                about_emoji: None,
            }),
        }
    }
    for status in &mut statuses {
        if status.is_registered {
            enrich_user_status(manager, status).await;
        }
    }
    Ok(statuses)
}

pub(crate) async fn run_whoami(
    config_dir: &Path,
    active_account: Option<String>,
) -> anyhow::Result<WhoAmIInfo> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let manager = load_registered(store).await?;
    core_whoami(&manager).await
}

pub(crate) async fn core_whoami<S: Store + Send + 'static>(
    manager: &Manager<S, Registered>,
) -> anyhow::Result<WhoAmIInfo> {
    let who = manager.whoami().await?;
    Ok(WhoAmIInfo {
        number: who.number.to_string(),
        uuid: who.aci.to_string(),
        pni: (who.pni != who.aci).then(|| who.pni.to_string()),
        device_id: u32::from(manager.device_id()),
    })
}

pub(crate) async fn run_clear_sessions(
    config_dir: &Path,
    active_account: Option<String>,
    recipient: String,
) -> anyhow::Result<SuccessResult> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let manager = load_registered(store).await?;
    core_clear_sessions(&manager, recipient).await
}

pub(crate) async fn core_clear_sessions<S: Store + Send + 'static>(
    manager: &Manager<S, Registered>,
    recipient: String,
) -> anyhow::Result<SuccessResult> {
    let mut resolver = manager.clone();
    let service_id = resolve_recipient(&mut resolver, &recipient).await?;
    manager.clear_sessions(&service_id).await?;
    Ok(SuccessResult {
        success: true,
        message: format!("Cleared sessions for {recipient}."),
    })
}

pub(crate) async fn run_send_session_reset(
    config_dir: &Path,
    active_account: Option<String>,
    recipient: String,
    timestamp: Option<u64>,
) -> anyhow::Result<SuccessResult> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered(store).await?;
    core_send_session_reset(&mut manager, recipient, timestamp).await
}

pub(crate) async fn core_send_session_reset<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    recipient: String,
    timestamp: Option<u64>,
) -> anyhow::Result<SuccessResult> {
    let mut resolver = manager.clone();
    let service_id = resolve_recipient(&mut resolver, &recipient).await?;
    let ts = timestamp.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis() as u64
    });
    manager.send_session_reset(&service_id, ts).await?;
    Ok(SuccessResult {
        success: true,
        message: format!("Session reset sent to {recipient}."),
    })
}

async fn enrich_user_status<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    status: &mut UserStatus,
) {
    let Some(ref aci_str) = status.aci else {
        return;
    };
    let Ok(uuid) = uuid::Uuid::parse_str(aci_str) else {
        return;
    };
    let service_id = ServiceId::Aci(uuid.into());
    let Ok(Some(contact)) = manager.store().contact_by_id(&service_id).await else {
        return;
    };
    if contact.profile_key.len() != 32 {
        return;
    }
    let Ok(key_bytes) = contact.profile_key.as_slice().try_into() else {
        return;
    };
    let profile_key = ProfileKey::create(key_bytes);
    let Ok(profile) = manager
        .retrieve_profile_by_uuid(uuid, profile_key)
        .await
    else {
        return;
    };
    if let Some(name) = profile.name {
        status.given_name = Some(name.given_name);
        status.family_name = name.family_name;
    }
    status.about = profile.about;
    status.about_emoji = profile.about_emoji;
}
