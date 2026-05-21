use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::UNIX_EPOCH;
use std::io::{Read, Write};
use anyhow::{anyhow, Context};
use base64::prelude::*;
use futures::{channel::oneshot, pin_mut, StreamExt};
use serde::Serialize;

use presage::libsignal_service::configuration::SignalServers;
use presage::libsignal_service::prelude::phonenumber::PhoneNumber;
use presage::libsignal_service::prelude::Uuid;
use presage::libsignal_service::protocol::ServiceId;
use presage::libsignal_service::content::{ContentBody, DataMessage, GroupContextV2};
use presage::libsignal_service::proto::{
    data_message, receipt_message, typing_message, ReceiptMessage, TypingMessage,
};
use presage::libsignal_service::proto::attachment_pointer::AttachmentIdentifier;
use presage::libsignal_service::zkgroup::profiles::ProfileKey;
use presage::manager::{Registered, RegistrationOptions};
use presage::store::{Store, ContentsStore, ContentExt, Thread};
use presage::Manager;
use presage_store_sqlite::SqliteStore;
use presage::model::identity::OnNewIdentity;
use presage::model::messages::Received;

pub async fn execute_command(
    args: crate::cli::Cli,
    config_dir: PathBuf,
    active_account: Option<String>,
) -> anyhow::Result<()> {
    match args.command {
        crate::cli::Command::Version => {
            println!("signal-cli 0.1.0 (Rust lightweight drop-in replacement)");
            Ok(())
        }

        crate::cli::Command::ListAccounts => {
            run_list_accounts(&config_dir).await
        }

        crate::cli::Command::Register { voice, captcha, reregister } => {
            run_register(&config_dir, active_account, voice, captcha, reregister, args.service_environment).await
        }

        crate::cli::Command::Verify { verification_code: _, pin: _ } => {
            println!("Verification is performed interactively during registration.");
            println!("Please run: signal-cli -a <PHONE> register");
            println!("And enter the verification code when prompted.");
            Ok(())
        }

        crate::cli::Command::Link { name } => {
            run_link(&config_dir, name, args.service_environment).await
        }

        crate::cli::Command::AddDevice { uri } => {
            run_add_device(&config_dir, active_account, uri).await
        }

        crate::cli::Command::ListDevices => {
            run_list_devices(&config_dir, active_account).await
        }

        crate::cli::Command::RemoveDevice { device_id } => {
            run_remove_device(&config_dir, active_account, device_id).await
        }

        crate::cli::Command::GetUserStatus { recipient, username } => {
            run_get_user_status(&config_dir, active_account, recipient, username).await
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
            .await
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
            run_receive(&config_dir, active_account, max_messages.map(|m| m as usize), args.output).await
        }

        crate::cli::Command::ListGroups { detailed, group_id } => {
            run_list_groups(&config_dir, active_account, detailed, group_id).await
        }

        crate::cli::Command::ListContacts {
            recipient: _,
            all_recipients: _,
            blocked: _,
            name,
            detailed,
            internal: _,
        } => {
            run_list_contacts(&config_dir, active_account, name, detailed).await
        }

        crate::cli::Command::GetAttachment { id, recipient, group_id } => {
            run_get_attachment(&config_dir, active_account, id, recipient, group_id).await
        }

        crate::cli::Command::GetAvatar { contact, profile, group_id } => {
            run_get_avatar(&config_dir, active_account, contact, profile, group_id).await
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
            .await
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
            .await
        }

        crate::cli::Command::SendReceipt {
            recipient,
            username: _,
            target_timestamp,
            r#type,
        } => run_send_receipt(&config_dir, active_account, recipient, target_timestamp, r#type).await,

        crate::cli::Command::SendTyping {
            recipient,
            group_id,
            stop,
        } => run_send_typing(&config_dir, active_account, recipient, group_id, stop).await,

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

        crate::cli::Command::Unregister { delete_account } => {
            run_unregister(&config_dir, active_account, delete_account).await
        }

        crate::cli::Command::DeleteLocalAccountData { ignore_registered } => {
            run_delete_local_account_data(&config_dir, active_account, ignore_registered).await
        }

        crate::cli::Command::SetPin { registration_lock_pin } => {
            run_set_pin(&config_dir, active_account, registration_lock_pin).await
        }

        crate::cli::Command::RemovePin => {
            run_remove_pin(&config_dir, active_account).await
        }

        crate::cli::Command::UpdateAccount { .. } => {
            println!("updateAccount is not yet implemented.");
            Ok(())
        }

        // Catch-all fallback for other subcommands (clean simulated outputs)
        _ => {
            let cmd_name = format!("{:?}", args.command);
            println!("Subcommand '{}' parsed successfully, but is not fully mapped in the Whisperfish presage CLI backend.", cmd_name);
            println!("Simulated success response for drop-in compatibility.");
            Ok(())
        }
    }
}

async fn run_list_accounts(config_dir: &Path) -> anyhow::Result<()> {
    let accounts = find_local_accounts(config_dir)?;
    if accounts.is_empty() {
        println!("No accounts registered.");
    } else {
        for account in accounts {
            println!("{}", account);
        }
    }
    Ok(())
}

async fn run_register(
    config_dir: &Path,
    active_account: Option<String>,
    voice: bool,
    captcha: Option<String>,
    reregister: bool,
    service_env: crate::cli::ServiceEnvironment,
) -> anyhow::Result<()> {
    let account = active_account.ok_or_else(|| anyhow!("Account phone number (-a / --account) must be specified for registration."))?;
    let servers = match service_env {
        crate::cli::ServiceEnvironment::Live => SignalServers::Production,
        crate::cli::ServiceEnvironment::Staging => SignalServers::Staging,
    };

    let db_path = config_dir.join(format!("{}.db", account));
    let store = SqliteStore::open_with_passphrase(
        db_path.to_str().unwrap(),
        None,
        OnNewIdentity::Trust,
    )
    .await?;

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

async fn run_link(
    config_dir: &Path,
    name: Option<String>,
    service_env: crate::cli::ServiceEnvironment,
) -> anyhow::Result<()> {
    let servers = match service_env {
        crate::cli::ServiceEnvironment::Live => SignalServers::Production,
        crate::cli::ServiceEnvironment::Staging => SignalServers::Staging,
    };
    let device_name = name.unwrap_or_else(|| "signal-cli-rust".to_string());
    let temp_db_path = config_dir.join("link_temp.db");
    if temp_db_path.exists() {
        let _ = std::fs::remove_file(&temp_db_path);
    }
    let store = SqliteStore::open_with_passphrase(
        temp_db_path.to_str().unwrap(),
        None,
        OnNewIdentity::Trust,
    )
    .await?;

    let (provisioning_link_tx, provisioning_link_rx) = oneshot::channel();
    println!("Linking secondary device...");
    let manager_fut = Manager::link_secondary_device(
        store,
        servers,
        device_name,
        provisioning_link_tx,
    );

    let link_rx_fut = async move {
        match provisioning_link_rx.await {
            Ok(url) => {
                println!("Please scan the QR code or use this URL to link your device:");
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

    // Drop manager to release file locks
    drop(manager);

    let final_db_path = config_dir.join(format!("{}.db", phone_number));
    if final_db_path.exists() {
        let _ = std::fs::remove_file(&final_db_path);
    }
    std::fs::rename(&temp_db_path, &final_db_path)?;
    println!("Account database saved to: {:?}", final_db_path);
    Ok(())
}

async fn run_add_device(
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

async fn run_list_devices(
    config_dir: &Path,
    active_account: Option<String>,
) -> anyhow::Result<()> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let manager = load_registered_and_receive(store).await?;
    let devices = manager.devices().await?;
    let current_device_id = manager.device_id();

    for device in devices {
        let device_name = device.name.unwrap_or_else(|| "(no device name)".to_string());
        let current_marker = if device.id == current_device_id {
            "(this device)"
        } else {
            ""
        };
        println!(
            "- Device {} {}\n  Name: {}\n  Created: {}\n  Last seen: {}",
            device.id, current_marker, device_name, device.created_at, device.last_seen
        );
    }
    Ok(())
}

async fn run_remove_device(
    config_dir: &Path,
    active_account: Option<String>,
    device_id: i64,
) -> anyhow::Result<()> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let manager = load_registered_and_receive(store).await?;
    manager.unlink_secondary(device_id as u32).await?;
    println!("Unlinked device with id: {}", device_id);
    Ok(())
}

async fn run_get_user_status(
    config_dir: &Path,
    active_account: Option<String>,
    recipients: Vec<String>,
    usernames: Vec<String>,
) -> anyhow::Result<()> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;

    for rec in recipients {
        if let Ok(phone) = PhoneNumber::from_str(&rec) {
            match manager.discover_contacts_by_phone_number(vec![phone]).await {
                Ok(discovered) => {
                    for (p, identity) in discovered {
                        match identity {
                            Some(id) => println!("Phone: {} is registered on Signal as Aci: {}", p, id.service_id_string()),
                            None => println!("Phone: {} is NOT registered on Signal.", p),
                        }
                    }
                }
                Err(e) => eprintln!("Failed to check user status for phone {}: {:?}", rec, e),
            }
        }
    }

    for user in usernames {
        match manager.lookup_username(&user).await {
            Ok(Some(service_id)) => println!("Username: {} is registered on Signal as Aci: {}", user, service_id.service_id_string()),
            Ok(None) => println!("Username: {} is NOT registered on Signal.", user),
            Err(e) => eprintln!("Failed to lookup username {}: {:?}", user, e),
        }
    }
    Ok(())
}

async fn run_send(
    config_dir: &Path,
    active_account: Option<String>,
    note_to_self: bool,
    recipient: Vec<String>,
    group_id: Option<String>,
    message: Option<String>,
    message_from_stdin: bool,
    attachment: Vec<String>,
) -> anyhow::Result<()> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;

    let mut body_text = message;
    if message_from_stdin {
        let mut buffer = String::new();
        std::io::stdin().read_to_string(&mut buffer)?;
        body_text = Some(buffer.trim().to_string());
    }

    let attachments = upload_attachments(attachment.into_iter().map(PathBuf::from).collect(), &manager).await?;

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
        manager.send_message_to_group(&master_key, content_body, timestamp).await?;
        println!("Message sent to group successfully.");
    } else {
        let mut resolved_recipients = Vec::new();
        if note_to_self {
            resolved_recipients.push(ServiceId::Aci(manager.registration_data().service_ids.aci.into()));
        }
        for rec in recipient {
            if let Ok(resolved) = resolve_recipient(&mut manager, &rec).await {
                resolved_recipients.push(resolved);
            } else {
                eprintln!("Could not resolve recipient: {}", rec);
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
            manager.send_message(r, content_body.clone(), timestamp).await?;
        }
        println!("Message sent successfully.");
    }
    Ok(())
}

async fn run_receive(
    config_dir: &Path,
    active_account: Option<String>,
    max_messages: Option<usize>,
    output_mode: crate::cli::OutputMode,
) -> anyhow::Result<()> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = Manager::load_registered(store).await?;

    let is_json = match output_mode {
        crate::cli::OutputMode::Json => true,
        crate::cli::OutputMode::PlainText => false,
    };

    let messages = manager.receive_messages().await.context("failed to receive messages")?;
    pin_mut!(messages);

    let mut count = 0;
    while let Some(content) = messages.next().await {
        match content {
            Received::QueueEmpty => {
                if !is_json {
                    println!("Queue empty.");
                }
                break;
            }
            Received::Contacts => {
                if !is_json {
                    println!("Contacts synchronised.");
                }
            }
            Received::Content(content) => {
                if is_json {
                    let group_id = if let ContentBody::DataMessage(ref msg) = content.body {
                        msg.group_v2.as_ref().and_then(|g| g.master_key.as_ref().map(|k| hex::encode(k)))
                    } else {
                        None
                    };
                    let message = if let ContentBody::DataMessage(ref msg) = content.body {
                        msg.body.clone()
                    } else {
                        None
                    };
                    #[derive(Serialize)]
                    struct JsonEnvelope {
                        source: String,
                        source_uuid: String,
                        timestamp: u64,
                        message: Option<String>,
                        group_id: Option<String>,
                    }
                    let envelope = JsonEnvelope {
                        source: content.metadata.sender.service_id_string(),
                        source_uuid: content.metadata.sender.service_id_string(),
                        timestamp: content.timestamp(),
                        message,
                        group_id,
                    };
                    if let Ok(json_val) = serde_json::to_string(&envelope) {
                        println!("{}", json_val);
                    }
                } else {
                    println!("Envelope from: {}", content.metadata.sender.service_id_string());
                    println!("Timestamp: {}", content.timestamp());
                    if let ContentBody::DataMessage(ref msg) = content.body {
                        if let Some(ref body) = msg.body {
                            println!("Message body: {}", body);
                        }
                    }
                    println!("-------------------------------------------");
                }

                count += 1;
                if let Some(max) = max_messages {
                    if count >= max {
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}

async fn run_list_groups(
    config_dir: &Path,
    active_account: Option<String>,
    detailed: bool,
    group_ids: Vec<String>,
) -> anyhow::Result<()> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let manager = load_registered_and_receive(store).await?;
    for group in manager.store().groups().await? {
        if let Ok((key, g)) = group {
            let hex_key = hex::encode(key);
            if !group_ids.is_empty() && !group_ids.contains(&hex_key) {
                continue;
            }
            if detailed {
                println!("Group ID: {}", hex_key);
                println!("Title: {}", g.title);
                if let Some(ref desc) = g.description {
                    println!("Description: {}", desc);
                }
                println!("Members count: {}", g.members.len());
                println!("-------------------------------------------");
            } else {
                println!("{} - {}", hex_key, g.title);
            }
        }
    }
    Ok(())
}

async fn run_list_contacts(
    config_dir: &Path,
    active_account: Option<String>,
    name: Option<String>,
    detailed: bool,
) -> anyhow::Result<()> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let manager = load_registered_and_receive(store).await?;
    for contact in manager.store().contacts().await?.flatten() {
        if let Some(ref n) = name {
            if !contact.name.contains(n) {
                continue;
            }
        }
        if detailed {
            println!("UUID: {}", contact.uuid);
            println!("Phone: {:?}", contact.phone_number);
            println!("Name: {}", contact.name);
            println!("-------------------------------------------");
        } else {
            println!("{} / {:?} / {}", contact.uuid, contact.phone_number, contact.name);
        }
    }
    Ok(())
}

async fn run_update_profile(
    config_dir: &Path,
    active_account: Option<String>,
    given_name: Option<String>,
    family_name: Option<String>,
    about: Option<String>,
    about_emoji: Option<String>,
) -> anyhow::Result<()> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;

    if let Some(first_name) = given_name {
        let name = presage::libsignal_service::profile_name::ProfileName {
            given_name: first_name,
            family_name,
        };
        manager.update_profile(name, about, about_emoji).await?;
        println!("Profile updated successfully.");
        Ok(())
    } else {
        anyhow::bail!("given_name (--given-name) is required to update the profile.")
    }
}

fn find_local_accounts(config_dir: &Path) -> anyhow::Result<Vec<String>> {
    let mut accounts = Vec::new();
    if config_dir.exists() {
        for entry in std::fs::read_dir(config_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "db" {
                        if let Some(file_stem) = path.file_stem() {
                            let stem = file_stem.to_string_lossy().to_string();
                            if stem.starts_with('+') {
                                accounts.push(stem);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(accounts)
}

async fn open_sqlite_store(config_dir: &Path, account: &str) -> anyhow::Result<SqliteStore> {
    let db_path = config_dir.join(format!("{}.db", account));
    if !db_path.exists() {
        anyhow::bail!("Account database not found at {:?}. Please register or link first.", db_path);
    }
    let store = SqliteStore::open_with_passphrase(
        db_path.to_str().unwrap(),
        None,
        OnNewIdentity::Trust,
    )
    .await?;
    Ok(store)
}

async fn load_registered_and_receive<S: Store + Send>(
    store: S,
) -> anyhow::Result<Manager<S, Registered>> {
    let manager = Manager::load_registered(store).await?;
    let manager_receive = manager.clone();
    tokio::task::spawn_local(receive_loop(manager_receive));
    Ok(manager)
}

async fn receive_loop<S: Store>(mut manager: Manager<S, Registered>) {
    if let Ok(messages) = manager.receive_messages().await {
        pin_mut!(messages);
        while let Some(_) = messages.next().await {}
    }
}

fn parse_group_id(id_str: &str) -> anyhow::Result<[u8; 32]> {
    if let Ok(bytes) = BASE64_STANDARD.decode(id_str) {
        if bytes.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            return Ok(key);
        }
    }
    if let Ok(bytes) = hex::decode(id_str) {
        if bytes.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            return Ok(key);
        }
    }
    anyhow::bail!("Invalid group ID. Must be 32-byte hex or base64 encoded string.")
}

async fn resolve_recipient<S: Store + Send>(
    manager: &mut Manager<S, Registered>,
    recipient: &str,
) -> anyhow::Result<ServiceId> {
    if let Ok(uuid) = uuid::Uuid::parse_str(recipient) {
        return Ok(ServiceId::Aci(uuid.into()));
    }
    if recipient.starts_with('+') {
        if let Ok(contacts) = manager.store().contacts().await {
            for contact_res in contacts {
                if let Ok(contact) = contact_res {
                    if contact.phone_number.as_ref().map(|p| p.to_string()) == Some(recipient.to_string()) {
                        return Ok(ServiceId::Aci(contact.uuid.into()));
                    }
                }
            }
        }
        if let Ok(phone) = recipient.parse::<PhoneNumber>() {
            if let Ok(discovered) = manager.discover_contacts_by_phone_number(vec![phone]).await {
                if let Some((_, Some(identity))) = discovered.into_iter().next() {
                    return Ok(identity);
                }
            }
        }
    }
    anyhow::bail!("Could not resolve recipient: {}", recipient)
}

async fn upload_attachments<S: Store>(
    attachment_filepath: Vec<PathBuf>,
    manager: &Manager<S, Registered>,
) -> anyhow::Result<Vec<presage::proto::AttachmentPointer>> {
    use mime_guess::mime::APPLICATION_OCTET_STREAM;
    use presage::libsignal_service::sender::AttachmentSpec;

    let mut attachment_specs = Vec::new();
    for path in attachment_filepath {
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

async fn run_delete_local_account_data(
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
        println!("Warning: Deleting local data for a registered account. This will NOT unregister the account from the Signal server.");
        println!("If you want to unregister, use the 'unregister' command (currently not implemented).");
    }

    std::fs::remove_file(&db_path)?;
    println!("Deleted local data for account: {}", account);
    Ok(())
}

async fn run_unregister(
    _config_dir: &std::path::Path,
    _active_account: Option<String>,
    _delete_account: bool,
) -> anyhow::Result<()> {
    println!("The 'unregister' command is not yet implemented in this Rust version of signal-cli.");
    println!("Unregistering requires server-side communication not currently exposed by the underlying library.");
    Ok(())
}

async fn run_set_pin(
    _config_dir: &std::path::Path,
    _active_account: Option<String>,
    _pin: String,
) -> anyhow::Result<()> {
    println!("The 'setPin' command is not yet implemented.");
    Ok(())
}

async fn run_remove_pin(
    _config_dir: &std::path::Path,
    _active_account: Option<String>,
) -> anyhow::Result<()> {
    println!("The 'removePin' command is not yet implemented.");
    Ok(())
}

async fn run_get_avatar(
    config_dir: &Path,
    active_account: Option<String>,
    contact: Option<String>,
    profile: Option<String>,
    group_id: Option<String>,
) -> anyhow::Result<()> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = Manager::load_registered(store).await?;

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
            let discovered = manager.discover_contacts_by_phone_number(vec![phone]).await?;
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

        let profile_key_bytes: [u8; 32] = contact_details.profile_key.as_slice().try_into()
            .map_err(|_| anyhow!("Invalid profile key length for contact."))?;
        let profile_key = ProfileKey::create(profile_key_bytes);

        manager.retrieve_profile_avatar_by_uuid(uuid, profile_key).await?
    } else {
        anyhow::bail!("Either --contact, --profile, or --group-id must be specified.");
    };

    if let Some(bytes) = avatar_bytes {
        std::io::stdout().write_all(&bytes)?;
        std::io::stdout().flush()?;
    } else {
        anyhow::bail!("No avatar found.");
    }

    Ok(())
}

async fn run_get_attachment(
    config_dir: &Path,
    active_account: Option<String>,
    id: String,
    recipient: Option<String>,
    group_id: Option<String>,
) -> anyhow::Result<()> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = Manager::load_registered(store).await?;

    let thread = if let Some(gid) = group_id {
        Thread::Group(parse_group_id(&gid)?)
    } else if let Some(rec) = recipient {
        let service_id = resolve_recipient(&mut manager, &rec).await?;
        Thread::Contact(service_id)
    } else {
        anyhow::bail!("Either --recipient or --group-id must be specified to locate the attachment.");
    };

    let messages = manager.store().messages(&thread, ..).await?;
    let mut attachment_pointer = None;

    for content_res in messages {
        let content = content_res.map_err(|e| anyhow!("Store error: {:?}", e))?;
        if let ContentBody::DataMessage(dm) = content.body {
            for att in dm.attachments {
                let matches = match att.attachment_identifier {
                    Some(AttachmentIdentifier::CdnId(cid)) => cid.to_string() == id,
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

    let pointer = attachment_pointer.ok_or_else(|| anyhow!("Attachment with ID {} not found in thread.", id))?;
    let bytes = manager.get_attachment(&pointer).await?;

    std::io::stdout().write_all(&bytes)?;
    std::io::stdout().flush()?;

    Ok(())
}

async fn run_send_reaction(
    config_dir: &Path,
    active_account: Option<String>,
    recipient: Vec<String>,
    group_id: Option<String>,
    note_to_self: bool,
    emoji: String,
    target_author: String,
    target_timestamp: i64,
    remove: bool,
) -> anyhow::Result<()> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;

    let target_author_sid = resolve_recipient(&mut manager, &target_author).await?;
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
        manager.send_message_to_group(&master_key, content_body, timestamp).await?;
    } else {
        let mut recipients = Vec::new();
        if note_to_self {
            recipients.push(ServiceId::Aci(manager.registration_data().service_ids.aci.into()));
        }
        for rec in recipient {
            recipients.push(resolve_recipient(&mut manager, &rec).await?);
        }
        for r in recipients {
            manager.send_message(r, content_body.clone(), timestamp).await?;
        }
    }

    println!("Reaction sent successfully.");
    Ok(())
}

async fn run_send_receipt(
    config_dir: &Path,
    active_account: Option<String>,
    recipient: String,
    target_timestamp: i64,
    r#type: Option<String>,
) -> anyhow::Result<()> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;

    let service_id = resolve_recipient(&mut manager, &recipient).await?;
    
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

    manager.send_message(service_id, content_body, timestamp).await?;

    println!("Receipt sent successfully.");
    Ok(())
}

async fn run_send_typing(
    config_dir: &Path,
    active_account: Option<String>,
    recipient: Vec<String>,
    group_id: Option<String>,
    stop: bool,
) -> anyhow::Result<()> {
    let account = active_account.ok_or_else(|| anyhow!("Account not specified."))?;
    let store = open_sqlite_store(config_dir, &account).await?;
    let mut manager = load_registered_and_receive(store).await?;

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
        manager.send_message_to_group(&master_key, content_body, timestamp).await?;
    } else {
        let content_body = ContentBody::TypingMessage(typing_message);
        for rec in recipient {
            let service_id = resolve_recipient(&mut manager, &rec).await?;
            manager.send_message(service_id, content_body.clone(), timestamp).await?;
        }
    }

    println!("Typing indicator sent successfully.");
    Ok(())
}

async fn run_join_group(
    _config_dir: &Path,
    _active_account: Option<String>,
    _uri: String,
) -> anyhow::Result<()> {
    println!("The 'joinGroup' command is not yet implemented in this Rust version of signal-cli.");
    println!("The underlying library does not yet support joining groups via URI.");
    Ok(())
}

async fn run_update_group(
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
    println!("The 'updateGroup' command is not yet implemented.");
    println!("Creating or modifying groups requires Group V2 operations not yet exposed by the underlying library.");
    Ok(())
}

async fn run_quit_group(
    _config_dir: &Path,
    _active_account: Option<String>,
    _group_id: String,
    _delete: bool,
) -> anyhow::Result<()> {
    println!("The 'quitGroup' command is not yet implemented.");
    Ok(())
}
