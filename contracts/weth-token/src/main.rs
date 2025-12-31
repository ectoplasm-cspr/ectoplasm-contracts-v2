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

const NAME: &str = "name";
const SYMBOL: &str = "symbol";
const DECIMALS: &str = "decimals";
const TOTAL_SUPPLY: &str = "total_supply";
const BALANCES: &str = "balances";
const ALLOWANCES: &str = "allowances";
const ADMIN: &str = "admin";

// WETH - Wrapped Ether (18 decimals)
const TOKEN_NAME: &str = "Wrapped Ether";
const TOKEN_SYMBOL: &str = "WETH";
const TOKEN_DECIMALS: u8 = 18;

const ERROR_INSUFFICIENT_BALANCE: u16 = 1;
const ERROR_INSUFFICIENT_ALLOWANCE: u16 = 2;
const ERROR_UNAUTHORIZED: u16 = 3;

fn read_from_uref<T: CLTyped + FromBytes>(name: &str) -> T {
    let key = runtime::get_key(name).unwrap_or_revert();
    storage::read(key.into_uref().unwrap_or_revert()).unwrap_or_revert().unwrap_or_revert()
}

fn write_to_uref<T: CLTyped + ToBytes>(name: &str, value: T) {
    let key = runtime::get_key(name).unwrap_or_revert();
    storage::write(key.into_uref().unwrap_or_revert(), value);
}

fn get_dictionary_uref(name: &str) -> URef {
    runtime::get_key(name).unwrap_or_revert().into_uref().unwrap_or_revert()
}

fn key_to_str(key: &Key) -> String {
    match key {
        Key::Account(ah) => hex_encode(ah.as_bytes()),
        Key::Hash(h) => hex_encode(h),
        _ => hex_encode(&key.to_bytes().unwrap_or_revert()),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut r = String::with_capacity(bytes.len() * 2);
    for b in bytes { r.push(hex_char(b >> 4)); r.push(hex_char(b & 0x0f)); }
    r
}

fn hex_char(n: u8) -> char { if n < 10 { (b'0' + n) as char } else { (b'a' + n - 10) as char } }

fn allowance_key(o: &Key, s: &Key) -> String { let mut k = key_to_str(o); k.push('_'); k.push_str(&key_to_str(s)); k }

fn read_balance(owner: &Key) -> U256 { storage::dictionary_get(get_dictionary_uref(BALANCES), &key_to_str(owner)).unwrap_or_default().unwrap_or_default() }
fn write_balance(owner: &Key, amount: U256) { storage::dictionary_put(get_dictionary_uref(BALANCES), &key_to_str(owner), amount); }
fn read_allowance(o: &Key, s: &Key) -> U256 { storage::dictionary_get(get_dictionary_uref(ALLOWANCES), &allowance_key(o, s)).unwrap_or_default().unwrap_or_default() }
fn write_allowance(o: &Key, s: &Key, a: U256) { storage::dictionary_put(get_dictionary_uref(ALLOWANCES), &allowance_key(o, s), a); }

fn transfer_internal(sender: &Key, recipient: &Key, amount: U256) {
    let bal = read_balance(sender);
    if bal < amount { runtime::revert(casper_types::ApiError::User(ERROR_INSUFFICIENT_BALANCE)); }
    write_balance(sender, bal - amount);
    write_balance(recipient, read_balance(recipient) + amount);
}

#[no_mangle] pub extern "C" fn name() { runtime::ret(CLValue::from_t(read_from_uref::<String>(NAME)).unwrap_or_revert()); }
#[no_mangle] pub extern "C" fn symbol() { runtime::ret(CLValue::from_t(read_from_uref::<String>(SYMBOL)).unwrap_or_revert()); }
#[no_mangle] pub extern "C" fn decimals() { runtime::ret(CLValue::from_t(read_from_uref::<u8>(DECIMALS)).unwrap_or_revert()); }
#[no_mangle] pub extern "C" fn total_supply() { runtime::ret(CLValue::from_t(read_from_uref::<U256>(TOTAL_SUPPLY)).unwrap_or_revert()); }
#[no_mangle] pub extern "C" fn balance_of() { runtime::ret(CLValue::from_t(read_balance(&runtime::get_named_arg::<Key>("owner"))).unwrap_or_revert()); }
#[no_mangle] pub extern "C" fn allowance() { runtime::ret(CLValue::from_t(read_allowance(&runtime::get_named_arg("owner"), &runtime::get_named_arg("spender"))).unwrap_or_revert()); }

#[no_mangle] pub extern "C" fn transfer() { transfer_internal(&Key::Account(runtime::get_caller()), &runtime::get_named_arg("recipient"), runtime::get_named_arg("amount")); }

#[no_mangle] pub extern "C" fn transfer_from() {
    let owner: Key = runtime::get_named_arg("owner");
    let amount: U256 = runtime::get_named_arg("amount");
    let spender = Key::Account(runtime::get_caller());
    let allow = read_allowance(&owner, &spender);
    if allow < amount { runtime::revert(casper_types::ApiError::User(ERROR_INSUFFICIENT_ALLOWANCE)); }
    write_allowance(&owner, &spender, allow - amount);
    transfer_internal(&owner, &runtime::get_named_arg("recipient"), amount);
}

#[no_mangle] pub extern "C" fn approve() { write_allowance(&Key::Account(runtime::get_caller()), &runtime::get_named_arg("spender"), runtime::get_named_arg("amount")); }

#[no_mangle] pub extern "C" fn mint() {
    if Key::Account(runtime::get_caller()) != read_from_uref::<Key>(ADMIN) { runtime::revert(casper_types::ApiError::User(ERROR_UNAUTHORIZED)); }
    let to: Key = runtime::get_named_arg("to");
    let amount: U256 = runtime::get_named_arg("amount");
    write_balance(&to, read_balance(&to) + amount);
    write_to_uref(TOTAL_SUPPLY, read_from_uref::<U256>(TOTAL_SUPPLY) + amount);
}

#[no_mangle] pub extern "C" fn burn() {
    if Key::Account(runtime::get_caller()) != read_from_uref::<Key>(ADMIN) { runtime::revert(casper_types::ApiError::User(ERROR_UNAUTHORIZED)); }
    let from: Key = runtime::get_named_arg("from");
    let amount: U256 = runtime::get_named_arg("amount");
    let bal = read_balance(&from);
    if bal < amount { runtime::revert(casper_types::ApiError::User(ERROR_INSUFFICIENT_BALANCE)); }
    write_balance(&from, bal - amount);
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
    // 10,000 WETH initial supply
    let initial_supply = U256::from(10_000u64) * U256::exp10(TOKEN_DECIMALS as usize);
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

    let (contract_hash, _) = storage::new_contract(get_entry_points(), Some(named_keys), Some("weth_token_package".to_string()), Some("weth_token_access".to_string()));
    runtime::put_key("weth_token_contract", contract_hash.into());
    storage::dictionary_put(balances_dict, &key_to_str(&admin), initial_supply);
}
