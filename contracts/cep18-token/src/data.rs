extern crate alloc;

use alloc::string::String;
use alloc::string::ToString;
use casper_contract::{
    contract_api::{runtime, storage},
    unwrap_or_revert::UnwrapOrRevert,
};
use casper_types::{bytesrepr::{FromBytes, ToBytes}, CLTyped, Key, URef};

// Storage keys
pub const NAME: &str = "name";
pub const SYMBOL: &str = "symbol";
pub const DECIMALS: &str = "decimals";
pub const TOTAL_SUPPLY: &str = "total_supply";
pub const BALANCES: &str = "balances";
pub const ALLOWANCES: &str = "allowances";
pub const ADMIN: &str = "admin";
pub const CONTRACT_HASH: &str = "cep18_token_contract";

/// Read a value from a named key
pub fn read_named_key<T: CLTyped + FromBytes>(name: &str) -> T {
    let key = runtime::get_key(name).unwrap_or_revert();
    let uref = key.into_uref().unwrap_or_revert();
    storage::read(uref).unwrap_or_revert().unwrap_or_revert()
}

/// Write a value to a named key
pub fn write_named_key<T: CLTyped + casper_types::bytesrepr::ToBytes>(name: &str, value: T) {
    let key = runtime::get_key(name).unwrap_or_revert();
    let uref = key.into_uref().unwrap_or_revert();
    storage::write(uref, value);
}

/// Get the URef for a dictionary
pub fn get_dictionary_uref(name: &str) -> URef {
    let key = runtime::get_key(name).unwrap_or_revert();
    key.into_uref().unwrap_or_revert()
}

/// Convert a Key to a string for dictionary lookups
/// This uses the hex representation which is directly queryable via RPC
pub fn key_to_str(key: &Key) -> String {
    match key {
        Key::Account(account_hash) => {
            // Use hex encoding for account hash
            let bytes = account_hash.as_bytes();
            hex_encode(bytes)
        }
        Key::Hash(hash) => {
            // Use hex encoding for contract hash
            hex_encode(hash)
        }
        _ => {
            // For other key types, use base64
            base64_encode(&key.to_bytes().unwrap_or_revert())
        }
    }
}

/// Simple hex encoding
fn hex_encode(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(hex_char(byte >> 4));
        result.push(hex_char(byte & 0x0f));
    }
    result
}

fn hex_char(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'a' + nibble - 10) as char,
        _ => '0',
    }
}

/// Simple base64 encoding (fallback for non-standard keys)
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();

    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

        result.push(ALPHABET[b0 >> 2] as char);
        result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

        if chunk.len() > 1 {
            result.push(ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(ALPHABET[b2 & 0x3f] as char);
        } else {
            result.push('=');
        }
    }

    result
}

/// Create a combined key for allowances (owner + spender)
pub fn allowance_key(owner: &Key, spender: &Key) -> String {
    let mut key = key_to_str(owner);
    key.push('_');
    key.push_str(&key_to_str(spender));
    key
}
