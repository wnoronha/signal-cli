use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[clap(
    name = "signal-cli",
    about = "A commandline interface for the Signal messenger",
    version
)]
pub struct Cli {
    #[clap(long = "config", help = "Set the path, where to store the config")]
    pub config: Option<PathBuf>,

    #[clap(
        short = 'a',
        long = "account",
        help = "Specify your phone number, that will be your identifier"
    )]
    pub account: Option<String>,

    #[clap(short = 'o', long = "output", default_value = "plain-text", value_enum)]
    pub output: OutputMode,

    #[clap(long = "service-environment", default_value = "live", value_enum)]
    pub service_environment: ServiceEnvironment,

    #[clap(
        long = "trust-new-identities",
        default_value = "on-first-use",
        value_enum
    )]
    pub trust_new_identities: TrustMode,

    #[clap(
        short = 'v',
        long = "verbose",
        help = "Raise log level and include lib signal logs"
    )]
    pub verbose: bool,

    #[clap(
        long = "log-file",
        help = "Append tracing log output to this file (default: stderr only)"
    )]
    pub log_file: Option<PathBuf>,

    #[clap(
        long = "scrub-log",
        help = "Not implemented: logs are not redacted (accepted for Java signal-cli compatibility)"
    )]
    pub scrub_log: bool,

    #[clap(
        long = "dbus",
        hide = true,
        help = "Not implemented: use daemon JSON-RPC instead"
    )]
    pub dbus: bool,

    #[clap(long = "dbus-system", hide = true)]
    pub dbus_system: bool,

    #[clap(long = "bus-name", hide = true)]
    pub bus_name: Option<String>,

    #[clap(long = "disable-send-log", hide = true)]
    pub disable_send_log: bool,

    #[clap(
        long = "attachment-dir",
        help = "Restrict file attachments to this directory (daemon operators: also honors SIGNAL_CLI_ATTACHMENT_DIR)"
    )]
    pub attachment_dir: Option<PathBuf>,

    #[clap(subcommand)]
    pub command: Command,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    #[clap(name = "plain-text")]
    PlainText,
    #[clap(name = "json")]
    Json,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceEnvironment {
    Live,
    Staging,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustMode {
    #[clap(name = "on-first-use")]
    OnFirstUse,
    Always,
    Never,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    #[clap(
        name = "register",
        about = "Register a phone number with SMS or voice verification"
    )]
    Register {
        #[clap(
            short = 'v',
            long = "voice",
            help = "The verification should be done over voice, not SMS"
        )]
        voice: bool,

        #[clap(long = "captcha", help = "The captcha token")]
        captcha: Option<String>,

        #[clap(
            long = "reregister",
            help = "Register even if account is already registered"
        )]
        reregister: bool,
    },

    #[clap(
        name = "verify",
        about = "Verify the number using the code received via SMS or voice"
    )]
    Verify {
        #[clap(help = "The verification code")]
        verification_code: String,

        #[clap(short = 'p', long = "pin", help = "The registration lock PIN")]
        pin: Option<String>,
    },

    #[clap(name = "unregister", about = "Disable push support for this device")]
    Unregister {
        #[clap(
            long = "delete-account",
            help = "Delete account completely from server"
        )]
        delete_account: bool,
    },

    #[clap(
        name = "deleteLocalAccountData",
        about = "Delete all local data for this account"
    )]
    DeleteLocalAccountData {
        #[clap(
            long = "ignore-registered",
            help = "Delete the account data even though registered"
        )]
        ignore_registered: bool,
    },

    #[clap(
        name = "updateAccount",
        about = "Update the account attributes on the signal server"
    )]
    UpdateAccount {
        #[clap(short = 'n', long = "device-name", help = "Set a new device name")]
        device_name: Option<String>,

        #[clap(
            short = 'u',
            long = "username",
            help = "Specify a username nickname or nickname.discriminator"
        )]
        username: Option<String>,

        #[clap(
            long = "delete-username",
            help = "Delete the username associated with this account"
        )]
        delete_username: bool,

        #[clap(
            long = "unrestricted-unidentified-sender",
            help = "Enable if anyone can send unidentified messages"
        )]
        unrestricted_unidentified_sender: Option<bool>,

        #[clap(
            long = "discoverable-by-number",
            help = "Enable if discoverable by phone number"
        )]
        discoverable_by_number: Option<bool>,

        #[clap(
            long = "number-sharing",
            help = "Indicates if Signal should share its phone number"
        )]
        number_sharing: Option<bool>,
    },

    #[clap(name = "startChangeNumber", about = "Change account to new number")]
    StartChangeNumber {
        #[clap(help = "New phone number")]
        recipient: String,

        #[clap(short = 'v', long = "voice")]
        voice: bool,

        #[clap(long = "captcha")]
        captcha: Option<String>,
    },

    #[clap(name = "finishChangeNumber", about = "Complete change number")]
    FinishChangeNumber {
        #[clap(help = "New phone number")]
        recipient: String,

        #[clap(short = 'v', long = "verification-code", help = "Verification code")]
        verification_code: Option<String>,

        #[clap(short = 'p', long = "pin", help = "Registration lock pin")]
        pin: Option<String>,
    },

    #[clap(
        name = "updateConfiguration",
        about = "Update signal configs and sync them to linked devices"
    )]
    UpdateConfiguration {
        #[clap(long = "read-receipts")]
        read_receipts: Option<bool>,

        #[clap(long = "unidentified-delivery-indicators")]
        unidentified_delivery_indicators: Option<bool>,

        #[clap(long = "typing-indicators")]
        typing_indicators: Option<bool>,

        #[clap(long = "link-previews")]
        link_previews: Option<bool>,
    },

    #[clap(name = "setPin", about = "Set a registration lock pin")]
    SetPin {
        #[clap(help = "The registration lock PIN")]
        registration_lock_pin: String,
    },

    #[clap(name = "removePin", about = "Remove the registration lock pin")]
    RemovePin,

    #[clap(name = "link", about = "Link to an existing device")]
    Link {
        #[clap(short = 'n', long = "name", help = "Name to describe this device")]
        name: Option<String>,
    },

    #[clap(name = "addDevice", about = "Link another device to this device")]
    AddDevice {
        #[clap(long = "uri", help = "The uri contained in the QR code")]
        uri: String,
    },

    #[clap(name = "listAccounts", about = "Show a list of registered accounts")]
    ListAccounts,

    #[clap(name = "listDevices", about = "Show a list of linked devices")]
    ListDevices,

    #[clap(name = "removeDevice", about = "Remove a linked device")]
    RemoveDevice {
        #[clap(short = 'd', long = "device-id", help = "Device to remove")]
        device_id: i64,
    },

    #[clap(
        name = "whoami",
        about = "Show registered account metadata from the server"
    )]
    Whoami,

    #[clap(
        name = "clearSessions",
        about = "Clear Signal sessions for a recipient"
    )]
    ClearSessions {
        #[clap(help = "Recipient UUID or phone number")]
        recipient: String,
    },

    #[clap(
        name = "sendSessionReset",
        about = "Send a session reset to a recipient"
    )]
    SendSessionReset {
        #[clap(help = "Recipient UUID or phone number")]
        recipient: String,
        #[clap(long = "timestamp", help = "Optional message timestamp for the reset")]
        timestamp: Option<u64>,
    },

    #[clap(
        name = "getUserStatus",
        about = "Check if users are registered on Signal"
    )]
    GetUserStatus {
        #[clap(help = "Numbers to check")]
        recipient: Vec<String>,

        #[clap(long = "username", help = "Usernames to check")]
        username: Vec<String>,
    },

    #[clap(name = "sendAdminDelete", about = "Send admin delete message")]
    SendAdminDelete {
        #[clap(short = 'g', long = "group-id", help = "Group ID in base64")]
        group_id: String,

        #[clap(long = "notify-self")]
        notify_self: bool,

        #[clap(
            short = 'a',
            long = "target-author",
            help = "Number of the original author"
        )]
        target_author: String,

        #[clap(
            short = 't',
            long = "target-timestamp",
            help = "Timestamp of the target message"
        )]
        target_timestamp: i64,

        #[clap(long = "story")]
        story: bool,
    },

    #[clap(name = "send", about = "Send a message to another user or group")]
    Send {
        #[clap(help = "Recipients")]
        recipient: Vec<String>,

        #[clap(long = "note-to-self")]
        note_to_self: bool,

        #[clap(long = "notify-self")]
        notify_self: bool,

        #[clap(short = 'g', long = "group-id")]
        group_id: Option<String>,

        #[clap(short = 'u', long = "username")]
        username: Option<String>,

        #[clap(short = 'm', long = "message")]
        message: Option<String>,

        #[clap(long = "message-from-stdin")]
        message_from_stdin: bool,

        #[clap(short = 'a', long = "attachment")]
        attachment: Vec<String>,

        #[clap(long = "view-once")]
        view_once: bool,

        #[clap(long = "sticker")]
        sticker: Option<String>,

        #[clap(long = "mention")]
        mention: Vec<String>,

        #[clap(long = "text-style")]
        text_style: Vec<String>,

        #[clap(long = "quote-timestamp")]
        quote_timestamp: Option<i64>,

        #[clap(long = "quote-author")]
        quote_author: Option<String>,

        #[clap(long = "quote-message")]
        quote_message: Option<String>,

        #[clap(long = "quote-mention")]
        quote_mention: Vec<String>,

        #[clap(long = "quote-text-style")]
        quote_text_style: Vec<String>,

        #[clap(long = "quote-attachment")]
        quote_attachment: Vec<String>,

        #[clap(long = "preview-url")]
        preview_url: Option<String>,

        #[clap(long = "preview-title")]
        preview_title: Option<String>,

        #[clap(long = "preview-description")]
        preview_description: Option<String>,

        #[clap(long = "preview-image")]
        preview_image: Option<String>,

        #[clap(long = "story-timestamp")]
        story_timestamp: Option<i64>,

        #[clap(long = "story-author")]
        story_author: Option<String>,

        #[clap(short = 'e', long = "end-session")]
        end_session: bool,

        #[clap(long = "edit-timestamp")]
        edit_timestamp: Option<i64>,

        #[clap(long = "no-urgent")]
        no_urgent: bool,
    },

    #[clap(name = "sendPollCreate", about = "Send poll create message")]
    SendPollCreate {
        #[clap(help = "Recipients")]
        recipient: Vec<String>,

        #[clap(long = "note-to-self")]
        note_to_self: bool,

        #[clap(long = "notify-self")]
        notify_self: bool,

        #[clap(short = 'g', long = "group-id")]
        group_id: Option<String>,

        #[clap(short = 'u', long = "username")]
        username: Option<String>,

        #[clap(short = 'q', long = "question", help = "Question for the poll")]
        question: String,

        #[clap(long = "no-multi")]
        no_multi: bool,

        #[clap(short = 'o', long = "option")]
        option: Vec<String>,
    },

    #[clap(name = "sendPollVote", about = "Send poll vote message")]
    SendPollVote {
        #[clap(help = "Recipients")]
        recipient: Vec<String>,

        #[clap(long = "note-to-self")]
        note_to_self: bool,

        #[clap(long = "notify-self")]
        notify_self: bool,

        #[clap(short = 'g', long = "group-id")]
        group_id: Option<String>,

        #[clap(short = 'u', long = "username")]
        username: Option<String>,

        #[clap(long = "poll-author")]
        poll_author: String,

        #[clap(long = "poll-timestamp")]
        poll_timestamp: i64,

        #[clap(short = 'o', long = "option")]
        option: Vec<i64>,

        #[clap(long = "vote-count")]
        vote_count: i64,
    },

    #[clap(name = "sendPollTerminate", about = "Send poll terminate message")]
    SendPollTerminate {
        #[clap(help = "Recipients")]
        recipient: Vec<String>,

        #[clap(long = "note-to-self")]
        note_to_self: bool,

        #[clap(long = "notify-self")]
        notify_self: bool,

        #[clap(short = 'g', long = "group-id")]
        group_id: Option<String>,

        #[clap(short = 'u', long = "username")]
        username: Option<String>,

        #[clap(long = "poll-timestamp")]
        poll_timestamp: i64,
    },

    #[clap(
        name = "sendMessageRequestResponse",
        about = "Send response to a message request"
    )]
    SendMessageRequestResponse {
        #[clap(help = "Recipients")]
        recipient: Vec<String>,

        #[clap(short = 'g', long = "group-id")]
        group_id: Option<String>,

        #[clap(short = 'u', long = "username")]
        username: Option<String>,

        #[clap(long = "type")]
        r#type: String, // accept, delete
    },

    #[clap(
        name = "sendPaymentNotification",
        about = "Send a payment notification"
    )]
    SendPaymentNotification {
        #[clap(help = "Recipient")]
        recipient: String,

        #[clap(long = "receipt")]
        receipt: String,

        #[clap(long = "note")]
        note: Option<String>,
    },

    #[clap(name = "sendPinMessage", about = "Send pin message")]
    SendPinMessage {
        #[clap(help = "Recipients")]
        recipient: Vec<String>,

        #[clap(short = 'g', long = "group-id")]
        group_id: Option<String>,

        #[clap(short = 'u', long = "username")]
        username: Option<String>,

        #[clap(long = "note-to-self")]
        note_to_self: bool,

        #[clap(long = "notify-self")]
        notify_self: bool,

        #[clap(short = 'd', long = "pin-duration")]
        pin_duration: Option<i64>,

        #[clap(short = 'a', long = "target-author")]
        target_author: String,

        #[clap(short = 't', long = "target-timestamp")]
        target_timestamp: i64,
    },

    #[clap(name = "sendUnpinMessage", about = "Send unpin message")]
    SendUnpinMessage {
        #[clap(help = "Recipients")]
        recipient: Vec<String>,

        #[clap(short = 'g', long = "group-id")]
        group_id: Option<String>,

        #[clap(short = 'u', long = "username")]
        username: Option<String>,

        #[clap(long = "note-to-self")]
        note_to_self: bool,

        #[clap(long = "notify-self")]
        notify_self: bool,

        #[clap(short = 'a', long = "target-author")]
        target_author: String,

        #[clap(short = 't', long = "target-timestamp")]
        target_timestamp: i64,
    },

    #[clap(name = "sendReaction", about = "Send reaction to a message")]
    SendReaction {
        #[clap(help = "Recipients")]
        recipient: Vec<String>,

        #[clap(short = 'g', long = "group-id")]
        group_id: Option<String>,

        #[clap(short = 'u', long = "username")]
        username: Option<String>,

        #[clap(long = "note-to-self")]
        note_to_self: bool,

        #[clap(long = "notify-self")]
        notify_self: bool,

        #[clap(short = 'e', long = "emoji")]
        emoji: String,

        #[clap(short = 'a', long = "target-author")]
        target_author: String,

        #[clap(short = 't', long = "target-timestamp")]
        target_timestamp: i64,

        #[clap(short = 'r', long = "remove")]
        remove: bool,

        #[clap(long = "story")]
        story: bool,
    },

    #[clap(name = "sendReceipt", about = "Send read or viewed receipt")]
    SendReceipt {
        #[clap(help = "Recipient")]
        recipient: String,

        #[clap(short = 'u', long = "username")]
        username: Option<String>,

        #[clap(short = 't', long = "target-timestamp")]
        target_timestamp: i64,

        #[clap(long = "type")]
        r#type: Option<String>, // read, viewed
    },

    #[clap(name = "sendTyping", about = "Send typing message")]
    SendTyping {
        #[clap(help = "Recipients")]
        recipient: Vec<String>,

        #[clap(short = 'g', long = "group-id")]
        group_id: Option<String>,

        #[clap(short = 's', long = "stop")]
        stop: bool,
    },

    #[clap(
        name = "remoteDelete",
        about = "Remotely delete a previously sent message"
    )]
    RemoteDelete {
        #[clap(help = "Recipients")]
        recipient: Vec<String>,

        #[clap(short = 'g', long = "group-id")]
        group_id: Option<String>,

        #[clap(short = 'u', long = "username")]
        username: Option<String>,

        #[clap(short = 't', long = "target-timestamp")]
        target_timestamp: i64,
    },

    #[clap(name = "receive", about = "Query the server for new messages")]
    Receive {
        #[clap(short = 't', long = "timeout")]
        timeout: Option<i64>,

        #[clap(long = "max-messages")]
        max_messages: Option<i64>,

        #[clap(long = "ignore-attachments")]
        ignore_attachments: bool,

        #[clap(long = "ignore-stories")]
        ignore_stories: bool,

        #[clap(long = "ignore-avatars")]
        ignore_avatars: bool,

        #[clap(long = "ignore-stickers")]
        ignore_stickers: bool,

        #[clap(long = "send-read-receipts")]
        send_read_receipts: bool,
    },

    #[clap(name = "joinGroup", about = "Join a group via link")]
    JoinGroup {
        #[clap(long = "uri")]
        uri: String,
    },

    #[clap(name = "updateGroup", about = "Create or update a group")]
    UpdateGroup {
        #[clap(short = 'g', long = "group-id")]
        group_id: Option<String>,

        #[clap(short = 'n', long = "name")]
        name: Option<String>,

        #[clap(short = 'd', long = "description")]
        description: Option<String>,

        #[clap(short = 'a', long = "avatar")]
        avatar: Option<String>,

        #[clap(short = 'm', long = "member")]
        member: Vec<String>,

        #[clap(short = 'r', long = "remove-member")]
        remove_member: Vec<String>,

        #[clap(long = "admin")]
        admin: Vec<String>,

        #[clap(long = "remove-admin")]
        remove_admin: Vec<String>,

        #[clap(long = "ban")]
        ban: Vec<String>,

        #[clap(long = "unban")]
        unban: Vec<String>,

        #[clap(long = "reset-link")]
        reset_link: bool,

        #[clap(long = "link")]
        link: Option<String>, // enabled, enabled-with-approval, disabled

        #[clap(long = "set-permission-add-member")]
        set_permission_add_member: Option<String>,

        #[clap(long = "set-permission-edit-details")]
        set_permission_edit_details: Option<String>,

        #[clap(long = "set-permission-send-messages")]
        set_permission_send_messages: Option<String>,

        #[clap(short = 'e', long = "expiration")]
        expiration: Option<i64>,

        #[clap(long = "member-label-emoji")]
        member_label_emoji: Option<String>,

        #[clap(long = "member-label")]
        member_label: Option<String>,
    },

    #[clap(name = "quitGroup", about = "Leave a group")]
    QuitGroup {
        #[clap(short = 'g', long = "group-id")]
        group_id: String,

        #[clap(long = "delete")]
        delete: bool,
    },

    #[clap(name = "listGroups", about = "Show a list of known groups")]
    ListGroups {
        #[clap(short = 'd', long = "detailed")]
        detailed: bool,

        #[clap(short = 'g', long = "group-id")]
        group_id: Vec<String>,
    },

    #[clap(name = "listContacts", about = "Show a list of known contacts")]
    ListContacts {
        #[clap(help = "Recipients")]
        recipient: Vec<String>,

        #[clap(short = 'a', long = "all-recipients")]
        all_recipients: bool,

        #[clap(long = "blocked")]
        blocked: Option<bool>,

        #[clap(long = "name")]
        name: Option<String>,

        #[clap(long = "detailed")]
        detailed: bool,

        #[clap(long = "internal")]
        internal: bool,
    },

    #[clap(name = "listIdentities", about = "List all known identity keys")]
    ListIdentities {
        #[clap(short = 'n', long = "number")]
        number: Option<String>,
    },

    #[clap(name = "trust", about = "Set the trust level of a given recipient")]
    Trust {
        #[clap(help = "Recipient")]
        recipient: String,

        #[clap(short = 'a', long = "trust-all-known-keys")]
        trust_all_known_keys: bool,

        #[clap(short = 'v', long = "verified-safety-number")]
        verified_safety_number: Option<String>,
    },

    #[clap(name = "updateProfile", about = "Update profile information")]
    UpdateProfile {
        #[clap(long = "given-name", alias = "name")]
        given_name: Option<String>,

        #[clap(long = "family-name")]
        family_name: Option<String>,

        #[clap(long = "about")]
        about: Option<String>,

        #[clap(long = "about-emoji")]
        about_emoji: Option<String>,

        #[clap(long = "avatar")]
        avatar: Option<String>,

        #[clap(long = "remove-avatar")]
        remove_avatar: bool,

        #[clap(long = "mobile-coin-address", alias = "mobilecoin-address")]
        mobile_coin_address: Option<String>,
    },

    #[clap(name = "updateContact", about = "Update contact information")]
    UpdateContact {
        #[clap(help = "Recipient")]
        recipient: String,

        #[clap(long = "given-name", alias = "name")]
        given_name: Option<String>,

        #[clap(long = "family-name")]
        family_name: Option<String>,

        #[clap(long = "nick-given-name")]
        nick_given_name: Option<String>,

        #[clap(long = "nick-family-name")]
        nick_family_name: Option<String>,

        #[clap(long = "note")]
        note: Option<String>,

        #[clap(short = 'e', long = "expiration")]
        expiration: Option<i64>,

        #[clap(long = "phone-number")]
        phone_number: Option<String>,
    },

    #[clap(name = "updateDevice", about = "Update a linked device")]
    UpdateDevice {
        #[clap(short = 'd', long = "device-id")]
        device_id: i64,

        #[clap(short = 'n', long = "device-name")]
        device_name: String,
    },

    #[clap(name = "removeContact", about = "Remove contact info")]
    RemoveContact {
        #[clap(help = "Recipient")]
        recipient: String,

        #[clap(long = "hide")]
        hide: bool,

        #[clap(long = "forget")]
        forget: bool,
    },

    #[clap(name = "block", about = "Block given contacts or groups")]
    Block {
        #[clap(help = "Recipients")]
        recipient: Vec<String>,

        #[clap(short = 'g', long = "group-id")]
        group_id: Vec<String>,
    },

    #[clap(name = "unblock", about = "Unblock given contacts or groups")]
    Unblock {
        #[clap(help = "Recipients")]
        recipient: Vec<String>,

        #[clap(short = 'g', long = "group-id")]
        group_id: Vec<String>,
    },

    #[clap(name = "sendContacts", about = "Send sync message with contact list")]
    SendContacts,

    #[clap(
        name = "sendSyncRequest",
        about = "Send a sync request to primary device"
    )]
    SendSyncRequest,

    #[clap(name = "uploadStickerPack", about = "Upload a new sticker pack")]
    UploadStickerPack {
        #[clap(help = "Path of manifest.json or zip")]
        path: String,
    },

    #[clap(
        name = "listStickerPacks",
        about = "Show a list of known sticker packs"
    )]
    ListStickerPacks,

    #[clap(name = "addStickerPack", about = "Install a sticker pack")]
    AddStickerPack {
        #[clap(long = "uri")]
        uri: String,
    },

    #[clap(name = "removeStickerPack", about = "Remove an installed sticker pack")]
    RemoveStickerPack {
        #[clap(long = "uri")]
        uri: String,
    },

    #[clap(name = "getAttachment", about = "Get raw attachment data")]
    GetAttachment {
        #[clap(long = "id")]
        id: String,

        #[clap(long = "recipient")]
        recipient: Option<String>,

        #[clap(short = 'g', long = "group-id")]
        group_id: Option<String>,
    },

    #[clap(name = "getAvatar", about = "Get raw contact or group avatar data")]
    GetAvatar {
        #[clap(long = "contact")]
        contact: Option<String>,

        #[clap(long = "profile")]
        profile: Option<String>,

        #[clap(short = 'g', long = "group-id")]
        group_id: Option<String>,
    },

    #[clap(name = "getSticker", about = "Get raw sticker data")]
    GetSticker {
        #[clap(long = "pack-id")]
        pack_id: String,

        #[clap(long = "sticker-id")]
        sticker_id: i64,
    },

    #[clap(name = "daemon", about = "Run in daemon mode with JSON-RPC or dbus")]
    Daemon {
        #[clap(
            long = "socket",
            num_args = 0..=1,
            default_missing_value = "/tmp/signal-cli.sock"
        )]
        socket: Option<String>,

        #[clap(
            long = "tcp",
            num_args = 0..=1,
            default_missing_value = "127.0.0.1:8080"
        )]
        tcp: Option<String>,

        #[clap(
            long = "http",
            num_args = 0..=1,
            default_missing_value = "127.0.0.1:8080"
        )]
        http: Option<String>,

        #[clap(long = "dbus", hide = true)]
        dbus: bool,

        #[clap(long = "dbus-system", hide = true)]
        dbus_system: bool,

        #[clap(long = "bus-name")]
        bus_name: Option<String>,

        #[clap(long = "ignore-attachments")]
        ignore_attachments: bool,

        #[clap(long = "ignore-stories")]
        ignore_stories: bool,

        #[clap(long = "ignore-avatars")]
        ignore_avatars: bool,

        #[clap(long = "ignore-stickers")]
        ignore_stickers: bool,

        #[clap(long = "send-read-receipts")]
        send_read_receipts: bool,

        #[clap(long = "no-receive-stdout")]
        no_receive_stdout: bool,

        #[clap(long = "receive-mode")]
        receive_mode: Option<String>, // on-start, on-connection, manual
    },

    #[clap(name = "jsonRpc", about = "Run in JSON-RPC stdin/stdout mode")]
    JsonRpc {
        #[clap(long = "ignore-attachments")]
        ignore_attachments: bool,

        #[clap(long = "ignore-stories")]
        ignore_stories: bool,

        #[clap(long = "ignore-avatars")]
        ignore_avatars: bool,

        #[clap(long = "ignore-stickers")]
        ignore_stickers: bool,

        #[clap(long = "send-read-receipts")]
        send_read_receipts: bool,

        #[clap(long = "receive-mode")]
        receive_mode: Option<String>, // on-start, manual
    },

    #[clap(
        name = "submitRateLimitChallenge",
        about = "Submit challenge to lift rate limits"
    )]
    SubmitRateLimitChallenge {
        #[clap(long = "challenge")]
        challenge: String,

        #[clap(long = "captcha")]
        captcha: String,
    },

    #[clap(name = "version", about = "Show version information")]
    Version,
}

impl Command {
    pub fn name(&self) -> &'static str {
        match self {
            Command::Register { .. } => "register",
            Command::Verify { .. } => "verify",
            Command::Unregister { .. } => "unregister",
            Command::DeleteLocalAccountData { .. } => "deleteLocalAccountData",
            Command::UpdateAccount { .. } => "updateAccount",
            Command::StartChangeNumber { .. } => "startChangeNumber",
            Command::FinishChangeNumber { .. } => "finishChangeNumber",
            Command::UpdateConfiguration { .. } => "updateConfiguration",
            Command::SetPin { .. } => "setPin",
            Command::RemovePin => "removePin",
            Command::Link { .. } => "link",
            Command::AddDevice { .. } => "addDevice",
            Command::ListAccounts => "listAccounts",
            Command::ListDevices => "listDevices",
            Command::RemoveDevice { .. } => "removeDevice",
            Command::Whoami => "whoami",
            Command::ClearSessions { .. } => "clearSessions",
            Command::SendSessionReset { .. } => "sendSessionReset",
            Command::GetUserStatus { .. } => "getUserStatus",
            Command::SendAdminDelete { .. } => "sendAdminDelete",
            Command::Send { .. } => "send",
            Command::SendPollCreate { .. } => "sendPollCreate",
            Command::SendPollVote { .. } => "sendPollVote",
            Command::SendPollTerminate { .. } => "sendPollTerminate",
            Command::SendMessageRequestResponse { .. } => "sendMessageRequestResponse",
            Command::SendPaymentNotification { .. } => "sendPaymentNotification",
            Command::SendPinMessage { .. } => "sendPinMessage",
            Command::SendUnpinMessage { .. } => "sendUnpinMessage",
            Command::SendReaction { .. } => "sendReaction",
            Command::SendReceipt { .. } => "sendReceipt",
            Command::SendTyping { .. } => "sendTyping",
            Command::RemoteDelete { .. } => "remoteDelete",
            Command::Receive { .. } => "receive",
            Command::JoinGroup { .. } => "joinGroup",
            Command::UpdateGroup { .. } => "updateGroup",
            Command::QuitGroup { .. } => "quitGroup",
            Command::ListGroups { .. } => "listGroups",
            Command::ListContacts { .. } => "listContacts",
            Command::ListIdentities { .. } => "listIdentities",
            Command::Trust { .. } => "trust",
            Command::UpdateProfile { .. } => "updateProfile",
            Command::UpdateContact { .. } => "updateContact",
            Command::UpdateDevice { .. } => "updateDevice",
            Command::RemoveContact { .. } => "removeContact",
            Command::Block { .. } => "block",
            Command::Unblock { .. } => "unblock",
            Command::SendContacts => "sendContacts",
            Command::SendSyncRequest => "sendSyncRequest",
            Command::UploadStickerPack { .. } => "uploadStickerPack",
            Command::ListStickerPacks => "listStickerPacks",
            Command::AddStickerPack { .. } => "addStickerPack",
            Command::RemoveStickerPack { .. } => "removeStickerPack",
            Command::GetAttachment { .. } => "getAttachment",
            Command::GetAvatar { .. } => "getAvatar",
            Command::GetSticker { .. } => "getSticker",
            Command::Daemon { .. } => "daemon",
            Command::JsonRpc { .. } => "jsonRpc",
            Command::SubmitRateLimitChallenge { .. } => "submitRateLimitChallenge",
            Command::Version => "version",
        }
    }

    pub fn requires_account(&self) -> bool {
        !matches!(
            self,
            Command::Version
                | Command::ListAccounts
                | Command::Link { .. }
                | Command::Daemon { .. }
                | Command::JsonRpc { .. }
        )
    }
}
