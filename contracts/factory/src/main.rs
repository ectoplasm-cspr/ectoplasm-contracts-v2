#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec;
use casper_contract::{
    contract_api::{runtime, storage},
    unwrap_or_revert::UnwrapOrRevert,
};
use casper_types::{
    addressable_entity::{EntityEntryPoint as EntryPoint, EntryPoints},
    bytesrepr::{FromBytes, ToBytes},
    contracts::NamedKeys,
    runtime_args, AddressableEntityHash, CLType, CLTyped, CLValue, EntryPointAccess,
    EntryPointPayment, EntryPointType, Key, Parameter, RuntimeArgs, URef,
};

// Storage keys
const FEE_TO: &str = "fee_to";
const FEE_TO_SETTER: &str = "fee_to_setter";
const PAIRS: &str = "pairs";
const ALL_PAIRS: &str = "all_pairs";
const ALL_PAIRS_LENGTH: &str = "all_pairs_length";

// Error codes
const ERROR_UNAUTHORIZED: u16 = 1;
const ERROR_PAIR_EXISTS: u16 = 2;
const ERROR_IDENTICAL_ADDRESSES: u16 = 3;
const ERROR_ALREADY_INITIALIZED: u16 = 4;
const ERROR_INDEX_OUT_OF_BOUNDS: u16 = 5;
const ERROR_FAILED_TO_CREATE_DICTIONARY: u16 = 6;

// ============ Helper Functions ============

fn read_from_uref<T: CLTyped + FromBytes>(name: &str) -> T {
    let key = runtime::get_key(name).unwrap_or_revert();
    let uref = key.into_uref().unwrap_or_revert();
    storage::read(uref).unwrap_or_revert().unwrap_or_revert()
}

fn write_to_uref<T: CLTyped + ToBytes>(name: &str, value: T) {
    let key = runtime::get_key(name).unwrap_or_revert();
    let uref = key.into_uref().unwrap_or_revert();
    storage::write(uref, value);
}

fn key_to_str(key: &Key) -> String {
    match key {
        Key::Account(account_hash) => hex_encode(account_hash.as_bytes()),
        Key::Hash(hash) => hex_encode(hash),
        _ => hex_encode(&key.to_bytes().unwrap_or_revert()),
    }
}

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

/// Get raw bytes from Key
fn key_to_bytes(key: &Key) -> [u8; 32] {
    let mut result = [0u8; 32];
    match key {
        Key::Account(account_hash) => {
            let bytes = account_hash.as_bytes();
            let len = bytes.len().min(32);
            result[..len].copy_from_slice(&bytes[..len]);
        }
        Key::Hash(hash) => {
            result = *hash;
        }
        _ => {
            let bytes = key.to_bytes().unwrap_or_revert();
            let len = bytes.len().min(32);
            result[..len].copy_from_slice(&bytes[..len]);
        }
    }
    result
}

/// Create pair key from two tokens using XOR (order-independent, fixed 64 chars)
fn pair_key(token_a: &Key, token_b: &Key) -> String {
    let bytes_a = key_to_bytes(token_a);
    let bytes_b = key_to_bytes(token_b);

    // XOR produces order-independent result
    let mut xored = [0u8; 32];
    for i in 0..32 {
        xored[i] = bytes_a[i] ^ bytes_b[i];
    }

    hex_encode(&xored)
}

/// Sort two tokens
fn sort_tokens(token_a: Key, token_b: Key) -> (Key, Key) {
    let key_a = key_to_str(&token_a);
    let key_b = key_to_str(&token_b);

    if key_a < key_b {
        (token_a, token_b)
    } else {
        (token_b, token_a)
    }
}

fn get_dictionary_uref(name: &str) -> URef {
    runtime::get_key(name)
        .unwrap_or_revert()
        .into_uref()
        .unwrap_or_revert()
}

fn read_pair(token_a: &Key, token_b: &Key) -> Option<Key> {
    let key = pair_key(token_a, token_b);
    let dict_uref = get_dictionary_uref(PAIRS);
    storage::dictionary_get(dict_uref, &key).unwrap_or_default()
}

fn write_pair(token_a: &Key, token_b: &Key, pair: Key) {
    let key = pair_key(token_a, token_b);
    let dict_uref = get_dictionary_uref(PAIRS);
    storage::dictionary_put(dict_uref, &key, pair);
}

fn read_all_pairs_at(index: u64) -> Option<Key> {
    let dict_uref = get_dictionary_uref(ALL_PAIRS);
    storage::dictionary_get(dict_uref, &index.to_string()).unwrap_or_default()
}

fn write_all_pairs_at(index: u64, pair: Key) {
    let dict_uref = get_dictionary_uref(ALL_PAIRS);
    storage::dictionary_put(dict_uref, &index.to_string(), pair);
}

// ============ Entry Points ============

/// Initialize dictionaries. Called after contract creation.
#[no_mangle]
pub extern "C" fn init() {
    // Check if already initialized (PAIRS dictionary exists)
    if runtime::get_key(PAIRS).is_some() {
        runtime::revert(casper_types::ApiError::User(ERROR_ALREADY_INITIALIZED));
    }

    // Create dictionaries in contract context
    storage::new_dictionary(PAIRS)
        .unwrap_or_revert_with(casper_types::ApiError::User(ERROR_FAILED_TO_CREATE_DICTIONARY));
    storage::new_dictionary(ALL_PAIRS)
        .unwrap_or_revert_with(casper_types::ApiError::User(ERROR_FAILED_TO_CREATE_DICTIONARY));
}

#[no_mangle]
pub extern "C" fn fee_to() {
    let fee_to: Key = read_from_uref(FEE_TO);
    runtime::ret(CLValue::from_t(fee_to).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn fee_to_setter() {
    let setter: Key = read_from_uref(FEE_TO_SETTER);
    runtime::ret(CLValue::from_t(setter).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn all_pairs_length() {
    let length: u64 = read_from_uref(ALL_PAIRS_LENGTH);
    runtime::ret(CLValue::from_t(length).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn get_pair() {
    let token_a: Key = runtime::get_named_arg("token_a");
    let token_b: Key = runtime::get_named_arg("token_b");

    let pair = read_pair(&token_a, &token_b);
    runtime::ret(CLValue::from_t(pair).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn all_pairs() {
    let index: u64 = runtime::get_named_arg("index");
    let length: u64 = read_from_uref(ALL_PAIRS_LENGTH);

    if index >= length {
        runtime::revert(casper_types::ApiError::User(ERROR_INDEX_OUT_OF_BOUNDS));
    }

    let pair = read_all_pairs_at(index);
    runtime::ret(CLValue::from_t(pair).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn create_pair() {
    let token_a: Key = runtime::get_named_arg("token_a");
    let token_b: Key = runtime::get_named_arg("token_b");
    let pair_contract: Key = runtime::get_named_arg("pair");

    // Validate tokens are different
    if key_to_str(&token_a) == key_to_str(&token_b) {
        runtime::revert(casper_types::ApiError::User(ERROR_IDENTICAL_ADDRESSES));
    }

    // Sort tokens
    let (token0, token1) = sort_tokens(token_a, token_b);

    // Check pair doesn't exist
    if read_pair(&token0, &token1).is_some() {
        runtime::revert(casper_types::ApiError::User(ERROR_PAIR_EXISTS));
    }

    // Store pair
    write_pair(&token0, &token1, pair_contract);

    // Add to all_pairs array
    let length: u64 = read_from_uref(ALL_PAIRS_LENGTH);
    write_all_pairs_at(length, pair_contract);
    write_to_uref(ALL_PAIRS_LENGTH, length + 1);

    // Return pair address
    runtime::ret(CLValue::from_t(pair_contract).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn set_fee_to() {
    let caller = Key::Account(runtime::get_caller());
    let setter: Key = read_from_uref(FEE_TO_SETTER);

    if caller != setter {
        runtime::revert(casper_types::ApiError::User(ERROR_UNAUTHORIZED));
    }

    let new_fee_to: Key = runtime::get_named_arg("fee_to");
    write_to_uref(FEE_TO, new_fee_to);
}

#[no_mangle]
pub extern "C" fn set_fee_to_setter() {
    let caller = Key::Account(runtime::get_caller());
    let setter: Key = read_from_uref(FEE_TO_SETTER);

    if caller != setter {
        runtime::revert(casper_types::ApiError::User(ERROR_UNAUTHORIZED));
    }

    let new_setter: Key = runtime::get_named_arg("fee_to_setter");
    write_to_uref(FEE_TO_SETTER, new_setter);
}

// ============ Contract Installation ============

fn get_entry_points() -> EntryPoints {
    let mut entry_points = EntryPoints::new();

    entry_points.add_entry_point(EntryPoint::new(
        "init", vec![], CLType::Unit,
        EntryPointAccess::Public, EntryPointType::Called, EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "fee_to", vec![], CLType::Key,
        EntryPointAccess::Public, EntryPointType::Called, EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "fee_to_setter", vec![], CLType::Key,
        EntryPointAccess::Public, EntryPointType::Called, EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "all_pairs_length", vec![], CLType::U64,
        EntryPointAccess::Public, EntryPointType::Called, EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "get_pair",
        vec![
            Parameter::new("token_a", CLType::Key),
            Parameter::new("token_b", CLType::Key),
        ],
        CLType::Option(Box::new(CLType::Key)),
        EntryPointAccess::Public, EntryPointType::Called, EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "all_pairs",
        vec![Parameter::new("index", CLType::U64)],
        CLType::Option(Box::new(CLType::Key)),
        EntryPointAccess::Public, EntryPointType::Called, EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "create_pair",
        vec![
            Parameter::new("token_a", CLType::Key),
            Parameter::new("token_b", CLType::Key),
            Parameter::new("pair", CLType::Key),
        ],
        CLType::Key,
        EntryPointAccess::Public, EntryPointType::Called, EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "set_fee_to",
        vec![Parameter::new("fee_to", CLType::Key)],
        CLType::Unit,
        EntryPointAccess::Public, EntryPointType::Called, EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "set_fee_to_setter",
        vec![Parameter::new("fee_to_setter", CLType::Key)],
        CLType::Unit,
        EntryPointAccess::Public, EntryPointType::Called, EntryPointPayment::Caller,
    ));

    entry_points
}

#[no_mangle]
pub extern "C" fn call() {
    let mut named_keys = NamedKeys::new();

    let deployer = Key::Account(runtime::get_caller());

    // Initialize fee_to to deployer (can be changed later)
    named_keys.insert(FEE_TO.to_string(), storage::new_uref(deployer).into());
    named_keys.insert(FEE_TO_SETTER.to_string(), storage::new_uref(deployer).into());
    named_keys.insert(ALL_PAIRS_LENGTH.to_string(), storage::new_uref(0u64).into());

    let (contract_hash, _) = storage::new_contract(
        get_entry_points(),
        Some(named_keys),
        Some("ectoplasm_factory_package".to_string()),
        Some("ectoplasm_factory_access".to_string()),
        None,
    );

    runtime::put_key("ectoplasm_factory_contract", contract_hash.into());

    // Call init to create dictionaries in contract context
    runtime::call_contract::<()>(contract_hash, "init", runtime_args! {});
}
