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

// Token constants - USDC has 6 decimals
const TOKEN_NAME: &str = "USD Coin";
const TOKEN_SYMBOL: &str = "USDC";
const TOKEN_DECIMALS: u8 = 6;

const ERROR_INSUFFICIENT_BALANCE: u16 = 1;
const ERROR_INSUFFICIENT_ALLOWANCE: u16 = 2;
const ERROR_UNAUTHORIZED: u16 = 3;

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
    runtime::get_key(name).unwrap_or_revert().into_uref().unwrap_or_revert()
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

fn allowance_key(owner: &Key, spender: &Key) -> String {
    let mut key = key_to_str(owner);
    key.push('_');
    key.push_str(&key_to_str(spender));
    key
}

fn read_balance(owner: &Key) -> U256 {
    let dict_uref = get_dictionary_uref(BALANCES);
    storage::dictionary_get(dict_uref, &key_to_str(owner)).unwrap_or_default().unwrap_or_default()
}

fn write_balance(owner: &Key, amount: U256) {
    let dict_uref = get_dictionary_uref(BALANCES);
    storage::dictionary_put(dict_uref, &key_to_str(owner), amount);
}

fn read_allowance(owner: &Key, spender: &Key) -> U256 {
    let dict_uref = get_dictionary_uref(ALLOWANCES);
    storage::dictionary_get(dict_uref, &allowance_key(owner, spender)).unwrap_or_default().unwrap_or_default()
}

fn write_allowance(owner: &Key, spender: &Key, amount: U256) {
    let dict_uref = get_dictionary_uref(ALLOWANCES);
    storage::dictionary_put(dict_uref, &allowance_key(owner, spender), amount);
}

fn transfer_internal(sender: &Key, recipient: &Key, amount: U256) {
    let sender_balance = read_balance(sender);
    if sender_balance < amount {
        runtime::revert(casper_types::ApiError::User(ERROR_INSUFFICIENT_BALANCE));
    }
    write_balance(sender, sender_balance - amount);
    write_balance(recipient, read_balance(recipient) + amount);
}

#[no_mangle] pub extern "C" fn name() { runtime::ret(CLValue::from_t(read_from_uref::<String>(NAME)).unwrap_or_revert()); }
#[no_mangle] pub extern "C" fn symbol() { runtime::ret(CLValue::from_t(read_from_uref::<String>(SYMBOL)).unwrap_or_revert()); }
#[no_mangle] pub extern "C" fn decimals() { runtime::ret(CLValue::from_t(read_from_uref::<u8>(DECIMALS)).unwrap_or_revert()); }
#[no_mangle] pub extern "C" fn total_supply() { runtime::ret(CLValue::from_t(read_from_uref::<U256>(TOTAL_SUPPLY)).unwrap_or_revert()); }
#[no_mangle] pub extern "C" fn balance_of() { let owner: Key = runtime::get_named_arg("owner"); runtime::ret(CLValue::from_t(read_balance(&owner)).unwrap_or_revert()); }
#[no_mangle] pub extern "C" fn allowance() { let owner: Key = runtime::get_named_arg("owner"); let spender: Key = runtime::get_named_arg("spender"); runtime::ret(CLValue::from_t(read_allowance(&owner, &spender)).unwrap_or_revert()); }

#[no_mangle]
pub extern "C" fn transfer() {
    let recipient: Key = runtime::get_named_arg("recipient");
    let amount: U256 = runtime::get_named_arg("amount");
    transfer_internal(&Key::Account(runtime::get_caller()), &recipient, amount);
}

#[no_mangle]
pub extern "C" fn transfer_from() {
    let owner: Key = runtime::get_named_arg("owner");
    let recipient: Key = runtime::get_named_arg("recipient");
    let amount: U256 = runtime::get_named_arg("amount");
    let spender = Key::Account(runtime::get_caller());
    let current_allowance = read_allowance(&owner, &spender);
    if current_allowance < amount { runtime::revert(casper_types::ApiError::User(ERROR_INSUFFICIENT_ALLOWANCE)); }
    write_allowance(&owner, &spender, current_allowance - amount);
    transfer_internal(&owner, &recipient, amount);
}

#[no_mangle]
pub extern "C" fn approve() {
    let spender: Key = runtime::get_named_arg("spender");
    let amount: U256 = runtime::get_named_arg("amount");
    write_allowance(&Key::Account(runtime::get_caller()), &spender, amount);
}

#[no_mangle]
pub extern "C" fn mint() {
    let admin: Key = read_from_uref(ADMIN);
    if Key::Account(runtime::get_caller()) != admin { runtime::revert(casper_types::ApiError::User(ERROR_UNAUTHORIZED)); }
    let to: Key = runtime::get_named_arg("to");
    let amount: U256 = runtime::get_named_arg("amount");
    write_balance(&to, read_balance(&to) + amount);
    write_to_uref(TOTAL_SUPPLY, read_from_uref::<U256>(TOTAL_SUPPLY) + amount);
}

#[no_mangle]
pub extern "C" fn burn() {
    let admin: Key = read_from_uref(ADMIN);
    if Key::Account(runtime::get_caller()) != admin { runtime::revert(casper_types::ApiError::User(ERROR_UNAUTHORIZED)); }
    let from: Key = runtime::get_named_arg("from");
    let amount: U256 = runtime::get_named_arg("amount");
    let balance = read_balance(&from);
    if balance < amount { runtime::revert(casper_types::ApiError::User(ERROR_INSUFFICIENT_BALANCE)); }
    write_balance(&from, balance - amount);
    write_to_uref(TOTAL_SUPPLY, read_from_uref::<U256>(TOTAL_SUPPLY) - amount);
}

fn get_entry_points() -> EntryPoints {
    let mut ep = EntryPoints::new();
    ep.add_entry_point(EntryPoint::new("name", vec![], CLType::String, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("symbol", vec![], CLType::String, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("decimals", vec![], CLType::U8, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("total_supply", vec![], CLType::U256, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("balance_of", vec![Parameter::new("owner", CLType::Key)], CLType::U256, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("allowance", vec![Parameter::new("owner", CLType::Key), Parameter::new("spender", CLType::Key)], CLType::U256, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("transfer", vec![Parameter::new("recipient", CLType::Key), Parameter::new("amount", CLType::U256)], CLType::Unit, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("transfer_from", vec![Parameter::new("owner", CLType::Key), Parameter::new("recipient", CLType::Key), Parameter::new("amount", CLType::U256)], CLType::Unit, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("approve", vec![Parameter::new("spender", CLType::Key), Parameter::new("amount", CLType::U256)], CLType::Unit, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("mint", vec![Parameter::new("to", CLType::Key), Parameter::new("amount", CLType::U256)], CLType::Unit, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("burn", vec![Parameter::new("from", CLType::Key), Parameter::new("amount", CLType::U256)], CLType::Unit, EntryPointAccess::Public, EntryPointType::Contract));
    ep
}

#[no_mangle]
pub extern "C" fn call() {
    let initial_supply = U256::from(1_000_000_000u64) * U256::exp10(TOKEN_DECIMALS as usize);
    let mut named_keys = NamedKeys::new();

    named_keys.insert(NAME.to_string(), storage::new_uref(String::from(TOKEN_NAME)).into());
    named_keys.insert(SYMBOL.to_string(), storage::new_uref(String::from(TOKEN_SYMBOL)).into());
    named_keys.insert(DECIMALS.to_string(), storage::new_uref(TOKEN_DECIMALS).into());
    named_keys.insert(TOTAL_SUPPLY.to_string(), storage::new_uref(initial_supply).into());

    let admin = Key::Account(runtime::get_caller());
    named_keys.insert(ADMIN.to_string(), storage::new_uref(admin).into());

    let balances_dict = storage::new_dictionary(BALANCES).unwrap_or_revert();
    named_keys.insert(BALANCES.to_string(), balances_dict.into());
    named_keys.insert(ALLOWANCES.to_string(), storage::new_dictionary(ALLOWANCES).unwrap_or_revert().into());

    let (contract_hash, _) = storage::new_contract(get_entry_points(), Some(named_keys), Some("usdc_token_package".to_string()), Some("usdc_token_access".to_string()));
    runtime::put_key("usdc_token_contract", contract_hash.into());
    storage::dictionary_put(balances_dict, &key_to_str(&admin), initial_supply);
}
