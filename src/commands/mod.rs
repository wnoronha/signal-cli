use std::path::PathBuf;

use crate::errors::SignalError;
use crate::output::Render;
use crate::output::render_command_error;
use crate::utils::{parse_sticker_pack_uri, push_store_identity};

mod envelope;
mod group_id;
mod runtime;
mod stubs;

pub(crate) use envelope::build_receive_envelope;
pub(crate) use group_id::parse_group_id;
pub(crate) use runtime::{load_registered, load_registered_and_receive};
pub(crate) use stubs::*;

mod account;
mod contacts;
mod devices;
mod groups;
mod media;
mod messaging;
mod stickers;

pub(crate) use account::*;
pub(crate) use contacts::*;
pub(crate) use devices::*;
pub(crate) use groups::*;
pub(crate) use media::*;
pub(crate) use messaging::*;
pub(crate) use stickers::*;

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

        crate::cli::Command::Verify { .. } => {
            not_implemented("verify (use `register` and enter the SMS/voice code when prompted)")
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

        crate::cli::Command::Whoami => {
            run_whoami(&config_dir, active_account)
                .await?
                .render(args.output);
            Ok(())
        }

        crate::cli::Command::ClearSessions { recipient } => {
            run_clear_sessions(&config_dir, active_account, recipient)
                .await?
                .render(args.output);
            Ok(())
        }

        crate::cli::Command::SendSessionReset {
            recipient,
            timestamp,
        } => {
            run_send_session_reset(&config_dir, active_account, recipient, timestamp)
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
                option
                    .into_iter()
                    .map(|o| crate::utils::i32_from_i64_checked(o, "option"))
                    .collect::<Result<Vec<_>, _>>()?,
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
            not_implemented("account settings and migration commands (not exposed by presage)")
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

        crate::cli::Command::RemoveStickerPack { uri } => {
            let (pack_id, pack_key) = parse_sticker_pack_uri(&uri)?;
            run_remove_sticker_pack(&config_dir, active_account, pack_id, pack_key)
                .await?
                .render(args.output);
            Ok(())
        }

        crate::cli::Command::GetSticker {
            pack_id,
            sticker_id,
        } => {
            run_get_sticker(
                &config_dir,
                active_account,
                pack_id,
                crate::utils::u32_from_i64_nonnegative(sticker_id, "sticker_id")?,
            )
            .await?
            .render(args.output);
            Ok(())
        }

        crate::cli::Command::Daemon { .. } | crate::cli::Command::JsonRpc { .. } => {
            crate::rpc::server::run_json_rpc(config_dir, active_account, args.command).await
        }

        crate::cli::Command::SubmitRateLimitChallenge { challenge, captcha } => {
            run_submit_rate_limit_challenge(&config_dir, active_account, challenge, captcha)
                .await?
                .render(args.output);
            Ok(())
        }

        crate::cli::Command::SendAdminDelete { .. } | crate::cli::Command::UpdateDevice { .. } => {
            not_implemented(args.command.name())
        }
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
