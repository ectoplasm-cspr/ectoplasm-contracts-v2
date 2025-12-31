#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec;
use casper_contract::{
    contract_api::{runtime, storage},
    unwrap_or_revert::UnwrapOrRevert,
};
use casper_types::{
    bytesrepr::{FromBytes, ToBytes},
    contracts::NamedKeys,
    CLType, CLTyped, CLValue, EntryPoint, EntryPointAccess, EntryPointType,
    EntryPoints, Key, Parameter, URef, U256,
};

// Storage keys
const NAME: &str = "name";
const SYMBOL: &str = "symbol";
const DECIMALS: &str = "decimals";
const TOTAL_SUPPLY: &str = "total_supply";
const BALANCES: &str = "balances";
const ALLOWANCES: &str = "allowances";
const ADMIN: &str = "admin";

// Token constants
const TOKEN_NAME: &str = "Ectoplasm Token";
const TOKEN_SYMBOL: &str = "ECTO";
const TOKEN_DECIMALS: u8 = 18;

// Error codes
const ERROR_INSUFFICIENT_BALANCE: u16 = 1;
const ERROR_INSUFFICIENT_ALLOWANCE: u16 = 2;
const ERROR_UNAUTHORIZED: u16 = 3;

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

fn get_dictionary_uref(name: &str) -> URef {
    let key = runtime::get_key(name).unwrap_or_revert();
    key.into_uref().unwrap_or_revert()
}

fn key_to_str(key: &Key) -> String {
    match key {
        Key::Account(account_hash) => {
            let bytes = account_hash.as_bytes();
            hex_encode(bytes)
        }
        Key::Hash(hash) => hex_encode(hash),
        _ => {
            // Fallback: use debug format
            let bytes = key.to_bytes().unwrap_or_revert();
            hex_encode(&bytes)
        }
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

fn allowance_key(owner: &Key, spender: &Key) -> String {
    let mut key = key_to_str(owner);
    key.push('_');
    key.push_str(&key_to_str(spender));
    key
}

fn read_balance(owner: &Key) -> U256 {
    let dict_uref = get_dictionary_uref(BALANCES);
    let key = key_to_str(owner);
    storage::dictionary_get(dict_uref, &key)
        .unwrap_or_default()
        .unwrap_or_default()
}

fn write_balance(owner: &Key, amount: U256) {
    let dict_uref = get_dictionary_uref(BALANCES);
    let key = key_to_str(owner);
    storage::dictionary_put(dict_uref, &key, amount);
}

fn read_allowance(owner: &Key, spender: &Key) -> U256 {
    let dict_uref = get_dictionary_uref(ALLOWANCES);
    let key = allowance_key(owner, spender);
    storage::dictionary_get(dict_uref, &key)
        .unwrap_or_default()
        .unwrap_or_default()
}

fn write_allowance(owner: &Key, spender: &Key, amount: U256) {
    let dict_uref = get_dictionary_uref(ALLOWANCES);
    let key = allowance_key(owner, spender);
    storage::dictionary_put(dict_uref, &key, amount);
}

fn transfer_internal(sender: &Key, recipient: &Key, amount: U256) {
    let sender_balance = read_balance(sender);
    if sender_balance < amount {
        runtime::revert(casper_types::ApiError::User(ERROR_INSUFFICIENT_BALANCE));
    }
    write_balance(sender, sender_balance - amount);
    let recipient_balance = read_balance(recipient);
    write_balance(recipient, recipient_balance + amount);
}

// ============ Entry Points ============

#[no_mangle]
pub extern "C" fn name() {
    let name: String = read_from_uref(NAME);
    runtime::ret(CLValue::from_t(name).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn symbol() {
    let symbol: String = read_from_uref(SYMBOL);
    runtime::ret(CLValue::from_t(symbol).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn decimals() {
    let decimals: u8 = read_from_uref(DECIMALS);
    runtime::ret(CLValue::from_t(decimals).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn total_supply() {
    let total_supply: U256 = read_from_uref(TOTAL_SUPPLY);
    runtime::ret(CLValue::from_t(total_supply).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn balance_of() {
    let owner: Key = runtime::get_named_arg("owner");
    let balance = read_balance(&owner);
    runtime::ret(CLValue::from_t(balance).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn allowance() {
    let owner: Key = runtime::get_named_arg("owner");
    let spender: Key = runtime::get_named_arg("spender");
    let allowance = read_allowance(&owner, &spender);
    runtime::ret(CLValue::from_t(allowance).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn transfer() {
    let recipient: Key = runtime::get_named_arg("recipient");
    let amount: U256 = runtime::get_named_arg("amount");
    let caller = runtime::get_caller();
    let sender = Key::Account(caller);
    transfer_internal(&sender, &recipient, amount);
}

#[no_mangle]
pub extern "C" fn transfer_from() {
    let owner: Key = runtime::get_named_arg("owner");
    let recipient: Key = runtime::get_named_arg("recipient");
    let amount: U256 = runtime::get_named_arg("amount");

    let caller = runtime::get_caller();
    let spender = Key::Account(caller);

    let current_allowance = read_allowance(&owner, &spender);
    if current_allowance < amount {
        runtime::revert(casper_types::ApiError::User(ERROR_INSUFFICIENT_ALLOWANCE));
    }

    write_allowance(&owner, &spender, current_allowance - amount);
    transfer_internal(&owner, &recipient, amount);
}

#[no_mangle]
pub extern "C" fn approve() {
    let spender: Key = runtime::get_named_arg("spender");
    let amount: U256 = runtime::get_named_arg("amount");
    let caller = runtime::get_caller();
    let owner = Key::Account(caller);
    write_allowance(&owner, &spender, amount);
}

#[no_mangle]
pub extern "C" fn mint() {
    let admin: Key = read_from_uref(ADMIN);
    let caller = Key::Account(runtime::get_caller());
    if caller != admin {
        runtime::revert(casper_types::ApiError::User(ERROR_UNAUTHORIZED));
    }

    let to: Key = runtime::get_named_arg("to");
    let amount: U256 = runtime::get_named_arg("amount");

    let balance = read_balance(&to);
    write_balance(&to, balance + amount);

    let total_supply: U256 = read_from_uref(TOTAL_SUPPLY);
    write_to_uref(TOTAL_SUPPLY, total_supply + amount);
}

#[no_mangle]
pub extern "C" fn burn() {
    let admin: Key = read_from_uref(ADMIN);
    let caller = Key::Account(runtime::get_caller());
    if caller != admin {
        runtime::revert(casper_types::ApiError::User(ERROR_UNAUTHORIZED));
    }

    let from: Key = runtime::get_named_arg("from");
    let amount: U256 = runtime::get_named_arg("amount");

    let balance = read_balance(&from);
    if balance < amount {
        runtime::revert(casper_types::ApiError::User(ERROR_INSUFFICIENT_BALANCE));
    }
    write_balance(&from, balance - amount);

    let total_supply: U256 = read_from_uref(TOTAL_SUPPLY);
    write_to_uref(TOTAL_SUPPLY, total_supply - amount);
}

// ============ Contract Installation ============

fn get_entry_points() -> EntryPoints {
    let mut entry_points = EntryPoints::new();

    entry_points.add_entry_point(EntryPoint::new(
        "name", vec![], CLType::String,
        EntryPointAccess::Public, EntryPointType::Contract,
    ));
    entry_points.add_entry_point(EntryPoint::new(
        "symbol", vec![], CLType::String,
        EntryPointAccess::Public, EntryPointType::Contract,
    ));
    entry_points.add_entry_point(EntryPoint::new(
        "decimals", vec![], CLType::U8,
        EntryPointAccess::Public, EntryPointType::Contract,
    ));
    entry_points.add_entry_point(EntryPoint::new(
        "total_supply", vec![], CLType::U256,
        EntryPointAccess::Public, EntryPointType::Contract,
    ));
    entry_points.add_entry_point(EntryPoint::new(
        "balance_of", vec![Parameter::new("owner", CLType::Key)], CLType::U256,
        EntryPointAccess::Public, EntryPointType::Contract,
    ));
    entry_points.add_entry_point(EntryPoint::new(
        "allowance",
        vec![Parameter::new("owner", CLType::Key), Parameter::new("spender", CLType::Key)],
        CLType::U256, EntryPointAccess::Public, EntryPointType::Contract,
    ));
    entry_points.add_entry_point(EntryPoint::new(
        "transfer",
        vec![Parameter::new("recipient", CLType::Key), Parameter::new("amount", CLType::U256)],
        CLType::Unit, EntryPointAccess::Public, EntryPointType::Contract,
    ));
    entry_points.add_entry_point(EntryPoint::new(
        "transfer_from",
        vec![
            Parameter::new("owner", CLType::Key),
            Parameter::new("recipient", CLType::Key),
            Parameter::new("amount", CLType::U256),
        ],
        CLType::Unit, EntryPointAccess::Public, EntryPointType::Contract,
    ));
    entry_points.add_entry_point(EntryPoint::new(
        "approve",
        vec![Parameter::new("spender", CLType::Key), Parameter::new("amount", CLType::U256)],
        CLType::Unit, EntryPointAccess::Public, EntryPointType::Contract,
    ));
    entry_points.add_entry_point(EntryPoint::new(
        "mint",
        vec![Parameter::new("to", CLType::Key), Parameter::new("amount", CLType::U256)],
        CLType::Unit, EntryPointAccess::Public, EntryPointType::Contract,
    ));
    entry_points.add_entry_point(EntryPoint::new(
        "burn",
        vec![Parameter::new("from", CLType::Key), Parameter::new("amount", CLType::U256)],
        CLType::Unit, EntryPointAccess::Public, EntryPointType::Contract,
    ));

    entry_points
}

#[no_mangle]
pub extern "C" fn call() {
    // Fixed token parameters
    let name = String::from(TOKEN_NAME);
    let symbol = String::from(TOKEN_SYMBOL);
    let decimals: u8 = TOKEN_DECIMALS;
    // 1 billion ECTO with 18 decimals
    let initial_supply = U256::from(1_000_000_000u64) * U256::exp10(18);

    let mut named_keys = NamedKeys::new();

    let name_uref = storage::new_uref(name);
    named_keys.insert(NAME.to_string(), name_uref.into());

    let symbol_uref = storage::new_uref(symbol);
    named_keys.insert(SYMBOL.to_string(), symbol_uref.into());

    let decimals_uref = storage::new_uref(decimals);
    named_keys.insert(DECIMALS.to_string(), decimals_uref.into());

    let total_supply_uref = storage::new_uref(initial_supply);
    named_keys.insert(TOTAL_SUPPLY.to_string(), total_supply_uref.into());

    let admin = Key::Account(runtime::get_caller());
    let admin_uref = storage::new_uref(admin);
    named_keys.insert(ADMIN.to_string(), admin_uref.into());

    let balances_dict = storage::new_dictionary(BALANCES).unwrap_or_revert();
    named_keys.insert(BALANCES.to_string(), balances_dict.into());

    let allowances_dict = storage::new_dictionary(ALLOWANCES).unwrap_or_revert();
    named_keys.insert(ALLOWANCES.to_string(), allowances_dict.into());

    let entry_points = get_entry_points();
    let (contract_hash, _) = storage::new_contract(
        entry_points,
        Some(named_keys),
        Some("ecto_token_package".to_string()),
        Some("ecto_token_access".to_string()),
    );

    runtime::put_key("ecto_token_contract", contract_hash.into());

    // Mint initial supply to deployer
    let balance_key = key_to_str(&admin);
    storage::dictionary_put(balances_dict, &balance_key, initial_supply);
}
