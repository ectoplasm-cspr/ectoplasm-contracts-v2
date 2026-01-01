#![no_std]
#![no_main]

extern crate alloc;

mod data;
mod balances;
mod allowances;
mod error;

use alloc::{string::{String, ToString}, vec, vec::Vec};
use casper_contract::{
    contract_api::{runtime, storage},
    unwrap_or_revert::UnwrapOrRevert,
};
use casper_types::{
    addressable_entity::{EntityEntryPoint as EntryPoint, EntryPoints},
    api_error::ApiError,
    contracts::NamedKeys,
    CLType, CLValue, EntryPointAccess, EntryPointPayment, EntryPointType, Key, Parameter, RuntimeArgs, U256,
};

use data::{
    ALLOWANCES, BALANCES, DECIMALS, NAME, SYMBOL, TOTAL_SUPPLY,
    ADMIN, CONTRACT_HASH,
};
use error::Cep18Error;

// ============ Entry Points ============

#[no_mangle]
pub extern "C" fn name() {
    let name: String = data::read_named_key(NAME);
    runtime::ret(CLValue::from_t(name).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn symbol() {
    let symbol: String = data::read_named_key(SYMBOL);
    runtime::ret(CLValue::from_t(symbol).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn decimals() {
    let decimals: u8 = data::read_named_key(DECIMALS);
    runtime::ret(CLValue::from_t(decimals).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn total_supply() {
    let total_supply: U256 = data::read_named_key(TOTAL_SUPPLY);
    runtime::ret(CLValue::from_t(total_supply).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn balance_of() {
    let owner: Key = runtime::get_named_arg("owner");
    let balance = balances::read_balance(&owner);
    runtime::ret(CLValue::from_t(balance).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn allowance() {
    let owner: Key = runtime::get_named_arg("owner");
    let spender: Key = runtime::get_named_arg("spender");
    let allowance = allowances::read_allowance(&owner, &spender);
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

    // Check allowance
    let current_allowance = allowances::read_allowance(&owner, &spender);
    if current_allowance < amount {
        runtime::revert(Cep18Error::InsufficientAllowance);
    }

    // Decrease allowance
    allowances::write_allowance(&owner, &spender, current_allowance - amount);

    // Transfer
    transfer_internal(&owner, &recipient, amount);
}

#[no_mangle]
pub extern "C" fn approve() {
    let spender: Key = runtime::get_named_arg("spender");
    let amount: U256 = runtime::get_named_arg("amount");

    let caller = runtime::get_caller();
    let owner = Key::Account(caller);

    allowances::write_allowance(&owner, &spender, amount);
}

#[no_mangle]
pub extern "C" fn mint() {
    // Only admin can mint
    let admin: Key = data::read_named_key(ADMIN);
    let caller = Key::Account(runtime::get_caller());
    if caller != admin {
        runtime::revert(Cep18Error::Unauthorized);
    }

    let to: Key = runtime::get_named_arg("to");
    let amount: U256 = runtime::get_named_arg("amount");

    // Increase balance
    let balance = balances::read_balance(&to);
    balances::write_balance(&to, balance + amount);

    // Increase total supply
    let total_supply: U256 = data::read_named_key(TOTAL_SUPPLY);
    data::write_named_key(TOTAL_SUPPLY, total_supply + amount);
}

#[no_mangle]
pub extern "C" fn burn() {
    // Only admin can burn
    let admin: Key = data::read_named_key(ADMIN);
    let caller = Key::Account(runtime::get_caller());
    if caller != admin {
        runtime::revert(Cep18Error::Unauthorized);
    }

    let from: Key = runtime::get_named_arg("from");
    let amount: U256 = runtime::get_named_arg("amount");

    // Decrease balance
    let balance = balances::read_balance(&from);
    if balance < amount {
        runtime::revert(Cep18Error::InsufficientBalance);
    }
    balances::write_balance(&from, balance - amount);

    // Decrease total supply
    let total_supply: U256 = data::read_named_key(TOTAL_SUPPLY);
    data::write_named_key(TOTAL_SUPPLY, total_supply - amount);
}

// ============ Internal Functions ============

fn transfer_internal(sender: &Key, recipient: &Key, amount: U256) {
    // Check sender balance
    let sender_balance = balances::read_balance(sender);
    if sender_balance < amount {
        runtime::revert(Cep18Error::InsufficientBalance);
    }

    // Update balances
    balances::write_balance(sender, sender_balance - amount);
    let recipient_balance = balances::read_balance(recipient);
    balances::write_balance(recipient, recipient_balance + amount);
}

// ============ Contract Installation ============

fn get_entry_points() -> EntryPoints {
    let mut entry_points = EntryPoints::new();

    // View functions
    entry_points.add_entry_point(EntryPoint::new(
        "name",
        vec![],
        CLType::String,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "symbol",
        vec![],
        CLType::String,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "decimals",
        vec![],
        CLType::U8,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "total_supply",
        vec![],
        CLType::U256,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "balance_of",
        vec![Parameter::new("owner", CLType::Key)],
        CLType::U256,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "allowance",
        vec![
            Parameter::new("owner", CLType::Key),
            Parameter::new("spender", CLType::Key),
        ],
        CLType::U256,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    // State-changing functions
    entry_points.add_entry_point(EntryPoint::new(
        "transfer",
        vec![
            Parameter::new("recipient", CLType::Key),
            Parameter::new("amount", CLType::U256),
        ],
        CLType::Unit,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "transfer_from",
        vec![
            Parameter::new("owner", CLType::Key),
            Parameter::new("recipient", CLType::Key),
            Parameter::new("amount", CLType::U256),
        ],
        CLType::Unit,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "approve",
        vec![
            Parameter::new("spender", CLType::Key),
            Parameter::new("amount", CLType::U256),
        ],
        CLType::Unit,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "mint",
        vec![
            Parameter::new("to", CLType::Key),
            Parameter::new("amount", CLType::U256),
        ],
        CLType::Unit,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "burn",
        vec![
            Parameter::new("from", CLType::Key),
            Parameter::new("amount", CLType::U256),
        ],
        CLType::Unit,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points
}

#[no_mangle]
pub extern "C" fn call() {
    // Get installation arguments
    let name: String = runtime::get_named_arg("name");
    let symbol: String = runtime::get_named_arg("symbol");
    let decimals: u8 = runtime::get_named_arg("decimals");
    let initial_supply: U256 = runtime::get_named_arg("initial_supply");

    // Create named keys
    let mut named_keys = NamedKeys::new();

    // Store metadata as URefs
    let name_uref = storage::new_uref(name);
    named_keys.insert(NAME.to_string(), name_uref.into());

    let symbol_uref = storage::new_uref(symbol);
    named_keys.insert(SYMBOL.to_string(), symbol_uref.into());

    let decimals_uref = storage::new_uref(decimals);
    named_keys.insert(DECIMALS.to_string(), decimals_uref.into());

    let total_supply_uref = storage::new_uref(initial_supply);
    named_keys.insert(TOTAL_SUPPLY.to_string(), total_supply_uref.into());

    // Store admin (deployer)
    let admin = Key::Account(runtime::get_caller());
    let admin_uref = storage::new_uref(admin);
    named_keys.insert(ADMIN.to_string(), admin_uref.into());

    // Create dictionaries for balances and allowances
    let balances_dict = storage::new_dictionary(BALANCES).unwrap_or_revert();
    named_keys.insert(BALANCES.to_string(), balances_dict.into());

    let allowances_dict = storage::new_dictionary(ALLOWANCES).unwrap_or_revert();
    named_keys.insert(ALLOWANCES.to_string(), allowances_dict.into());

    // Create the contract
    let entry_points = get_entry_points();
    let (contract_hash, _contract_version) = storage::new_contract(
        entry_points,
        Some(named_keys),
        Some("cep18_token_package".to_string()),
        Some("cep18_token_access".to_string()),
        None,
    );

    // Store contract hash in account's named keys
    runtime::put_key(CONTRACT_HASH, contract_hash.into());

    // Mint initial supply to deployer if > 0
    if initial_supply > U256::zero() {
        // Write directly to the dictionary
        let balance_key = data::key_to_str(&admin);
        storage::dictionary_put(balances_dict, &balance_key, initial_supply);
    }
}
