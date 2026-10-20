use anyhow::bail;
use base64::prelude::*;

pub(crate) fn encode_group_id_b64(key: &[u8; 32]) -> String {
    BASE64_STANDARD.encode(key)
}

pub(crate) fn parse_group_id(id_str: &str) -> anyhow::Result<[u8; 32]> {
    if let Ok(bytes) = BASE64_STANDARD.decode(id_str)
        && bytes.len() == 32
    {
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        return Ok(key);
    }
    if let Ok(bytes) = hex::decode(id_str)
        && bytes.len() == 32
    {
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        return Ok(key);
    }
    bail!("Invalid group ID. Must be 32-byte hex or base64 encoded string.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_group_id_hex() {
        let hex_id = "00".repeat(32);
        let parsed = parse_group_id(&hex_id).unwrap();
        assert_eq!(parsed, [0u8; 32]);
    }

    #[test]
    fn parse_group_id_base64() {
        let raw = [1u8; 32];
        let b64 = BASE64_STANDARD.encode(raw);
        assert_eq!(parse_group_id(&b64).unwrap(), raw);
    }

    #[test]
    fn parse_group_id_rejects_short_hex() {
        assert!(parse_group_id("abcd").is_err());
    }

    #[test]
    fn encode_group_id_round_trip() {
        let raw = [2u8; 32];
        let b64 = encode_group_id_b64(&raw);
        assert_eq!(parse_group_id(&b64).unwrap(), raw);
    }
}
