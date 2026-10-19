use std::path::Path;

use anyhow::anyhow;
use presage::Manager;
use presage::manager::Registered;
use presage::store::Store;

use crate::errors::SignalError;
use crate::output::{IdentityInfo, SendResult, SuccessResult};
use crate::utils::open_sqlite_store;

use super::load_registered_and_receive;

pub(crate) async fn core_list_identities() -> anyhow::Result<Vec<IdentityInfo>> {
    Err(SignalError::LibraryLimitation(
        "listIdentities is not yet supported by presage (parameters ignored).".to_string(),
    )
    .into())
}

pub(crate) async fn run_list_identities(
    _config_dir: &Path,
    active_account: Option<String>,
    _recipient: Option<String>,
) -> anyhow::Result<Vec<IdentityInfo>> {
    let _account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    core_list_identities().await
}

pub(crate) async fn core_trust() -> anyhow::Result<SuccessResult> {
    Err(SignalError::LibraryLimitation(
        "Manual trust management is not yet supported by presage (parameters ignored).".to_string(),
    )
    .into())
}

pub(crate) async fn run_trust(
    _config_dir: &Path,
    active_account: Option<String>,
    _recipient: String,
    _trust_level: String,
) -> anyhow::Result<SuccessResult> {
    let _account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    core_trust().await
}

pub(crate) async fn run_remove_contact(
    config_dir: &Path,
    active_account: Option<String>,
    recipient: String,
) -> anyhow::Result<SuccessResult> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;
    core_remove_contact(&mut manager, recipient).await
}

#[tracing::instrument(skip(_manager))]
pub(crate) async fn core_remove_contact<S: Store + Send + 'static>(
    _manager: &mut Manager<S, Registered>,
    _recipient: String,
) -> anyhow::Result<SuccessResult> {
    Err(SignalError::LibraryLimitation(
        "removeContact is not yet supported by presage.".to_string(),
    )
    .into())
}

pub(crate) async fn core_send_poll_create() -> anyhow::Result<SendResult> {
    Err(SignalError::LibraryLimitation(
        "Poll creation is not yet supported by presage (parameters ignored).".to_string(),
    )
    .into())
}

pub(crate) async fn run_send_poll_create(
    _config_dir: &Path,
    active_account: Option<String>,
    _recipient: Vec<String>,
    _group_id: Option<String>,
    _question: String,
    _options: Vec<String>,
    _multiple_answers: bool,
) -> anyhow::Result<SendResult> {
    let _account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    core_send_poll_create().await
}

pub(crate) async fn core_send_poll_vote() -> anyhow::Result<SendResult> {
    Err(SignalError::LibraryLimitation(
        "Poll voting is not yet supported by presage (parameters ignored).".to_string(),
    )
    .into())
}

pub(crate) async fn run_send_poll_vote(
    _config_dir: &Path,
    active_account: Option<String>,
    _recipient: Vec<String>,
    _group_id: Option<String>,
    _target_timestamp: i64,
    _choices: Vec<i32>,
) -> anyhow::Result<SendResult> {
    let _account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    core_send_poll_vote().await
}

pub(crate) async fn core_send_poll_terminate() -> anyhow::Result<SendResult> {
    Err(SignalError::LibraryLimitation(
        "Poll termination is not yet supported by presage (parameters ignored).".to_string(),
    )
    .into())
}

pub(crate) async fn run_send_poll_terminate(
    _config_dir: &Path,
    active_account: Option<String>,
    _recipient: Vec<String>,
    _group_id: Option<String>,
    _target_timestamp: i64,
) -> anyhow::Result<SendResult> {
    let _account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    core_send_poll_terminate().await
}

pub(crate) async fn core_block() -> anyhow::Result<SuccessResult> {
    Err(SignalError::LibraryLimitation(
        "Blocking is not yet supported by presage (parameters ignored).".to_string(),
    )
    .into())
}

pub(crate) async fn run_block(
    _config_dir: &Path,
    active_account: Option<String>,
    _recipient: Vec<String>,
    _group_id: Vec<String>,
) -> anyhow::Result<SuccessResult> {
    let _account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    core_block().await
}

pub(crate) async fn core_unblock() -> anyhow::Result<SuccessResult> {
    Err(SignalError::LibraryLimitation(
        "Unblocking is not yet supported by presage (parameters ignored).".to_string(),
    )
    .into())
}

pub(crate) async fn run_unblock(
    _config_dir: &Path,
    active_account: Option<String>,
    _recipient: Vec<String>,
    _group_id: Vec<String>,
) -> anyhow::Result<SuccessResult> {
    let _account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    core_unblock().await
}

pub(crate) async fn core_subscribe_receive() -> anyhow::Result<i64> {
    Ok(0)
}

pub(crate) async fn core_unsubscribe_receive(_subscription: i64) -> anyhow::Result<SuccessResult> {
    Ok(SuccessResult {
        success: true,
        message: "Receive subscription cleared (daemon always receives in the background)."
            .to_string(),
    })
}

pub(crate) async fn core_send_contacts() -> anyhow::Result<SuccessResult> {
    Err(
        SignalError::LibraryLimitation("sendContacts is not yet supported by presage.".to_string())
            .into(),
    )
}

pub(crate) async fn run_send_contacts(
    _config_dir: &Path,
    active_account: Option<String>,
) -> anyhow::Result<SuccessResult> {
    let _account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    core_send_contacts().await
}

// run_upload_sticker_pack core in mod
pub(crate) async fn core_upload_sticker_pack() -> anyhow::Result<SuccessResult> {
    Err(SignalError::LibraryLimitation(
        "uploadStickerPack is not yet supported by presage.".to_string(),
    )
    .into())
}
