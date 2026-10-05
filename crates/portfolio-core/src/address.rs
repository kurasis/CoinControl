//! Local address validation and normalization (SPECIFICATION.md §4).
//!
//! Every account is identified by `(network_id, canonical_address)`. The
//! canonical form is what the database compares; the display form is what the
//! user sees. Validation is fully local and never queries a provider.

use std::str::FromStr;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};

use crate::error::CoreError;
use crate::network::{NetworkFamily, NetworkId};

/// Result of validating one public address for one network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct NormalizedAddress {
    pub network: NetworkId,
    /// Representation used for identity comparison and storage uniqueness.
    pub canonical: String,
    /// Representation shown to the user (e.g. EIP-55 checksummed for EVM).
    pub display: String,
}

/// Validates `input` for `network` and returns its canonical and display forms.
pub fn normalize_address(network: NetworkId, input: &str) -> Result<NormalizedAddress, CoreError> {
    let input = input.trim();
    let (canonical, display) = match network.family() {
        NetworkFamily::Evm => normalize_evm(input)?,
        NetworkFamily::Bitcoin => normalize_bitcoin(input)?,
        NetworkFamily::Solana => normalize_solana(input)?,
        NetworkFamily::Tron => normalize_tron(input)?,
        NetworkFamily::Ton => normalize_ton(input)?,
    };
    Ok(NormalizedAddress {
        network,
        canonical,
        display,
    })
}

fn invalid(network: &'static str, reason: impl Into<String>) -> CoreError {
    CoreError::InvalidAddress {
        network,
        reason: reason.into(),
    }
}

// ---------------------------------------------------------------- EVM

/// Canonical form: lowercase `0x` hex. Display: EIP-55 checksum.
fn normalize_evm(input: &str) -> Result<(String, String), CoreError> {
    let hex_part = input
        .strip_prefix("0x")
        .or_else(|| input.strip_prefix("0X"))
        .ok_or_else(|| invalid("EVM", "address must start with 0x"))?;
    if hex_part.len() != 40 || !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(invalid(
            "EVM",
            "address must contain 40 hexadecimal characters",
        ));
    }
    let lower = hex_part.to_ascii_lowercase();
    let checksummed = eip55(&lower);
    let has_lower = hex_part.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = hex_part.chars().any(|c| c.is_ascii_uppercase());
    if has_lower && has_upper && checksummed[2..] != *hex_part {
        return Err(invalid(
            "EVM",
            "mixed-case address has an invalid EIP-55 checksum",
        ));
    }
    Ok((format!("0x{lower}"), checksummed))
}

fn eip55(lower_hex: &str) -> String {
    let hash = Keccak256::digest(lower_hex.as_bytes());
    let mut out = String::with_capacity(42);
    out.push_str("0x");
    for (i, c) in lower_hex.chars().enumerate() {
        let nibble = (hash[i / 2] >> (if i % 2 == 0 { 4 } else { 0 })) & 0x0f;
        if c.is_ascii_alphabetic() && nibble >= 8 {
            out.push(c.to_ascii_uppercase());
        } else {
            out.push(c);
        }
    }
    out
}

// ---------------------------------------------------------------- Bitcoin

/// Validated with the `bitcoin` crate. Bech32 forms are canonicalized to lowercase.
fn normalize_bitcoin(input: &str) -> Result<(String, String), CoreError> {
    let unchecked = bitcoin::Address::from_str(input)
        .map_err(|e| invalid("Bitcoin", format!("not a valid address ({e})")))?;
    let address = unchecked
        .require_network(bitcoin::Network::Bitcoin)
        .map_err(|_| invalid("Bitcoin", "address is not for Bitcoin mainnet"))?;
    let canonical = address.to_string();
    Ok((canonical.clone(), canonical))
}

// ---------------------------------------------------------------- Solana

/// Base58 of a 32-byte public key. Case-sensitive; stored exactly as validated.
fn normalize_solana(input: &str) -> Result<(String, String), CoreError> {
    let bytes = bs58::decode(input)
        .into_vec()
        .map_err(|_| invalid("Solana", "address is not valid base58"))?;
    if bytes.len() != 32 {
        return Err(invalid("Solana", "address must decode to 32 bytes"));
    }
    let canonical = bs58::encode(bytes).into_string();
    Ok((canonical.clone(), canonical))
}

// ---------------------------------------------------------------- TRON

const TRON_PREFIX: u8 = 0x41;

/// Accepts Base58Check (`T...`) or 21-byte hex (`41...`). Canonical form is Base58Check.
fn normalize_tron(input: &str) -> Result<(String, String), CoreError> {
    let bytes = if input.len() == 42 && input.chars().all(|c| c.is_ascii_hexdigit()) {
        hex::decode(input).map_err(|_| invalid("TRON", "invalid hex address"))?
    } else {
        bs58::decode(input)
            .with_check(None)
            .into_vec()
            .map_err(|_| invalid("TRON", "invalid Base58Check address or checksum"))?
    };
    if bytes.len() != 21 || bytes[0] != TRON_PREFIX {
        return Err(invalid(
            "TRON",
            "address must be 21 bytes with mainnet prefix 0x41",
        ));
    }
    let canonical = bs58::encode(bytes).with_check().into_string();
    Ok((canonical.clone(), canonical))
}

/// Hex form (`41...`) of a TRON address, as used by some provider endpoints.
pub fn tron_to_hex(canonical: &str) -> Result<String, CoreError> {
    let bytes = bs58::decode(canonical)
        .with_check(None)
        .into_vec()
        .map_err(|_| invalid("TRON", "invalid Base58Check address or checksum"))?;
    Ok(hex::encode(bytes))
}

// ---------------------------------------------------------------- TON

const TON_TAG_BOUNCEABLE: u8 = 0x11;
const TON_TAG_NON_BOUNCEABLE: u8 = 0x51;
const TON_TAG_TESTNET: u8 = 0x80;
const CRC16_XMODEM: crc::Crc<u16> = crc::Crc::<u16>::new(&crc::CRC_16_XMODEM);

/// Accepts raw (`0:<hex>`) or user-friendly base64/base64url forms. The
/// canonical form is the raw `workchain:account_id` with lowercase hex, so the
/// bounceable and non-bounceable representations of one account compare equal.
fn normalize_ton(input: &str) -> Result<(String, String), CoreError> {
    if let Some((wc, hash)) = input.split_once(':') {
        let workchain: i8 = wc
            .parse()
            .map_err(|_| invalid("TON", "raw address has an invalid workchain"))?;
        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(invalid(
                "TON",
                "raw address must have a 64-character hex account ID",
            ));
        }
        let canonical = format!("{workchain}:{}", hash.to_ascii_lowercase());
        return Ok((canonical.clone(), canonical));
    }
    if input.len() != 48 {
        return Err(invalid(
            "TON",
            "user-friendly address must be 48 characters",
        ));
    }
    let normalized_b64: String = input
        .chars()
        .map(|c| match c {
            '-' => '+',
            '_' => '/',
            other => other,
        })
        .collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(normalized_b64)
        .map_err(|_| invalid("TON", "user-friendly address is not valid base64"))?;
    if bytes.len() != 36 {
        return Err(invalid(
            "TON",
            "user-friendly address must decode to 36 bytes",
        ));
    }
    let expected_crc = u16::from_be_bytes([bytes[34], bytes[35]]);
    if CRC16_XMODEM.checksum(&bytes[..34]) != expected_crc {
        return Err(invalid("TON", "user-friendly address checksum mismatch"));
    }
    let tag = bytes[0];
    if tag & TON_TAG_TESTNET != 0 {
        return Err(invalid(
            "TON",
            "testnet address cannot be added to a mainnet portfolio",
        ));
    }
    if tag != TON_TAG_BOUNCEABLE && tag != TON_TAG_NON_BOUNCEABLE {
        return Err(invalid("TON", "unknown address flags"));
    }
    let workchain = bytes[1] as i8;
    let canonical = format!("{workchain}:{}", hex::encode(&bytes[2..34]));
    Ok((canonical, input.to_owned()))
}

/// Encodes a canonical raw TON address in user-friendly url-safe form.
pub fn ton_to_friendly(canonical: &str, bounceable: bool) -> Result<String, CoreError> {
    let (wc, hash) = canonical
        .split_once(':')
        .ok_or_else(|| invalid("TON", "expected raw workchain:account form"))?;
    let workchain: i8 = wc
        .parse()
        .map_err(|_| invalid("TON", "raw address has an invalid workchain"))?;
    let account = hex::decode(hash).map_err(|_| invalid("TON", "invalid account hex"))?;
    if account.len() != 32 {
        return Err(invalid("TON", "account ID must be 32 bytes"));
    }
    let mut bytes = Vec::with_capacity(36);
    bytes.push(if bounceable {
        TON_TAG_BOUNCEABLE
    } else {
        TON_TAG_NON_BOUNCEABLE
    });
    bytes.push(workchain as u8);
    bytes.extend_from_slice(&account);
    let crc = CRC16_XMODEM.checksum(&bytes);
    bytes.extend_from_slice(&crc.to_be_bytes());
    Ok(base64::engine::general_purpose::URL_SAFE.encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(network: NetworkId, input: &str) -> NormalizedAddress {
        normalize_address(network, input).unwrap_or_else(|e| panic!("{input}: {e}"))
    }

    #[test]
    fn evm_eip55_vectors() {
        // Test vectors from EIP-55.
        for v in [
            "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed",
            "0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359",
            "0xdbF03B407c01E7cD3CBea99509d93f8DDDC8C6FB",
            "0xD1220A0cf47c7B9Be7A2E6BA89F429762e7b9aDb",
        ] {
            let n = ok(NetworkId::Ethereum, v);
            assert_eq!(n.display, v);
            assert_eq!(n.canonical, v.to_ascii_lowercase());
        }
    }

    #[test]
    fn evm_casing_does_not_change_identity_but_bad_checksum_is_rejected() {
        let lower = ok(
            NetworkId::Base,
            "0x5aaeb6053f3e94c9b9a09f33669435e7ef1beaed",
        );
        let upper = ok(
            NetworkId::Base,
            "0x5AAEB6053F3E94C9B9A09F33669435E7EF1BEAED",
        );
        assert_eq!(lower.canonical, upper.canonical);
        assert_eq!(lower.display, "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed");
        assert!(
            normalize_address(
                NetworkId::Ethereum,
                "0x5AAeb6053F3E94C9b9A09f33669435E7Ef1BeAed"
            )
            .is_err()
        );
        assert!(normalize_address(NetworkId::Ethereum, "0x1234").is_err());
        assert!(
            normalize_address(
                NetworkId::Ethereum,
                "5aaeb6053f3e94c9b9a09f33669435e7ef1beaed"
            )
            .is_err()
        );
    }

    #[test]
    fn bitcoin_mainnet_only() {
        ok(NetworkId::Bitcoin, "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa");
        let segwit = ok(
            NetworkId::Bitcoin,
            "BC1QW508D6QEJXTDG4Y5R3ZARVARY0C5XW7KV8F3T4",
        );
        assert_eq!(
            segwit.canonical,
            "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4"
        );
        assert!(
            normalize_address(
                NetworkId::Bitcoin,
                "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx"
            )
            .is_err()
        );
        assert!(
            normalize_address(NetworkId::Bitcoin, "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNb").is_err()
        );
    }

    #[test]
    fn solana_is_case_sensitive() {
        let system = ok(NetworkId::Solana, "11111111111111111111111111111111");
        assert_eq!(system.canonical, "11111111111111111111111111111111");
        let token = ok(
            NetworkId::Solana,
            "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
        );
        let swapped = normalize_address(
            NetworkId::Solana,
            "tOKENKEGqFEzYInWajBnbgkpfxcwUbVF9sS623vq5da",
        );
        assert!(
            swapped
                .map(|s| s.canonical != token.canonical)
                .unwrap_or(true)
        );
        assert!(normalize_address(NetworkId::Solana, "0OIl").is_err());
    }

    #[test]
    fn tron_base58_and_hex_are_equivalent() {
        let b58 = ok(NetworkId::Tron, "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t");
        let hex_form = tron_to_hex(&b58.canonical).unwrap();
        assert_eq!(hex_form, "41a614f803b6fd780986a42c78ec9c7f77e6ded13c");
        let from_hex = ok(NetworkId::Tron, &hex_form);
        assert_eq!(from_hex.canonical, b58.canonical);
        assert!(normalize_address(NetworkId::Tron, "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6u").is_err());
    }

    #[test]
    fn ton_friendly_and_raw_forms_are_equivalent() {
        let raw = "0:83dfd552e63729b472fcbcc8c45ebcc6691702558b68ec7527e1ba403a0f31a8";
        let bounceable = ton_to_friendly(raw, true).unwrap();
        let non_bounceable = ton_to_friendly(raw, false).unwrap();
        assert_ne!(bounceable, non_bounceable);
        for form in [raw, bounceable.as_str(), non_bounceable.as_str()] {
            assert_eq!(ok(NetworkId::Ton, form).canonical, raw);
        }
        assert_eq!(
            ok(
                NetworkId::Ton,
                "EQCD39VS5jcptHL8vMjEXrzGaRcCVYto7HUn4bpAOg8xqB2N"
            )
            .canonical,
            raw
        );
        // Corrupt one character: checksum must fail.
        let mut corrupted = bounceable.clone();
        corrupted.replace_range(10..11, if &bounceable[10..11] == "A" { "B" } else { "A" });
        assert!(normalize_address(NetworkId::Ton, &corrupted).is_err());
    }

    #[test]
    fn ton_testnet_flag_is_rejected() {
        let raw = "0:83dfd552e63729b472fcbcc8c45ebcc6691702558b68ec7527e1ba403a0f31a8";
        let mut bytes = base64::engine::general_purpose::URL_SAFE
            .decode(ton_to_friendly(raw, true).unwrap())
            .unwrap();
        bytes[0] |= TON_TAG_TESTNET;
        let crc = CRC16_XMODEM.checksum(&bytes[..34]);
        bytes[34..36].copy_from_slice(&crc.to_be_bytes());
        let testnet = base64::engine::general_purpose::URL_SAFE.encode(bytes);
        assert!(normalize_address(NetworkId::Ton, &testnet).is_err());
    }
}
