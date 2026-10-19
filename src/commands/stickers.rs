use anyhow::anyhow;
use base64::prelude::*;
use presage::Manager;
use presage::manager::Registered;
use presage::store::Store;
use std::path::Path;

use crate::output::{MediaData, StickerInfo, StickerPackInfo, StickerPackList, SuccessResult};
use crate::utils::open_sqlite_store;

use super::load_registered_and_receive;
use super::stubs::core_upload_sticker_pack;

pub(crate) async fn run_list_sticker_packs(
    config_dir: &Path,
    active_account: Option<String>,
) -> anyhow::Result<StickerPackList> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;
    core_list_sticker_packs(&mut manager).await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_list_sticker_packs<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
) -> anyhow::Result<StickerPackList> {
    let mut packs = Vec::new();
    for pack in manager.store().sticker_packs().await?.flatten() {
        packs.push(StickerPackInfo {
            id: hex::encode(pack.id),
            key: hex::encode(pack.key),
            title: pack.manifest.title,
            author: pack.manifest.author,
            stickers: pack
                .manifest
                .stickers
                .into_iter()
                .map(|s| StickerInfo {
                    id: s.id,
                    emoji: s.emoji,
                    content_type: s.content_type,
                    data: s.bytes.map(|b| BASE64_STANDARD.encode(b)),
                })
                .collect(),
        });
    }
    Ok(StickerPackList { packs })
}

pub(crate) async fn run_add_sticker_pack(
    config_dir: &Path,
    active_account: Option<String>,
    pack_id: String,
    pack_key: String,
) -> anyhow::Result<SuccessResult> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;
    core_add_sticker_pack(&mut manager, pack_id, pack_key).await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_add_sticker_pack<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    pack_id_str: String,
    pack_key_str: String,
) -> anyhow::Result<SuccessResult> {
    let pack_id = hex::decode(pack_id_str.trim_start_matches("0x"))?;
    let pack_key = hex::decode(pack_key_str.trim_start_matches("0x"))?;

    let pack_id: [u8; 16] = pack_id
        .try_into()
        .map_err(|_| anyhow!("Invalid pack ID length (must be 16 bytes)"))?;
    let pack_key: [u8; 32] = pack_key
        .try_into()
        .map_err(|_| anyhow!("Invalid pack key length (must be 32 bytes)"))?;

    manager.install_sticker_pack(&pack_id, &pack_key).await?;

    Ok(SuccessResult {
        success: true,
        message: "Sticker pack installed successfully.".to_string(),
    })
}

pub(crate) async fn run_get_sticker(
    config_dir: &Path,
    active_account: Option<String>,
    pack_id: String,
    sticker_id: u32,
) -> anyhow::Result<MediaData> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;
    core_get_sticker(&mut manager, pack_id, sticker_id).await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_get_sticker<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    pack_id_str: String,
    sticker_id: u32,
) -> anyhow::Result<MediaData> {
    let pack_id = hex::decode(pack_id_str.trim_start_matches("0x"))?;
    let pack_id_bytes: [u8; 16] = pack_id
        .try_into()
        .map_err(|_| anyhow!("Invalid pack ID length"))?;

    let sticker = manager
        .sticker_metadata(&pack_id_bytes, sticker_id)
        .await?
        .ok_or_else(|| anyhow!("Sticker not found."))?;

    if let Some(bytes) = sticker.bytes {
        Ok(MediaData {
            data: BASE64_STANDARD.encode(bytes),
        })
    } else {
        anyhow::bail!("Sticker data not available.");
    }
}

pub(crate) async fn run_upload_sticker_pack(
    _config_dir: &Path,
    active_account: Option<String>,
    _manifest: String,
) -> anyhow::Result<SuccessResult> {
    let _account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    core_upload_sticker_pack().await
}
