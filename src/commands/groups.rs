use anyhow::anyhow;
use std::path::Path;

use presage::Manager;
use presage::manager::Registered;
use presage::store::Store;

use crate::errors::SignalError;
use crate::output::{GroupInfo, GroupList};
use crate::utils::open_sqlite_store;

use super::load_registered_and_receive;

pub(crate) async fn run_list_groups(
    config_dir: &Path,
    active_account: Option<String>,
    detailed: bool,
    group_ids: Vec<String>,
) -> anyhow::Result<GroupList> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
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
        if !group_ids.is_empty() && !group_ids.contains(&hex_key) {
            continue;
        }
        groups.push(GroupInfo {
            id: hex_key,
            name: g.title,
            description: g.description,
            members_count: g.members.len(),
        });
    }
    Ok(GroupList { groups, detailed })
}

pub(crate) async fn run_join_group(
    _config_dir: &Path,
    _active_account: Option<String>,
    _uri: String,
) -> anyhow::Result<()> {
    Err(SignalError::NotImplemented(
        "joinGroup is not yet implemented (join via URI is not exposed by presage).".to_string(),
    )
    .into())
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
    Err(SignalError::NotImplemented(
        "updateGroup is not yet implemented (Group V2 admin operations are not exposed by presage)."
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
    Err(SignalError::NotImplemented("quitGroup is not yet implemented.".to_string()).into())
}
