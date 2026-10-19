use anyhow::{Context, anyhow};
use base64::prelude::*;
use futures::{StreamExt, channel::oneshot, pin_mut};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::UNIX_EPOCH;

use crate::errors::SignalError;
use crate::output::render_command_error;
use crate::output::{
    AccountList, AttachmentEnvelope, ContactInfo, ContactList, DataMessageEnvelope, DeviceInfo,
    DeviceList, GroupInfo, GroupList, MediaData, ProfileInfo, ReceiveEnvelope, Render, SendResult,
    StickerInfo, StickerPackInfo, StickerPackList, SuccessResult, UserStatus,
};
use crate::utils::{
    find_local_accounts, open_or_create_sqlite_store, open_sqlite_store, parse_sticker_pack_uri,
    push_store_identity, resolve_recipient,
};
use presage::Manager;
use presage::libsignal_service::configuration::SignalServers;
use presage::libsignal_service::content::{ContentBody, DataMessage, GroupContextV2};
use presage::libsignal_service::prelude::Uuid;
use presage::libsignal_service::prelude::phonenumber::PhoneNumber;
use presage::libsignal_service::proto::attachment_pointer::AttachmentIdentifier;
use presage::libsignal_service::proto::{
    ReceiptMessage, TypingMessage, data_message, receipt_message, typing_message,
};
use presage::libsignal_service::protocol::ServiceId;
use presage::libsignal_service::zkgroup::profiles::ProfileKey;
use presage::manager::{Registered, RegistrationOptions};
use presage::model::contacts::Contact;
use presage::model::messages::Received;
use presage::store::{Store, Thread};

mod group_id;
mod runtime;
mod stubs;

use group_id::parse_group_id;
use runtime::load_registered_and_receive;

pub(crate) use stubs::*;

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

pub async fn execute_command(
    args: crate::cli::Cli,
    config_dir: PathBuf,
    active_account: Option<String>,
) -> anyhow::Result<()> {
    let output = args.output;
    crate::compat::warn_unimplemented_cli_flags(&args);
    if args.trust_new_identities == crate::cli::TrustMode::Always {
        eprintln!(
            "warning: --trust-new-identities always is mapped to presage Trust (same as on-first-use in this port)"
        );
    }
    let _store_identity = push_store_identity(args.trust_new_identities);
    let result = match args.command {
        crate::cli::Command::Version => {
            println!(
                "signal-cli {} (Rust lightweight drop-in replacement)",
                env!("CARGO_PKG_VERSION")
            );
            Ok(())
        }

        crate::cli::Command::ListAccounts => {
            run_list_accounts(&config_dir).await?.render(args.output);
            Ok(())
        }

        crate::cli::Command::Register {
            voice,
            captcha,
            reregister,
        } => {
            run_register(
                &config_dir,
                active_account,
                voice,
                captcha,
                reregister,
                args.service_environment,
            )
            .await
        }

        crate::cli::Command::Verify {
            verification_code: _,
            pin: _,
        } => {
            println!(
                "The verify subcommand is not used in this Rust port: run `register` and enter the SMS/voice code when prompted."
            );
            Ok(())
        }

        crate::cli::Command::Link { name } => {
            run_link(&config_dir, active_account, name, args.service_environment).await
        }

        crate::cli::Command::AddDevice { uri } => {
            run_add_device(&config_dir, active_account, uri).await
        }

        crate::cli::Command::ListDevices => {
            run_list_devices(&config_dir, active_account)
                .await?
                .render(args.output);
            Ok(())
        }

        crate::cli::Command::RemoveDevice { device_id } => {
            run_remove_device(&config_dir, active_account, device_id)
                .await?
                .render(args.output);
            Ok(())
        }

        crate::cli::Command::GetUserStatus {
            recipient,
            username,
        } => {
            run_get_user_status(&config_dir, active_account, recipient, username)
                .await?
                .render(args.output);
            Ok(())
        }

        crate::cli::Command::Send {
            recipient,
            note_to_self,
            notify_self: _,
            group_id,
            username: _,
            message,
            message_from_stdin,
            attachment,
            view_once: _,
            sticker: _,
            mention: _,
            text_style: _,
            quote_timestamp: _,
            quote_author: _,
            quote_message: _,
            quote_mention: _,
            quote_text_style: _,
            quote_attachment: _,
            preview_url: _,
            preview_title: _,
            preview_description: _,
            preview_image: _,
            story_timestamp: _,
            story_author: _,
            end_session: _,
            edit_timestamp: _,
            no_urgent: _,
        } => {
            run_send(
                &config_dir,
                active_account,
                note_to_self,
                recipient,
                group_id,
                message,
                message_from_stdin,
                attachment,
            )
            .await?
            .render(args.output);
            Ok(())
        }

        crate::cli::Command::Receive {
            timeout: _,
            max_messages,
            ignore_attachments: _,
            ignore_stories: _,
            ignore_avatars: _,
            ignore_stickers: _,
            send_read_receipts: _,
        } => {
            run_receive(
                &config_dir,
                active_account,
                max_messages.map(|m| m as usize),
            )
            .await?
            .render(args.output);
            Ok(())
        }

        crate::cli::Command::ListGroups { detailed, group_id } => {
            run_list_groups(&config_dir, active_account, detailed, group_id)
                .await?
                .render(args.output);
            Ok(())
        }

        crate::cli::Command::ListContacts {
            recipient: _,
            all_recipients: _,
            blocked: _,
            name,
            detailed,
            internal: _,
        } => {
            run_list_contacts(&config_dir, active_account, name, detailed)
                .await?
                .render(args.output);
            Ok(())
        }

        crate::cli::Command::GetAttachment {
            id,
            recipient,
            group_id,
        } => {
            run_get_attachment(&config_dir, active_account, id, recipient, group_id)
                .await?
                .render(args.output);
            Ok(())
        }

        crate::cli::Command::GetAvatar {
            contact,
            profile,
            group_id,
        } => {
            run_get_avatar(&config_dir, active_account, contact, profile, group_id)
                .await?
                .render(args.output);
            Ok(())
        }

        crate::cli::Command::UpdateProfile {
            given_name,
            family_name,
            about,
            about_emoji,
            avatar: _,
            remove_avatar: _,
            mobile_coin_address: _,
        } => {
            run_update_profile(
                &config_dir,
                active_account,
                given_name,
                family_name,
                about,
                about_emoji,
            )
            .await?
            .render(args.output);
            Ok(())
        }

        crate::cli::Command::SendReaction {
            recipient,
            group_id,
            username: _,
            note_to_self,
            notify_self: _,
            emoji,
            target_author,
            target_timestamp,
            remove,
            story: _,
        } => {
            run_send_reaction(
                &config_dir,
                active_account,
                recipient,
                group_id,
                note_to_self,
                emoji,
                target_author,
                target_timestamp,
                remove,
            )
            .await?
            .render(args.output);
            Ok(())
        }

        crate::cli::Command::SendReceipt {
            recipient,
            username: _,
            target_timestamp,
            r#type,
        } => {
            run_send_receipt(
                &config_dir,
                active_account,
                recipient,
                target_timestamp,
                r#type,
            )
            .await?
            .render(args.output);
            Ok(())
        }

        crate::cli::Command::SendTyping {
            recipient,
            group_id,
            stop,
        } => {
            run_send_typing(&config_dir, active_account, recipient, group_id, stop)
                .await?
                .render(args.output);
            Ok(())
        }

        crate::cli::Command::JoinGroup { uri } => {
            run_join_group(&config_dir, active_account, uri).await
        }

        crate::cli::Command::UpdateGroup {
            group_id,
            name,
            description,
            avatar,
            member,
            remove_member,
            admin,
            remove_admin,
            ban,
            unban,
            reset_link,
            link,
            set_permission_add_member,
            set_permission_edit_details,
            set_permission_send_messages,
            expiration,
            member_label_emoji,
            member_label,
        } => {
            run_update_group(
                &config_dir,
                active_account,
                group_id,
                name,
                description,
                avatar,
                member,
                remove_member,
                admin,
                remove_admin,
                ban,
                unban,
                reset_link,
                link,
                set_permission_add_member,
                set_permission_edit_details,
                set_permission_send_messages,
                expiration,
                member_label_emoji,
                member_label,
            )
            .await
        }

        crate::cli::Command::QuitGroup { group_id, delete } => {
            run_quit_group(&config_dir, active_account, group_id, delete).await
        }

        crate::cli::Command::RemoteDelete {
            recipient,
            group_id,
            username,
            target_timestamp,
        } => {
            run_remote_delete(
                &config_dir,
                active_account,
                recipient,
                group_id,
                username,
                target_timestamp,
            )
            .await?
            .render(args.output);
            Ok(())
        }

        crate::cli::Command::SendPollCreate {
            recipient,
            group_id,
            question,
            option,
            no_multi,
            ..
        } => {
            run_send_poll_create(
                &config_dir,
                active_account,
                recipient,
                group_id,
                question,
                option,
                !no_multi,
            )
            .await?
            .render(args.output);
            Ok(())
        }

        crate::cli::Command::SendPollVote {
            recipient,
            group_id,
            poll_timestamp,
            option,
            ..
        } => {
            run_send_poll_vote(
                &config_dir,
                active_account,
                recipient,
                group_id,
                poll_timestamp,
                option.into_iter().map(|o| o as i32).collect(),
            )
            .await?
            .render(args.output);
            Ok(())
        }

        crate::cli::Command::SendPollTerminate {
            recipient,
            group_id,
            poll_timestamp,
            ..
        } => {
            run_send_poll_terminate(
                &config_dir,
                active_account,
                recipient,
                group_id,
                poll_timestamp,
            )
            .await?
            .render(args.output);
            Ok(())
        }
        crate::cli::Command::SendMessageRequestResponse { .. }
        | crate::cli::Command::SendPaymentNotification { .. }
        | crate::cli::Command::SendPinMessage { .. }
        | crate::cli::Command::SendUnpinMessage { .. } => not_implemented(args.command.name()),

        crate::cli::Command::Unregister { delete_account } => {
            run_unregister(&config_dir, active_account, delete_account).await
        }

        crate::cli::Command::DeleteLocalAccountData { ignore_registered } => {
            run_delete_local_account_data(&config_dir, active_account, ignore_registered).await
        }

        crate::cli::Command::SetPin {
            registration_lock_pin,
        } => run_set_pin(&config_dir, active_account, registration_lock_pin).await,

        crate::cli::Command::RemovePin => run_remove_pin(&config_dir, active_account).await,

        crate::cli::Command::UpdateAccount { .. }
        | crate::cli::Command::StartChangeNumber { .. }
        | crate::cli::Command::FinishChangeNumber { .. }
        | crate::cli::Command::UpdateConfiguration { .. } => {
            Err(SignalError::NotImplemented(
                "Account settings and migration commands are not yet supported by the underlying presage library.".to_string(),
            )
            .into())
        }

        crate::cli::Command::UpdateContact {
            recipient,
            given_name,
            family_name,
            nick_given_name,
            nick_family_name,
            note,
            expiration,
            phone_number,
        } => {
            run_update_contact(
                &config_dir,
                active_account,
                recipient,
                given_name,
                family_name,
                nick_given_name,
                nick_family_name,
                note,
                expiration,
                phone_number,
            )
            .await?
            .render(args.output);
            Ok(())
        }

        crate::cli::Command::ListIdentities { number } => {
            run_list_identities(&config_dir, active_account, number)
                .await?
                .render(args.output);
            Ok(())
        }

        crate::cli::Command::Trust { recipient, .. } => {
            run_trust(
                &config_dir,
                active_account,
                recipient,
                "trusted".to_string(),
            )
            .await?
            .render(args.output);
            Ok(())
        }

        crate::cli::Command::RemoveContact { recipient, .. } => {
            run_remove_contact(&config_dir, active_account, recipient)
                .await?
                .render(args.output);
            Ok(())
        }

        crate::cli::Command::Block {
            recipient,
            group_id,
        } => {
            run_block(&config_dir, active_account, recipient, group_id)
                .await?
                .render(args.output);
            Ok(())
        }

        crate::cli::Command::Unblock {
            recipient,
            group_id,
        } => {
            run_unblock(&config_dir, active_account, recipient, group_id)
                .await?
                .render(args.output);
            Ok(())
        }

        crate::cli::Command::SendContacts => {
            run_send_contacts(&config_dir, active_account)
                .await?
                .render(args.output);
            Ok(())
        }

        crate::cli::Command::SendSyncRequest => {
            run_send_sync_request(&config_dir, active_account)
                .await?
                .render(args.output);
            Ok(())
        }

        crate::cli::Command::UploadStickerPack { path } => {
            run_upload_sticker_pack(&config_dir, active_account, path)
                .await?
                .render(args.output);
            Ok(())
        }

        crate::cli::Command::ListStickerPacks => {
            run_list_sticker_packs(&config_dir, active_account)
                .await?
                .render(args.output);
            Ok(())
        }

        crate::cli::Command::AddStickerPack { uri } => {
            let (pack_id, pack_key) = parse_sticker_pack_uri(&uri)?;
            run_add_sticker_pack(&config_dir, active_account, pack_id, pack_key)
                .await?
                .render(args.output);
            Ok(())
        }

        crate::cli::Command::GetSticker {
            pack_id,
            sticker_id,
        } => {
            run_get_sticker(&config_dir, active_account, pack_id, sticker_id as u32)
                .await?
                .render(args.output);
            Ok(())
        }

        crate::cli::Command::Daemon { .. } | crate::cli::Command::JsonRpc { .. } => {
            crate::rpc::server::run_json_rpc(config_dir, active_account, args.command).await
        }

        crate::cli::Command::SubmitRateLimitChallenge { .. }
        | crate::cli::Command::SendAdminDelete { .. }
        | crate::cli::Command::UpdateDevice { .. } => not_implemented(args.command.name()),
    };
    if let Err(err) = &result {
        render_command_error(err, output);
        return Err(anyhow::Error::new(crate::output::ErrorReported));
    }
    result
}

fn not_implemented(name: &str) -> anyhow::Result<()> {
    Err(SignalError::NotImplemented(name.to_string()).into())
}

pub(crate) async fn run_list_accounts(config_dir: &Path) -> anyhow::Result<AccountList> {
    let accounts = find_local_accounts(config_dir)?;
    Ok(AccountList { accounts })
}

pub(crate) async fn run_register(
    config_dir: &Path,
    active_account: Option<String>,
    voice: bool,
    captcha: Option<String>,
    reregister: bool,
    service_env: crate::cli::ServiceEnvironment,
) -> anyhow::Result<()> {
    let account = active_account.ok_or_else(|| {
        anyhow!("Account phone number (-a / --account) must be specified for registration.")
    })?;
    let servers = match service_env {
        crate::cli::ServiceEnvironment::Live => SignalServers::Production,
        crate::cli::ServiceEnvironment::Staging => SignalServers::Staging,
    };

    let store = open_or_create_sqlite_store(config_dir, &account)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    let phone_number = PhoneNumber::from_str(&account)
        .map_err(|e| anyhow!("Invalid E.164 phone number: {:?}", e))?;

    println!("Registering account {}...", account);
    let manager = Manager::register(
        store,
        RegistrationOptions {
            signal_servers: servers,
            phone_number,
            use_voice_call: voice,
            captcha: captcha.as_deref(),
            force: reregister,
        },
    )
    .await?;

    println!("Registration successful! An SMS/voice code has been requested.");
    println!("Please enter the verification code received (followed by enter):");

    let mut code = String::new();
    std::io::stdin().read_line(&mut code)?;
    let code = code.trim();

    if code.is_empty() {
        anyhow::bail!("Verification code cannot be empty.");
    }

    println!("Verifying registration with code {}...", code);
    let registered_manager = manager.confirm_verification_code(code).await?;
    println!(
        "Successfully verified and registered account: {}",
        registered_manager.registration_data().phone_number
    );
    Ok(())
}

pub(crate) async fn run_link(
    config_dir: &Path,
    active_account: Option<String>,
    name: Option<String>,
    service_env: crate::cli::ServiceEnvironment,
) -> anyhow::Result<()> {
    let servers = match service_env {
        crate::cli::ServiceEnvironment::Live => SignalServers::Production,
        crate::cli::ServiceEnvironment::Staging => SignalServers::Staging,
    };
    let device_name = name.unwrap_or_else(|| "signal-cli-rust".to_string());

    // Use active_account if provided, otherwise use a temporary file
    let db_name = active_account
        .clone()
        .unwrap_or_else(|| "link_temp".to_string());
    let db_path = config_dir.join(format!("{}.db", db_name));

    if db_path.exists() {
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(config_dir.join(format!("{}.db-wal", db_name)));
        let _ = std::fs::remove_file(config_dir.join(format!("{}.db-shm", db_name)));
    }

    let store = open_or_create_sqlite_store(config_dir, &db_name)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    let (provisioning_link_tx, provisioning_link_rx) = oneshot::channel();
    println!("Linking secondary device...");
    let manager_fut =
        Manager::link_secondary_device(store, servers, device_name, provisioning_link_tx);

    let link_rx_fut = async move {
        match provisioning_link_rx.await {
            Ok(url) => {
                use qrcode::QrCode;
                use qrcode::render::unicode;
                println!("Please scan the QR code or use this URL to link your device:");
                match QrCode::new(url.as_str()) {
                    Ok(code) => {
                        println!(
                            "{}",
                            code.render::<unicode::Dense1x2>()
                                .quiet_zone(false)
                                .module_dimensions(1, 1)
                                .build()
                        );
                    }
                    Err(e) => eprintln!("Failed to generate QR code: {:?}", e),
                }
                println!("{}", url);
            }
            Err(error) => {
                eprintln!("Linking was cancelled: {:?}", error);
            }
        }
    };

    let (manager_res, _) = futures::future::join(manager_fut, link_rx_fut).await;
    let manager = manager_res?;
    let phone_number = manager.registration_data().phone_number.to_string();
    println!("Successfully linked device for account: {}", phone_number);

    // Drop manager to release file locks and ensure flush
    drop(manager);

    // If we used a temporary name or a name different from the actual phone number, rename it now
    if db_name != phone_number {
        let final_db_path = config_dir.join(format!("{}.db", phone_number));
        if final_db_path.exists() {
            let _ = std::fs::remove_file(&final_db_path);
            let _ = std::fs::remove_file(config_dir.join(format!("{}.db-wal", phone_number)));
            let _ = std::fs::remove_file(config_dir.join(format!("{}.db-shm", phone_number)));
        }

        // Rename main file
        std::fs::rename(&db_path, &final_db_path)?;

        // Also rename WAL and SHM files if they exist
        let wal_path = config_dir.join(format!("{}.db-wal", db_name));
        if wal_path.exists() {
            std::fs::rename(
                &wal_path,
                config_dir.join(format!("{}.db-wal", phone_number)),
            )?;
        }
        let shm_path = config_dir.join(format!("{}.db-shm", db_name));
        if shm_path.exists() {
            std::fs::rename(
                &shm_path,
                config_dir.join(format!("{}.db-shm", phone_number)),
            )?;
        }

        println!("Account database saved to: {:?}", final_db_path);
    } else {
        println!("Account database saved to: {:?}", db_path);
    }

    Ok(())
}

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

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_send(
    config_dir: &Path,
    active_account: Option<String>,
    note_to_self: bool,
    recipient: Vec<String>,
    group_id: Option<String>,
    message: Option<String>,
    message_from_stdin: bool,
    attachment: Vec<String>,
) -> anyhow::Result<SendResult> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;

    let mut body_text = message;
    if message_from_stdin {
        let mut buffer = String::new();
        std::io::stdin().read_to_string(&mut buffer)?;
        body_text = Some(buffer.trim().to_string());
    }

    core_send(
        &mut manager,
        note_to_self,
        recipient,
        group_id,
        body_text,
        attachment,
    )
    .await
}

#[tracing::instrument(skip(manager, body_text))]
pub(crate) async fn core_send<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    note_to_self: bool,
    recipient: Vec<String>,
    group_id: Option<String>,
    body_text: Option<String>,
    attachment: Vec<String>,
) -> anyhow::Result<SendResult> {
    use tracing::warn;
    let attachments = upload_attachments(attachment, manager).await?;

    let timestamp = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as u64;

    if let Some(ref gid) = group_id {
        let master_key = parse_group_id(gid)?;
        let data_message = DataMessage {
            body: body_text,
            attachments,
            group_v2: Some(GroupContextV2 {
                master_key: Some(master_key.to_vec()),
                revision: Some(0),
                ..Default::default()
            }),
            ..Default::default()
        };
        let content_body = ContentBody::DataMessage(data_message);
        manager
            .send_message_to_group(&master_key.clone(), content_body, timestamp)
            .await?;
    } else {
        let mut resolved_recipients = Vec::new();
        if note_to_self {
            resolved_recipients.push(ServiceId::Aci(
                manager.registration_data().service_ids.aci.into(),
            ));
        }
        for rec in recipient {
            match resolve_recipient(manager, &rec).await {
                Ok(resolved) => resolved_recipients.push(resolved),
                Err(e) => warn!("Error resolving recipient {}: {:?}", rec, e),
            }
        }

        if resolved_recipients.is_empty() {
            anyhow::bail!("No valid recipients specified.");
        }

        let data_message = DataMessage {
            body: body_text,
            attachments,
            ..Default::default()
        };
        let content_body = ContentBody::DataMessage(data_message);

        for r in resolved_recipients {
            manager
                .send_message(r, content_body.clone(), timestamp)
                .await?;
        }
    }
    Ok(SendResult { timestamp })
}

pub(crate) async fn run_receive(
    config_dir: &Path,
    active_account: Option<String>,
    max_messages: Option<usize>,
) -> anyhow::Result<Vec<ReceiveEnvelope>> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = Manager::load_registered(store).await?;

    core_receive(&mut manager, max_messages).await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_receive<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    max_messages: Option<usize>,
) -> anyhow::Result<Vec<ReceiveEnvelope>> {
    use tracing::{debug, info};
    info!("Querying server for new messages...");
    let messages = manager
        .receive_messages()
        .await
        .context("failed to receive messages")?;
    pin_mut!(messages);

    let mut count = 0;
    let mut envelopes = Vec::new();
    while let Some(content) = messages.next().await {
        match content {
            Received::QueueEmpty => break,
            Received::Contacts => {}
            Received::DecryptionError(sid) => {
                debug!("Decryption error from {}", sid.service_id_string());
            }
            Received::Content(content) => {
                debug!(
                    "Processing received message from {}",
                    content.metadata.sender.service_id_string()
                );
                let source_uuid = content.metadata.sender.service_id_string();
                let timestamp = content.metadata.server_timestamp.timestamp_millis() as u64;

                let data_message = if let ContentBody::DataMessage(dm) = content.body {
                    Some(DataMessageEnvelope {
                        message: dm.body,
                        attachments: Some(
                            dm.attachments
                                .into_iter()
                                .map(|a| AttachmentEnvelope {
                                    id: a
                                        .attachment_identifier
                                        .as_ref()
                                        .map(crate::utils::attachment_id_str)
                                        .unwrap_or_default(),
                                    content_type: a.content_type.unwrap_or_default(),
                                    filename: a.file_name,
                                    size: a.size.map(|s| s as u64),
                                    width: a.width,
                                    height: a.height,
                                    caption: a.caption,
                                    upload_timestamp: a.upload_timestamp,
                                })
                                .collect(),
                        ),
                        sticker: None,
                        reaction: None,
                        expires_in_seconds: dm.expire_timer.unwrap_or_default(),
                        is_expiration_update: false,
                        view_once: dm.is_view_once.unwrap_or_default(),
                    })
                } else {
                    None
                };

                envelopes.push(ReceiveEnvelope {
                    source: None,
                    source_uuid: Some(source_uuid),
                    source_name: None,
                    source_device: Some(content.metadata.sender_device.into()),
                    timestamp,
                    server_received_timestamp: None,
                    server_delivered_timestamp: None,
                    data_message,
                    typing_message: None,
                    call_message: None,
                });

                count += 1;
                if let Some(max) = max_messages
                    && count >= max
                {
                    break;
                }
            }
        }
    }
    Ok(envelopes)
}

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

pub(crate) async fn run_list_contacts(
    config_dir: &Path,
    active_account: Option<String>,
    name: Option<String>,
    detailed: bool,
) -> anyhow::Result<ContactList> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
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
        contacts.push(ContactInfo {
            uuid: contact.uuid.to_string(),
            phone: contact.phone_number.map(|p| p.to_string()),
            name: contact.name,
        });
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
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
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

async fn upload_attachments<S: Store>(
    attachment_filepath: Vec<String>,
    manager: &Manager<S, Registered>,
) -> anyhow::Result<Vec<presage::proto::AttachmentPointer>> {
    use mime_guess::mime::APPLICATION_OCTET_STREAM;
    use presage::libsignal_service::sender::AttachmentSpec;

    let mut attachment_specs = Vec::new();
    for path_or_uri in attachment_filepath {
        if let Some(data_uri) = path_or_uri.strip_prefix("data:") {
            // Handle Data URI: data:[<mediatype>][;base64],<data>
            let (metadata, base64_data) = data_uri
                .split_once(',')
                .ok_or_else(|| anyhow!("Invalid data URI format"))?;

            let mut parts = metadata.split(';');
            let content_type = parts
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("application/octet-stream");

            let mut is_base64 = false;
            let mut file_name = None;
            for part in parts {
                if part == "base64" {
                    is_base64 = true;
                } else if let Some(name) = part.strip_prefix("filename=") {
                    file_name = Some(name.to_string());
                }
            }

            if !is_base64 {
                anyhow::bail!("Only base64 encoded data URIs are supported.");
            }

            let data = BASE64_STANDARD
                .decode(base64_data)
                .map_err(|e| anyhow!("Failed to decode base64 in data URI: {:?}", e))?;

            attachment_specs.push((
                AttachmentSpec {
                    content_type: content_type.to_string(),
                    length: data.len(),
                    file_name,
                    preview: None,
                    voice_note: None,
                    borderless: None,
                    width: None,
                    height: None,
                    caption: None,
                    blur_hash: None,
                },
                data,
            ));
        } else {
            // Handle local file path
            let path = PathBuf::from(path_or_uri);
            let data = std::fs::read(&path)?;
            attachment_specs.push((
                AttachmentSpec {
                    content_type: mime_guess::from_path(&path)
                        .first()
                        .unwrap_or(APPLICATION_OCTET_STREAM)
                        .to_string(),
                    length: data.len(),
                    file_name: path.file_name().map(|s| s.to_string_lossy().to_string()),
                    preview: None,
                    voice_note: None,
                    borderless: None,
                    width: None,
                    height: None,
                    caption: None,
                    blur_hash: None,
                },
                data,
            ));
        }
    }

    if attachment_specs.is_empty() {
        return Ok(Vec::new());
    }

    let attachments: Result<Vec<_>, _> = manager
        .upload_attachments(attachment_specs)
        .await?
        .into_iter()
        .collect();

    Ok(attachments?)
}

pub(crate) async fn run_delete_local_account_data(
    config_dir: &std::path::Path,
    active_account: Option<String>,
    ignore_registered: bool,
) -> anyhow::Result<()> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let db_path = config_dir.join(format!("{}.db", account));

    if !db_path.exists() {
        println!("No local data found for account: {}", account);
        return Ok(());
    }

    if !ignore_registered {
        println!(
            "Warning: Deleting local data for a registered account. This will NOT unregister the account from the Signal server."
        );
        println!(
            "If you want to unregister, use the 'unregister' command (currently not implemented)."
        );
    }

    std::fs::remove_file(&db_path)?;
    println!("Deleted local data for account: {}", account);
    Ok(())
}

pub(crate) async fn run_unregister(
    _config_dir: &std::path::Path,
    _active_account: Option<String>,
    _delete_account: bool,
) -> anyhow::Result<()> {
    Err(SignalError::NotImplemented(
        "unregister is not yet implemented (server-side unregister is not exposed by presage)."
            .to_string(),
    )
    .into())
}

pub(crate) async fn run_set_pin(
    _config_dir: &std::path::Path,
    _active_account: Option<String>,
    _pin: String,
) -> anyhow::Result<()> {
    Err(SignalError::NotImplemented("setPin is not yet implemented.".to_string()).into())
}

pub(crate) async fn run_remove_pin(
    _config_dir: &std::path::Path,
    _active_account: Option<String>,
) -> anyhow::Result<()> {
    Err(SignalError::NotImplemented("removePin is not yet implemented.".to_string()).into())
}

pub(crate) async fn run_get_avatar(
    config_dir: &Path,
    active_account: Option<String>,
    contact: Option<String>,
    profile: Option<String>,
    group_id: Option<String>,
) -> anyhow::Result<MediaData> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
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
            if let Some((_, Some(id))) = discovered.into_iter().next() {
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
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
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

    for content_res in messages {
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

    let pointer = attachment_pointer
        .ok_or_else(|| anyhow!("Attachment with ID {} not found in thread.", id))?;
    let bytes = manager.get_attachment(&pointer).await?;

    Ok(MediaData {
        data: BASE64_STANDARD.encode(bytes),
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_send_reaction(
    config_dir: &Path,
    active_account: Option<String>,
    recipient: Vec<String>,
    group_id: Option<String>,
    note_to_self: bool,
    emoji: String,
    target_author: String,
    target_timestamp: i64,
    remove: bool,
) -> anyhow::Result<SuccessResult> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;

    core_send_reaction(
        &mut manager,
        recipient,
        group_id,
        note_to_self,
        emoji,
        target_author,
        target_timestamp,
        remove,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip(manager))]
pub(crate) async fn core_send_reaction<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    recipient: Vec<String>,
    group_id: Option<String>,
    note_to_self: bool,
    emoji: String,
    target_author: String,
    target_timestamp: i64,
    remove: bool,
) -> anyhow::Result<SuccessResult> {
    let target_author_sid = resolve_recipient(manager, &target_author).await?;
    let target_author_aci = match target_author_sid {
        ServiceId::Aci(u) => Uuid::from(u),
        ServiceId::Pni(u) => Uuid::from(u),
    };

    let reaction = data_message::Reaction {
        emoji: Some(emoji),
        remove: Some(remove),
        target_author_aci: Some(target_author_aci.to_string()),
        target_sent_timestamp: Some(target_timestamp as u64),
        target_author_aci_binary: Some(target_author_aci.as_bytes().to_vec()),
    };

    let data_message = DataMessage {
        reaction: Some(reaction),
        ..Default::default()
    };
    let content_body = ContentBody::DataMessage(data_message);

    let timestamp = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis() as u64;

    if let Some(gid) = group_id {
        let master_key = parse_group_id(&gid)?;
        manager
            .send_message_to_group(&master_key, content_body, timestamp)
            .await?;
    } else {
        let mut recipients = Vec::new();
        if note_to_self {
            recipients.push(ServiceId::Aci(
                manager.registration_data().service_ids.aci.into(),
            ));
        }
        for rec in recipient {
            recipients.push(resolve_recipient(manager, &rec).await?);
        }
        for r in recipients {
            manager
                .send_message(r, content_body.clone(), timestamp)
                .await?;
        }
    }

    Ok(SuccessResult {
        success: true,
        message: "Reaction sent successfully.".to_string(),
    })
}

pub(crate) async fn run_send_receipt(
    config_dir: &Path,
    active_account: Option<String>,
    recipient: String,
    target_timestamp: i64,
    r#type: Option<String>,
) -> anyhow::Result<SuccessResult> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;

    core_send_receipt(&mut manager, recipient, target_timestamp, r#type).await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_send_receipt<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    recipient: String,
    target_timestamp: i64,
    r#type: Option<String>,
) -> anyhow::Result<SuccessResult> {
    let service_id = resolve_recipient(manager, &recipient).await?;

    let receipt_type = match r#type.as_deref() {
        Some("read") => receipt_message::Type::Read,
        Some("viewed") => receipt_message::Type::Viewed,
        _ => receipt_message::Type::Read, // Default to read
    };

    let receipt_message = ReceiptMessage {
        r#type: Some(receipt_type as i32),
        timestamp: vec![target_timestamp as u64],
    };

    let content_body = ContentBody::ReceiptMessage(receipt_message);
    let timestamp = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis() as u64;

    manager
        .send_message(service_id, content_body, timestamp)
        .await?;

    Ok(SuccessResult {
        success: true,
        message: "Receipt sent successfully.".to_string(),
    })
}

pub(crate) async fn run_send_typing(
    config_dir: &Path,
    active_account: Option<String>,
    recipient: Vec<String>,
    group_id: Option<String>,
    stop: bool,
) -> anyhow::Result<SuccessResult> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;

    core_send_typing(&mut manager, recipient, group_id, stop).await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_send_typing<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    recipient: Vec<String>,
    group_id: Option<String>,
    stop: bool,
) -> anyhow::Result<SuccessResult> {
    let action = if stop {
        typing_message::Action::Stopped
    } else {
        typing_message::Action::Started
    };

    let timestamp = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis() as u64;

    let typing_message = TypingMessage {
        timestamp: Some(timestamp),
        action: Some(action as i32),
        group_id: None, // Will be set if group_id is provided
    };

    if let Some(gid) = group_id {
        let master_key = parse_group_id(&gid)?;
        let mut msg = typing_message.clone();
        msg.group_id = Some(master_key.to_vec());
        let content_body = ContentBody::TypingMessage(msg);
        manager
            .send_message_to_group(&master_key, content_body, timestamp)
            .await?;
    } else {
        let content_body = ContentBody::TypingMessage(typing_message);
        for rec in recipient {
            let service_id = resolve_recipient(manager, &rec).await?;
            manager
                .send_message(service_id, content_body.clone(), timestamp)
                .await?;
        }
    }

    Ok(SuccessResult {
        success: true,
        message: "Typing indicator sent successfully.".to_string(),
    })
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

pub(crate) async fn run_remote_delete(
    config_dir: &Path,
    active_account: Option<String>,
    recipient: Vec<String>,
    group_id: Option<String>,
    _username: Option<String>,
    target_timestamp: i64,
) -> anyhow::Result<SuccessResult> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;
    core_remote_delete(&mut manager, recipient, group_id, target_timestamp).await
}

#[tracing::instrument(skip(manager))]
pub(crate) async fn core_remote_delete<S: Store + Send + 'static>(
    manager: &mut Manager<S, Registered>,
    recipient: Vec<String>,
    group_id: Option<String>,
    target_timestamp: i64,
) -> anyhow::Result<SuccessResult> {
    let delete = data_message::Delete {
        target_sent_timestamp: Some(target_timestamp as u64),
    };

    let data_message = DataMessage {
        delete: Some(delete),
        ..Default::default()
    };
    let content_body = ContentBody::DataMessage(data_message);

    let timestamp = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis() as u64;

    if let Some(gid) = group_id {
        let master_key = parse_group_id(&gid)?;
        manager
            .send_message_to_group(&master_key, content_body, timestamp)
            .await?;
    } else {
        for rec in recipient {
            let service_id = resolve_recipient(manager, &rec).await?;
            manager
                .send_message(service_id, content_body.clone(), timestamp)
                .await?;
        }
    }

    Ok(SuccessResult {
        success: true,
        message: "Remote delete sent successfully.".to_string(),
    })
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
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
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
                profile_key: vec![0u8; 32], // Placeholder
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
        contact.expire_timer = e as u32;
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
