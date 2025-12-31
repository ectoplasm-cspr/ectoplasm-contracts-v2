#![no_std]
#![no_main]

// ECTO Token - Ectoplasm native token (18 decimals)
// Uses the same CEP-18 implementation as cep18-token

extern crate alloc;

use alloc::string::String;
use casper_contract::{
    contract_api::{runtime, storage},
    unwrap_or_revert::UnwrapOrRevert,
};
use casper_types::{
    contracts::NamedKeys, CLType, CLValue, EntryPoint, EntryPointAccess, EntryPointType,
    EntryPoints, Key, Parameter, U256,
};

// Storage keys
const NAME: &str = "name";
const SYMBOL: &str = "symbol";
const DECIMALS: &str = "decimals";
const TOTAL_SUPPLY: &str = "total_supply";
const BALANCES: &str = "balances";
const ALLOWANCES: &str = "allowances";
const ADMIN: &str = "admin";

// Include the CEP-18 entry points (copy from cep18-token for now)
// In production, this would be a shared library

#[no_mangle]
pub extern "C" fn call() {
    // Hardcoded for ECTO token
    let name = String::from("Ectoplasm Token");
    let symbol = String::from("ECTO");
    let decimals: u8 = 18;
    let initial_supply = U256::from(1_000_000_000u64) * U256::exp10(18); // 1 billion ECTO

    // Create named keys
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
    let balance_key = key_to_hex(&admin);
    storage::dictionary_put(balances_dict, &balance_key, initial_supply);
}

fn key_to_hex(key: &Key) -> String {
    match key {
        Key::Account(account_hash) => {
            let bytes = account_hash.as_bytes();
            let mut result = String::with_capacity(bytes.len() * 2);
            for byte in bytes {
                result.push(hex_char(byte >> 4));
                result.push(hex_char(byte & 0x0f));
            }
            result
        }
        _ => String::new(),
    }
}

fn hex_char(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'a' + nibble - 10) as char,
        _ => '0',
    }
}

fn get_entry_points() -> EntryPoints {
    let mut entry_points = EntryPoints::new();

    entry_points.add_entry_point(EntryPoint::new(
        "name", vec![], CLType::String, EntryPointAccess::Public, EntryPointType::Called,
    ));
    entry_points.add_entry_point(EntryPoint::new(
        "symbol", vec![], CLType::String, EntryPointAccess::Public, EntryPointType::Called,
    ));
    entry_points.add_entry_point(EntryPoint::new(
        "decimals", vec![], CLType::U8, EntryPointAccess::Public, EntryPointType::Called,
    ));
    entry_points.add_entry_point(EntryPoint::new(
        "total_supply", vec![], CLType::U256, EntryPointAccess::Public, EntryPointType::Called,
    ));
    entry_points.add_entry_point(EntryPoint::new(
        "balance_of", vec![Parameter::new("owner", CLType::Key)], CLType::U256, EntryPointAccess::Public, EntryPointType::Called,
    ));
    entry_points.add_entry_point(EntryPoint::new(
        "allowance", vec![Parameter::new("owner", CLType::Key), Parameter::new("spender", CLType::Key)], CLType::U256, EntryPointAccess::Public, EntryPointType::Called,
    ));
    entry_points.add_entry_point(EntryPoint::new(
        "transfer", vec![Parameter::new("recipient", CLType::Key), Parameter::new("amount", CLType::U256)], CLType::Unit, EntryPointAccess::Public, EntryPointType::Called,
    ));
    entry_points.add_entry_point(EntryPoint::new(
        "transfer_from", vec![Parameter::new("owner", CLType::Key), Parameter::new("recipient", CLType::Key), Parameter::new("amount", CLType::U256)], CLType::Unit, EntryPointAccess::Public, EntryPointType::Called,
    ));
    entry_points.add_entry_point(EntryPoint::new(
        "approve", vec![Parameter::new("spender", CLType::Key), Parameter::new("amount", CLType::U256)], CLType::Unit, EntryPointAccess::Public, EntryPointType::Called,
    ));
    entry_points.add_entry_point(EntryPoint::new(
        "mint", vec![Parameter::new("to", CLType::Key), Parameter::new("amount", CLType::U256)], CLType::Unit, EntryPointAccess::Public, EntryPointType::Called,
    ));
    entry_points.add_entry_point(EntryPoint::new(
        "burn", vec![Parameter::new("from", CLType::Key), Parameter::new("amount", CLType::U256)], CLType::Unit, EntryPointAccess::Public, EntryPointType::Called,
    ));

    entry_points
}

// Entry point implementations would go here
// For brevity, creating placeholder that will be filled in later

#[no_mangle] pub extern "C" fn name() { runtime::ret(CLValue::from_t(String::from("Ectoplasm Token")).unwrap_or_revert()); }
#[no_mangle] pub extern "C" fn symbol() { runtime::ret(CLValue::from_t(String::from("ECTO")).unwrap_or_revert()); }
#[no_mangle] pub extern "C" fn decimals() { runtime::ret(CLValue::from_t(18u8).unwrap_or_revert()); }
#[no_mangle] pub extern "C" fn total_supply() { /* TODO */ }
#[no_mangle] pub extern "C" fn balance_of() { /* TODO */ }
#[no_mangle] pub extern "C" fn allowance() { /* TODO */ }
#[no_mangle] pub extern "C" fn transfer() { /* TODO */ }
#[no_mangle] pub extern "C" fn transfer_from() { /* TODO */ }
#[no_mangle] pub extern "C" fn approve() { /* TODO */ }
#[no_mangle] pub extern "C" fn mint() { /* TODO */ }
#[no_mangle] pub extern "C" fn burn() { /* TODO */ }
