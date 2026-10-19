use anyhow::anyhow;
use std::path::Path;

use presage::Manager;
use presage::libsignal_service::prelude::phonenumber::PhoneNumber;
use presage::manager::Registered;
use presage::store::Store;
use std::str::FromStr;

use crate::output::{DeviceInfo, DeviceList, SuccessResult, UserStatus};
use crate::utils::open_sqlite_store;

use super::load_registered_and_receive;

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
                        });
                    }
                }
                Err(_) => statuses.push(UserStatus {
                    recipient: rec,
                    is_registered: false,
                    aci: None,
                }), // Or handle error
            }
        }
    }

    for user in usernames {
        match manager.lookup_username(&user).await {
            Ok(Some(service_id)) => statuses.push(UserStatus {
                recipient: user,
                is_registered: true,
                aci: Some(service_id.service_id_string()),
            }),
            Ok(None) => statuses.push(UserStatus {
                recipient: user,
                is_registered: false,
                aci: None,
            }),
            Err(_) => statuses.push(UserStatus {
                recipient: user,
                is_registered: false,
                aci: None,
            }),
        }
    }
    Ok(statuses)
}
