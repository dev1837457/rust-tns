/*
 * This file is part of the Rust TNS modernization and is made available
 * under the Mozilla Public License Version 1.1. See the repository LICENSE
 * file for the complete terms.
 */

//! TI-Nspire compression method 13.
//!
//! A method-13 payload is a fixed-key 3DES-encrypted `TIEN0100` header,
//! followed by a counter-mode XOR transform over a raw DEFLATE stream. The
//! DEFLATE stream contains a TIXC0100 token stream, not readable XML.

use des::cipher::{Block, BlockDecrypt, BlockEncrypt, KeyInit};
use des::TdesEde3;

use crate::compression::{deflate_raw, inflate_raw};
use crate::{decode_tixc_with_limits, Result, TixcLimits, TnsError};

const HEADER_KEY: [u8; 24] = [
    0x79, 0xc4, 0xe0, 0xf4, 0x5e, 0xef, 0x7a, 0x5b, 0x70, 0x13, 0x7a, 0x57, 0xc2, 0xfd, 0x3d, 0x2c,
    0xc2, 0x70, 0x7c, 0xc1, 0xad, 0x2f, 0x15, 0x75,
];

/// Default 21-byte packed 3DES body key material used by Luna/TnsTools.
pub const DEFAULT_BODY_KEY_MATERIAL: [u8; 21] = [
    0x8d, 0x24, 0xef, 0x91, 0x1c, 0x6e, 0xb6, 0x27, 0x02, 0xb5, 0x38, 0xe0, 0x4b, 0x13, 0xe0, 0xe9,
    0xd4, 0xe0, 0x3d, 0x75, 0x16,
];

/// Default little-endian counter seed used by the interoperable encoder.
pub const DEFAULT_COUNTER_SEED: u32 = 0x6d65_7468;

const MAX_METHOD13_DEFLATED: usize = 256 * 1024 * 1024;

/// Controls method-13 encoding parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Method13Options {
    pub seed: u32,
    pub key_material: [u8; 21],
    pub compression_level: u32,
}

impl Default for Method13Options {
    fn default() -> Self {
        Self {
            seed: DEFAULT_COUNTER_SEED,
            key_material: DEFAULT_BODY_KEY_MATERIAL,
            compression_level: 9,
        }
    }
}

/// Decode a method-13 payload to its inflated TIXC stream.
pub fn decode_method13_to_tixc(payload: &[u8]) -> Result<Vec<u8>> {
    if payload.len() < 40 {
        return Err(TnsError::method13(
            "payload is shorter than the 40-byte TIEN0100 header",
        ));
    }

    let mut header = [0u8; 40];
    header.copy_from_slice(&payload[..40]);
    let header_cipher = TdesEde3::new_from_slice(&HEADER_KEY)
        .map_err(|_| TnsError::method13("invalid fixed header key"))?;
    decrypt_blocks(&header_cipher, &mut header);

    if &header[..8] != b"TIEN0100" {
        return Err(TnsError::method13(format!(
            "bad TIEN0100 magic: {:?}",
            &header[..8]
        )));
    }
    let block_size = le_u32(&header[8..12]);
    if block_size != 0x400 {
        return Err(TnsError::method13(format!(
            "unsupported block size 0x{block_size:x}"
        )));
    }
    let seed = le_u32(&header[12..16]);
    let mut key_material = [0u8; 21];
    key_material.copy_from_slice(&header[16..37]);
    if header[37..].iter().any(|byte| *byte != 0) {
        return Err(TnsError::method13(
            "non-zero reserved bytes in TIEN0100 header",
        ));
    }

    let mut encrypted_body = payload[40..].to_vec();
    crypt_body(&mut encrypted_body, seed, &key_material)?;
    inflate_raw(&encrypted_body, MAX_METHOD13_DEFLATED)
}

/// Decode a method-13 payload all the way to readable XML.
pub fn decode_method13_to_xml(payload: &[u8], limits: TixcLimits) -> Result<Vec<u8>> {
    let tixc = decode_method13_to_tixc(payload)?;
    if tixc.starts_with(b"TIXC0100") {
        decode_tixc_with_limits(&tixc, limits)
    } else if tixc.starts_with(b"<?xml") {
        if tixc.len() > limits.max_output_size {
            return Err(TnsError::LimitExceeded {
                kind: "method-13 XML",
                actual: tixc.len() as u64,
                limit: limits.max_output_size as u64,
            });
        }
        Ok(tixc)
    } else {
        Err(TnsError::method13(
            "inflated payload is neither TIXC0100 nor XML",
        ))
    }
}

/// Encode a TIXC stream into an interoperable method-13 payload.
pub fn encode_tixc_to_method13(tixc: &[u8], options: &Method13Options) -> Result<Vec<u8>> {
    if !tixc.starts_with(b"TIXC0100") {
        return Err(TnsError::method13("encoder expects a TIXC0100 stream"));
    }
    if options.compression_level > 9 {
        return Err(TnsError::field(
            "method-13 compression level",
            options.compression_level.to_string(),
        ));
    }

    let deflated = deflate_raw(tixc, options.compression_level)?;
    let mut header = [0u8; 40];
    header[..8].copy_from_slice(b"TIEN0100");
    header[8..12].copy_from_slice(&0x400u32.to_le_bytes());
    header[12..16].copy_from_slice(&options.seed.to_le_bytes());
    header[16..37].copy_from_slice(&options.key_material);

    let header_cipher = TdesEde3::new_from_slice(&HEADER_KEY)
        .map_err(|_| TnsError::method13("invalid fixed header key"))?;
    encrypt_blocks(&header_cipher, &mut header);

    let mut body = deflated;
    crypt_body(&mut body, options.seed, &options.key_material)?;

    let mut result = Vec::with_capacity(header.len() + body.len());
    result.extend_from_slice(&header);
    result.extend_from_slice(&body);
    Ok(result)
}

fn le_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn odd_parity(byte: u8) -> u8 {
    let mut result = byte & 0xfe;
    if result.count_ones().is_multiple_of(2) {
        result |= 1;
    }
    result
}

fn packed_key_to_des_key(key: &[u8]) -> Result<[u8; 8]> {
    if key.len() != 7 {
        return Err(TnsError::method13(
            "packed DES key component must contain 7 bytes",
        ));
    }
    let b = key;
    let raw = [
        b[0] & 0xfe,
        ((b[1] >> 1) & 0x7e) | (b[0] << 7),
        ((b[2] >> 2) & 0x3e) | (b[1] << 6),
        ((b[3] >> 3) & 0x1e) | (b[2] << 5),
        ((b[4] >> 4) & 0x0e) | (b[3] << 4),
        ((b[5] >> 5) & 0x06) | (b[4] << 3),
        ((b[6] >> 6) & 0x02) | (b[5] << 2),
        b[6] << 1,
    ];
    Ok(raw.map(odd_parity))
}

fn body_cipher(key_material: &[u8; 21]) -> Result<TdesEde3> {
    let mut key = [0u8; 24];
    for component in 0..3 {
        let part = packed_key_to_des_key(&key_material[component * 7..component * 7 + 7])?;
        key[component * 8..component * 8 + 8].copy_from_slice(&part);
    }
    TdesEde3::new_from_slice(&key).map_err(|_| TnsError::method13("invalid method-13 body key"))
}

fn crypt_body(data: &mut [u8], seed: u32, key_material: &[u8; 21]) -> Result<()> {
    let cipher = body_cipher(key_material)?;
    for (block_index, chunk) in data.chunks_mut(8).enumerate() {
        let mut counter = [0u8; 8];
        counter[4..].copy_from_slice(
            &seed
                .wrapping_add((block_index as u32) % 0x400)
                .to_le_bytes(),
        );
        cipher.encrypt_block(Block::<TdesEde3>::from_mut_slice(&mut counter));
        for (value, mask) in chunk.iter_mut().zip(counter) {
            *value ^= mask;
        }
    }
    Ok(())
}

fn decrypt_blocks(cipher: &TdesEde3, bytes: &mut [u8; 40]) {
    for block in bytes.chunks_mut(8) {
        cipher.decrypt_block(Block::<TdesEde3>::from_mut_slice(block));
    }
}

fn encrypt_blocks(cipher: &TdesEde3, bytes: &mut [u8; 40]) {
    for block in bytes.chunks_mut(8) {
        cipher.encrypt_block(Block::<TdesEde3>::from_mut_slice(block));
    }
}
