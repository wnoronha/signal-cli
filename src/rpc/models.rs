use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// Accept either `"recipient": "<id>"` or `"recipient": ["<id>", ...]`.
/// Some JSON-RPC clients send the scalar form first.
pub fn string_or_vec<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }
    Ok(match Option::<OneOrMany>::deserialize(deserializer)? {
        None => None,
        Some(OneOrMany::One(s)) => Some(vec![s]),
        Some(OneOrMany::Many(v)) => Some(v),
    })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Value>,
    pub id: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendParams {
    #[serde(default, deserialize_with = "string_or_vec")]
    pub recipient: Option<Vec<String>>,
    pub group_id: Option<String>,
    pub message: Option<String>,
    pub attachments: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProfileParams {
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub about: Option<String>,
    pub about_emoji: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateContactParams {
    pub recipient: String,
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub expiration: Option<i64>,
    pub phone_number: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveDeviceParams {
    pub device_id: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendReceiptParams {
    pub recipient: String,
    pub target_timestamp: i64,
    pub r#type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendTypingParams {
    #[serde(default, deserialize_with = "string_or_vec")]
    pub recipient: Option<Vec<String>>,
    pub group_id: Option<String>,
    pub stop: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUserStatusParams {
    pub recipient: Vec<String>,
    pub username: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendReactionParams {
    pub recipient: Vec<String>,
    pub group_id: Option<String>,
    pub emoji: String,
    pub target_author: String,
    pub target_timestamp: i64,
    pub remove: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDeleteParams {
    pub recipient: Vec<String>,
    pub group_id: Option<String>,
    pub target_timestamp: i64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListIdentitiesParams {
    pub recipient: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustParams {
    pub recipient: String,
    pub trust_level: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveContactParams {
    pub recipient: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockParams {
    pub recipient: Vec<String>,
    pub group_id: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendPollCreateParams {
    pub recipient: Vec<String>,
    pub group_id: Option<String>,
    pub question: String,
    pub options: Vec<String>,
    pub multiple_answers: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendPollVoteParams {
    pub recipient: Vec<String>,
    pub group_id: Option<String>,
    pub target_timestamp: i64,
    pub choice: Vec<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendPollTerminateParams {
    pub recipient: Vec<String>,
    pub group_id: Option<String>,
    pub target_timestamp: i64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListContactsParams {
    pub name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddStickerPackParams {
    pub pack_id: String,
    pub pack_key: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetStickerParams {
    pub pack_id: String,
    pub sticker_id: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadStickerPackParams {
    pub manifest: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAttachmentParams {
    pub id: String,
    pub recipient: Option<String>,
    pub group_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAvatarParams {
    pub contact: Option<String>,
    pub profile: Option<String>,
    pub group_id: Option<String>,
}
