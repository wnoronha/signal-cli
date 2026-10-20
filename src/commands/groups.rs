use std::path::Path;

use presage::Manager;
use presage::libsignal_service::groups_v2::Role;
use presage::libsignal_service::protocol::ServiceId;
use presage::manager::Registered;
use presage::store::Store;

use crate::errors::SignalError;
use crate::output::{GroupInfo, GroupList, SendResult};
use crate::utils::open_sqlite_store;

use super::group_id::encode_group_id_b64;
use super::load_registered_and_receive;

pub(crate) async fn run_list_groups(
    config_dir: &Path,
    active_account: Option<String>,
    detailed: bool,
    group_ids: Vec<String>,
) -> anyhow::Result<GroupList> {
    let account = crate::utils::require_account(active_account)?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;
    core_list_groups(&mut manager, detailed, group_ids).await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_list_groups<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    detailed: bool,
    group_ids: Vec<String>,
) -> anyhow::Result<GroupList> {
    let mut groups = Vec::new();
    for (key, g) in manager.store().groups().await?.flatten() {
        let hex_key = hex::encode(key);
        let b64_key = encode_group_id_b64(&key);
        if !group_ids.is_empty() && !group_ids.contains(&hex_key) && !group_ids.contains(&b64_key) {
            continue;
        }

        let info = if detailed {
            let mut members = Vec::with_capacity(g.members.len());
            let mut admins = Vec::new();
            for m in &g.members {
                let id = ServiceId::from(m.aci).service_id_string();
                if m.role == Role::Administrator {
                    admins.push(id.clone());
                }
                members.push(id);
            }
            let pending_members: Vec<String> = g
                .pending_members
                .iter()
                .map(|p| p.uuid.to_string())
                .collect();
            let requesting_members: Vec<String> = g
                .requesting_members
                .iter()
                .map(|r| ServiceId::from(r.aci).service_id_string())
                .collect();

            GroupInfo {
                id: b64_key,
                name: g.title,
                description: g.description,
                members_count: None,
                is_member: Some(true),
                is_blocked: Some(false),
                members: Some(members),
                pending_members: Some(pending_members),
                requesting_members: Some(requesting_members),
                admins: Some(admins),
                group_invite_link: None,
            }
        } else {
            GroupInfo {
                id: b64_key,
                name: g.title,
                description: g.description,
                members_count: Some(g.members.len()),
                is_member: None,
                is_blocked: None,
                members: None,
                pending_members: None,
                requesting_members: None,
                admins: None,
                group_invite_link: None,
            }
        };

        groups.push(info);
    }
    Ok(GroupList { groups, detailed })
}

pub(crate) async fn core_join_group() -> anyhow::Result<()> {
    Err(SignalError::LibraryLimitation(
        "joinGroup is not yet supported by presage (join via URI is not exposed).".to_string(),
    )
    .into())
}

pub(crate) async fn run_join_group(
    _config_dir: &Path,
    _active_account: Option<String>,
    _uri: String,
) -> anyhow::Result<()> {
    core_join_group().await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_update_group(
    _config_dir: &Path,
    _active_account: Option<String>,
    _group_id: Option<String>,
    _name: Option<String>,
    _description: Option<String>,
    _avatar: Option<String>,
    _member: Vec<String>,
    _remove_member: Vec<String>,
    _admin: Vec<String>,
    _remove_admin: Vec<String>,
    _ban: Vec<String>,
    _unban: Vec<String>,
    _reset_link: bool,
    _link: Option<String>,
    _set_permission_add_member: Option<String>,
    _set_permission_edit_details: Option<String>,
    _set_permission_send_messages: Option<String>,
    _expiration: Option<i64>,
    _member_label_emoji: Option<String>,
    _member_label: Option<String>,
) -> anyhow::Result<()> {
    core_update_group().await.map(|_| ())
}

pub(crate) async fn core_update_group() -> anyhow::Result<SendResult> {
    Err(SignalError::LibraryLimitation(
        "updateGroup is not yet supported by presage (Group V2 admin operations are not exposed)."
            .to_string(),
    )
    .into())
}

pub(crate) async fn run_quit_group(
    _config_dir: &Path,
    _active_account: Option<String>,
    _group_id: String,
    _delete: bool,
) -> anyhow::Result<()> {
    core_quit_group().await.map(|_| ())
}

pub(crate) async fn core_quit_group() -> anyhow::Result<crate::output::SuccessResult> {
    Err(SignalError::LibraryLimitation(
        "quitGroup is not yet supported by presage (leave group is not exposed).".to_string(),
    )
    .into())
}
