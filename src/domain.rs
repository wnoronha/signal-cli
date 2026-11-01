//! Branded domain types parsed once at CLI/RPC boundaries.
//!
//! `AccountId` is a validated `+E.164` string. `GroupKey` is a 32-byte
//! group master key parsed from base64 or hex. Both replace raw `String`
//! plumbing through `core_*` functions.

use crate::errors::SignalError;

/// Validated `+E.164`-ish account name. Construction validates.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AccountId(String);

impl AccountId {
    pub fn parse(s: &str) -> Result<Self, SignalError> {
        crate::utils::validate_account_name(s)?;
        Ok(Self(s.to_string()))
    }

    /// Trusts the caller (account lists, daemon defaults). Validation lives
    /// at the boundary; internal code trusts the type.
    pub fn from_validated(s: String) -> Self {
        Self(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AccountId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<AccountId> for String {
    fn from(id: AccountId) -> String {
        id.0
    }
}

/// 32-byte group master key. Parsed once at the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GroupKey([u8; 32]);

impl GroupKey {
    pub fn parse(s: &str) -> anyhow::Result<Self> {
        Ok(Self(crate::commands::parse_group_id(s)?))
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn encode_b64(&self) -> String {
        use base64::prelude::*;
        BASE64_STANDARD.encode(self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_id_rejects_traversal() {
        assert!(AccountId::parse("+15551234567").is_ok());
        assert!(AccountId::parse("+../../evil").is_err());
        assert!(AccountId::parse("1555").is_err());
    }

    #[test]
    fn group_key_round_trip() {
        let raw = [7u8; 32];
        let key = GroupKey::from_bytes(raw);
        let b64 = key.encode_b64();
        assert_eq!(GroupKey::parse(&b64).unwrap(), key);
        assert!(GroupKey::parse("abcd").is_err());
    }
}
