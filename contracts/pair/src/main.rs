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
    bytesrepr::{FromBytes, ToBytes},
    contracts::NamedKeys,
    runtime_args, RuntimeArgs,
    CLType, CLTyped, CLValue, ContractHash, EntryPoint, EntryPointAccess, EntryPointType,
    EntryPoints, Key, Parameter, URef, U256,
};

// Storage keys
const TOKEN0: &str = "token0";
const TOKEN1: &str = "token1";
const RESERVE0: &str = "reserve0";
const RESERVE1: &str = "reserve1";
const FACTORY: &str = "factory";
const BLOCK_TIMESTAMP_LAST: &str = "block_timestamp_last";
const PRICE0_CUMULATIVE_LAST: &str = "price0_cumulative_last";
const PRICE1_CUMULATIVE_LAST: &str = "price1_cumulative_last";
const K_LAST: &str = "k_last";
const LOCKED: &str = "locked";

// LP Token storage
const LP_NAME: &str = "lp_name";
const LP_SYMBOL: &str = "lp_symbol";
const LP_DECIMALS: &str = "lp_decimals";
const LP_TOTAL_SUPPLY: &str = "lp_total_supply";
const LP_BALANCES: &str = "lp_balances";
const LP_ALLOWANCES: &str = "lp_allowances";

// Constants
const MINIMUM_LIQUIDITY: u128 = 1000;

// Error codes
const ERROR_INSUFFICIENT_BALANCE: u16 = 1;
const ERROR_INSUFFICIENT_ALLOWANCE: u16 = 2;
const ERROR_INSUFFICIENT_LIQUIDITY: u16 = 3;
const ERROR_INSUFFICIENT_INPUT_AMOUNT: u16 = 4;
const ERROR_INSUFFICIENT_OUTPUT_AMOUNT: u16 = 5;
const ERROR_INSUFFICIENT_LIQUIDITY_MINTED: u16 = 6;
const ERROR_INSUFFICIENT_LIQUIDITY_BURNED: u16 = 7;
const ERROR_INVALID_TO: u16 = 8;
const ERROR_K: u16 = 9;
const ERROR_LOCKED: u16 = 10;
const ERROR_OVERFLOW: u16 = 11;

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

// ============ LP Token Functions ============

fn read_lp_balance(owner: &Key) -> U256 {
    let dict_uref = get_dictionary_uref(LP_BALANCES);
    storage::dictionary_get(dict_uref, &key_to_str(owner))
        .unwrap_or_default()
        .unwrap_or_default()
}

fn write_lp_balance(owner: &Key, amount: U256) {
    let dict_uref = get_dictionary_uref(LP_BALANCES);
    storage::dictionary_put(dict_uref, &key_to_str(owner), amount);
}

fn read_lp_allowance(owner: &Key, spender: &Key) -> U256 {
    let dict_uref = get_dictionary_uref(LP_ALLOWANCES);
    storage::dictionary_get(dict_uref, &allowance_key(owner, spender))
        .unwrap_or_default()
        .unwrap_or_default()
}

fn write_lp_allowance(owner: &Key, spender: &Key, amount: U256) {
    let dict_uref = get_dictionary_uref(LP_ALLOWANCES);
    storage::dictionary_put(dict_uref, &allowance_key(owner, spender), amount);
}

fn mint_lp(to: &Key, amount: U256) {
    let balance = read_lp_balance(to);
    write_lp_balance(to, balance + amount);
    let total_supply: U256 = read_from_uref(LP_TOTAL_SUPPLY);
    write_to_uref(LP_TOTAL_SUPPLY, total_supply + amount);
}

fn burn_lp(from: &Key, amount: U256) {
    let balance = read_lp_balance(from);
    if balance < amount {
        runtime::revert(casper_types::ApiError::User(ERROR_INSUFFICIENT_BALANCE));
    }
    write_lp_balance(from, balance - amount);
    let total_supply: U256 = read_from_uref(LP_TOTAL_SUPPLY);
    write_to_uref(LP_TOTAL_SUPPLY, total_supply - amount);
}

fn transfer_lp_internal(sender: &Key, recipient: &Key, amount: U256) {
    let sender_balance = read_lp_balance(sender);
    if sender_balance < amount {
        runtime::revert(casper_types::ApiError::User(ERROR_INSUFFICIENT_BALANCE));
    }
    write_lp_balance(sender, sender_balance - amount);
    write_lp_balance(recipient, read_lp_balance(recipient) + amount);
}

// ============ Reentrancy Guard ============

fn lock() {
    let locked: bool = read_from_uref(LOCKED);
    if locked {
        runtime::revert(casper_types::ApiError::User(ERROR_LOCKED));
    }
    write_to_uref(LOCKED, true);
}

fn unlock() {
    write_to_uref(LOCKED, false);
}

// ============ AMM Functions ============

fn get_reserves() -> (U256, U256, u64) {
    let reserve0: U256 = read_from_uref(RESERVE0);
    let reserve1: U256 = read_from_uref(RESERVE1);
    let block_timestamp_last: u64 = read_from_uref(BLOCK_TIMESTAMP_LAST);
    (reserve0, reserve1, block_timestamp_last)
}

fn update_reserves(balance0: U256, balance1: U256) {
    write_to_uref(RESERVE0, balance0);
    write_to_uref(RESERVE1, balance1);
    // Note: In a real implementation, we'd use block timestamp
    // For simplicity, we're just tracking reserves here
    write_to_uref(BLOCK_TIMESTAMP_LAST, 0u64);
}

fn sqrt(y: U256) -> U256 {
    if y > U256::from(3u32) {
        let mut z = y;
        let mut x = y / 2 + 1;
        while x < z {
            z = x;
            x = (y / x + x) / 2;
        }
        z
    } else if y != U256::zero() {
        U256::from(1u32)
    } else {
        U256::zero()
    }
}

fn min(a: U256, b: U256) -> U256 {
    if a < b { a } else { b }
}

/// Call token's balance_of entry point
fn get_token_balance(token: Key, owner: Key) -> U256 {
    let contract_hash = match token {
        Key::Hash(hash) => ContractHash::new(hash),
        _ => runtime::revert(casper_types::ApiError::User(ERROR_INVALID_TO)),
    };

    runtime::call_contract(
        contract_hash,
        "balance_of",
        runtime_args! {
            "owner" => owner
        },
    )
}

/// Call token's transfer entry point
fn transfer_token(token: Key, recipient: Key, amount: U256) {
    let contract_hash = match token {
        Key::Hash(hash) => ContractHash::new(hash),
        _ => runtime::revert(casper_types::ApiError::User(ERROR_INVALID_TO)),
    };

    runtime::call_contract::<()>(
        contract_hash,
        "transfer",
        runtime_args! {
            "recipient" => recipient,
            "amount" => amount
        },
    );
}

// ============ LP Token Entry Points ============

#[no_mangle]
pub extern "C" fn name() {
    let name: String = read_from_uref(LP_NAME);
    runtime::ret(CLValue::from_t(name).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn symbol() {
    let symbol: String = read_from_uref(LP_SYMBOL);
    runtime::ret(CLValue::from_t(symbol).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn decimals() {
    let decimals: u8 = read_from_uref(LP_DECIMALS);
    runtime::ret(CLValue::from_t(decimals).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn total_supply() {
    let total_supply: U256 = read_from_uref(LP_TOTAL_SUPPLY);
    runtime::ret(CLValue::from_t(total_supply).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn balance_of() {
    let owner: Key = runtime::get_named_arg("owner");
    let balance = read_lp_balance(&owner);
    runtime::ret(CLValue::from_t(balance).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn allowance() {
    let owner: Key = runtime::get_named_arg("owner");
    let spender: Key = runtime::get_named_arg("spender");
    let allowance = read_lp_allowance(&owner, &spender);
    runtime::ret(CLValue::from_t(allowance).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn transfer() {
    let recipient: Key = runtime::get_named_arg("recipient");
    let amount: U256 = runtime::get_named_arg("amount");
    let sender = Key::Account(runtime::get_caller());
    transfer_lp_internal(&sender, &recipient, amount);
}

#[no_mangle]
pub extern "C" fn transfer_from() {
    let owner: Key = runtime::get_named_arg("owner");
    let recipient: Key = runtime::get_named_arg("recipient");
    let amount: U256 = runtime::get_named_arg("amount");
    let spender = Key::Account(runtime::get_caller());

    let current_allowance = read_lp_allowance(&owner, &spender);
    if current_allowance < amount {
        runtime::revert(casper_types::ApiError::User(ERROR_INSUFFICIENT_ALLOWANCE));
    }

    write_lp_allowance(&owner, &spender, current_allowance - amount);
    transfer_lp_internal(&owner, &recipient, amount);
}

#[no_mangle]
pub extern "C" fn approve() {
    let spender: Key = runtime::get_named_arg("spender");
    let amount: U256 = runtime::get_named_arg("amount");
    let owner = Key::Account(runtime::get_caller());
    write_lp_allowance(&owner, &spender, amount);
}

// ============ AMM Entry Points ============

#[no_mangle]
pub extern "C" fn token0() {
    let token0: Key = read_from_uref(TOKEN0);
    runtime::ret(CLValue::from_t(token0).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn token1() {
    let token1: Key = read_from_uref(TOKEN1);
    runtime::ret(CLValue::from_t(token1).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn factory() {
    let factory: Key = read_from_uref(FACTORY);
    runtime::ret(CLValue::from_t(factory).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn get_reserves_ep() {
    let (reserve0, reserve1, block_timestamp_last) = get_reserves();
    runtime::ret(CLValue::from_t((reserve0, reserve1, block_timestamp_last)).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn mint() {
    lock();

    let to: Key = runtime::get_named_arg("to");

    let (reserve0, reserve1, _) = get_reserves();
    let token0: Key = read_from_uref(TOKEN0);
    let token1: Key = read_from_uref(TOKEN1);

    // Get this contract's key
    let self_key = runtime::get_key("ectoplasm_pair_contract").unwrap_or_revert();

    let balance0 = get_token_balance(token0, self_key);
    let balance1 = get_token_balance(token1, self_key);

    let amount0 = balance0 - reserve0;
    let amount1 = balance1 - reserve1;

    let total_supply: U256 = read_from_uref(LP_TOTAL_SUPPLY);
    let liquidity: U256;

    if total_supply == U256::zero() {
        // Initial liquidity: sqrt(amount0 * amount1) - MINIMUM_LIQUIDITY
        let product = amount0 * amount1;
        liquidity = sqrt(product) - U256::from(MINIMUM_LIQUIDITY);
        // Permanently lock the first MINIMUM_LIQUIDITY tokens
        mint_lp(&Key::Hash([0u8; 32]), U256::from(MINIMUM_LIQUIDITY));
    } else {
        // Liquidity = min((amount0 * totalSupply) / reserve0, (amount1 * totalSupply) / reserve1)
        let liquidity0 = (amount0 * total_supply) / reserve0;
        let liquidity1 = (amount1 * total_supply) / reserve1;
        liquidity = min(liquidity0, liquidity1);
    }

    if liquidity == U256::zero() {
        runtime::revert(casper_types::ApiError::User(ERROR_INSUFFICIENT_LIQUIDITY_MINTED));
    }

    mint_lp(&to, liquidity);
    update_reserves(balance0, balance1);

    unlock();
    runtime::ret(CLValue::from_t(liquidity).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn burn() {
    lock();

    let to: Key = runtime::get_named_arg("to");

    let token0: Key = read_from_uref(TOKEN0);
    let token1: Key = read_from_uref(TOKEN1);

    // Get this contract's key
    let self_key = runtime::get_key("ectoplasm_pair_contract").unwrap_or_revert();

    let balance0 = get_token_balance(token0, self_key);
    let balance1 = get_token_balance(token1, self_key);

    // Get LP tokens held by this contract (sent by caller before calling burn)
    let liquidity = read_lp_balance(&self_key);

    let total_supply: U256 = read_from_uref(LP_TOTAL_SUPPLY);

    // Calculate amounts to return
    let amount0 = (liquidity * balance0) / total_supply;
    let amount1 = (liquidity * balance1) / total_supply;

    if amount0 == U256::zero() || amount1 == U256::zero() {
        runtime::revert(casper_types::ApiError::User(ERROR_INSUFFICIENT_LIQUIDITY_BURNED));
    }

    // Burn LP tokens
    burn_lp(&self_key, liquidity);

    // Transfer tokens to recipient
    transfer_token(token0, to, amount0);
    transfer_token(token1, to, amount1);

    // Update reserves
    let new_balance0 = get_token_balance(token0, self_key);
    let new_balance1 = get_token_balance(token1, self_key);
    update_reserves(new_balance0, new_balance1);

    unlock();
    runtime::ret(CLValue::from_t((amount0, amount1)).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn swap() {
    lock();

    let amount0_out: U256 = runtime::get_named_arg("amount0_out");
    let amount1_out: U256 = runtime::get_named_arg("amount1_out");
    let to: Key = runtime::get_named_arg("to");

    if amount0_out == U256::zero() && amount1_out == U256::zero() {
        runtime::revert(casper_types::ApiError::User(ERROR_INSUFFICIENT_OUTPUT_AMOUNT));
    }

    let (reserve0, reserve1, _) = get_reserves();

    if amount0_out >= reserve0 || amount1_out >= reserve1 {
        runtime::revert(casper_types::ApiError::User(ERROR_INSUFFICIENT_LIQUIDITY));
    }

    let token0: Key = read_from_uref(TOKEN0);
    let token1: Key = read_from_uref(TOKEN1);

    // Optimistically transfer tokens
    if amount0_out > U256::zero() {
        transfer_token(token0, to, amount0_out);
    }
    if amount1_out > U256::zero() {
        transfer_token(token1, to, amount1_out);
    }

    // Get this contract's key
    let self_key = runtime::get_key("ectoplasm_pair_contract").unwrap_or_revert();

    let balance0 = get_token_balance(token0, self_key);
    let balance1 = get_token_balance(token1, self_key);

    // Calculate amounts in
    let amount0_in = if balance0 > reserve0 - amount0_out {
        balance0 - (reserve0 - amount0_out)
    } else {
        U256::zero()
    };
    let amount1_in = if balance1 > reserve1 - amount1_out {
        balance1 - (reserve1 - amount1_out)
    } else {
        U256::zero()
    };

    if amount0_in == U256::zero() && amount1_in == U256::zero() {
        runtime::revert(casper_types::ApiError::User(ERROR_INSUFFICIENT_INPUT_AMOUNT));
    }

    // Check K invariant with 0.3% fee
    // (balance0 * 1000 - amount0In * 3) * (balance1 * 1000 - amount1In * 3) >= reserve0 * reserve1 * 1000^2
    let balance0_adjusted = balance0 * 1000 - amount0_in * 3;
    let balance1_adjusted = balance1 * 1000 - amount1_in * 3;

    let k_before = reserve0 * reserve1 * 1000 * 1000;
    let k_after = balance0_adjusted * balance1_adjusted;

    if k_after < k_before {
        runtime::revert(casper_types::ApiError::User(ERROR_K));
    }

    update_reserves(balance0, balance1);

    unlock();
}

#[no_mangle]
pub extern "C" fn sync() {
    let token0: Key = read_from_uref(TOKEN0);
    let token1: Key = read_from_uref(TOKEN1);
    let self_key = runtime::get_key("ectoplasm_pair_contract").unwrap_or_revert();

    let balance0 = get_token_balance(token0, self_key);
    let balance1 = get_token_balance(token1, self_key);

    update_reserves(balance0, balance1);
}

#[no_mangle]
pub extern "C" fn skim() {
    let to: Key = runtime::get_named_arg("to");

    let token0: Key = read_from_uref(TOKEN0);
    let token1: Key = read_from_uref(TOKEN1);
    let self_key = runtime::get_key("ectoplasm_pair_contract").unwrap_or_revert();

    let (reserve0, reserve1, _) = get_reserves();
    let balance0 = get_token_balance(token0, self_key);
    let balance1 = get_token_balance(token1, self_key);

    if balance0 > reserve0 {
        transfer_token(token0, to, balance0 - reserve0);
    }
    if balance1 > reserve1 {
        transfer_token(token1, to, balance1 - reserve1);
    }
}

// ============ Contract Installation ============

fn get_entry_points() -> EntryPoints {
    let mut ep = EntryPoints::new();

    // LP Token entry points
    ep.add_entry_point(EntryPoint::new("name", vec![], CLType::String, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("symbol", vec![], CLType::String, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("decimals", vec![], CLType::U8, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("total_supply", vec![], CLType::U256, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("balance_of", vec![Parameter::new("owner", CLType::Key)], CLType::U256, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("allowance", vec![Parameter::new("owner", CLType::Key), Parameter::new("spender", CLType::Key)], CLType::U256, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("transfer", vec![Parameter::new("recipient", CLType::Key), Parameter::new("amount", CLType::U256)], CLType::Unit, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("transfer_from", vec![Parameter::new("owner", CLType::Key), Parameter::new("recipient", CLType::Key), Parameter::new("amount", CLType::U256)], CLType::Unit, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("approve", vec![Parameter::new("spender", CLType::Key), Parameter::new("amount", CLType::U256)], CLType::Unit, EntryPointAccess::Public, EntryPointType::Contract));

    // AMM entry points
    ep.add_entry_point(EntryPoint::new("token0", vec![], CLType::Key, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("token1", vec![], CLType::Key, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("factory", vec![], CLType::Key, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("get_reserves", vec![], CLType::Tuple3([Box::new(CLType::U256), Box::new(CLType::U256), Box::new(CLType::U64)]), EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("mint", vec![Parameter::new("to", CLType::Key)], CLType::U256, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("burn", vec![Parameter::new("to", CLType::Key)], CLType::Tuple2([Box::new(CLType::U256), Box::new(CLType::U256)]), EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("swap", vec![Parameter::new("amount0_out", CLType::U256), Parameter::new("amount1_out", CLType::U256), Parameter::new("to", CLType::Key)], CLType::Unit, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("sync", vec![], CLType::Unit, EntryPointAccess::Public, EntryPointType::Contract));
    ep.add_entry_point(EntryPoint::new("skim", vec![Parameter::new("to", CLType::Key)], CLType::Unit, EntryPointAccess::Public, EntryPointType::Contract));

    ep
}

#[no_mangle]
pub extern "C" fn call() {
    let token0: Key = runtime::get_named_arg("token0");
    let token1: Key = runtime::get_named_arg("token1");
    let factory: Key = runtime::get_named_arg("factory");

    let mut named_keys = NamedKeys::new();

    // AMM storage
    named_keys.insert(TOKEN0.to_string(), storage::new_uref(token0).into());
    named_keys.insert(TOKEN1.to_string(), storage::new_uref(token1).into());
    named_keys.insert(FACTORY.to_string(), storage::new_uref(factory).into());
    named_keys.insert(RESERVE0.to_string(), storage::new_uref(U256::zero()).into());
    named_keys.insert(RESERVE1.to_string(), storage::new_uref(U256::zero()).into());
    named_keys.insert(BLOCK_TIMESTAMP_LAST.to_string(), storage::new_uref(0u64).into());
    named_keys.insert(PRICE0_CUMULATIVE_LAST.to_string(), storage::new_uref(U256::zero()).into());
    named_keys.insert(PRICE1_CUMULATIVE_LAST.to_string(), storage::new_uref(U256::zero()).into());
    named_keys.insert(K_LAST.to_string(), storage::new_uref(U256::zero()).into());
    named_keys.insert(LOCKED.to_string(), storage::new_uref(false).into());

    // LP Token storage
    named_keys.insert(LP_NAME.to_string(), storage::new_uref(String::from("Ectoplasm LP Token")).into());
    named_keys.insert(LP_SYMBOL.to_string(), storage::new_uref(String::from("ECTO-LP")).into());
    named_keys.insert(LP_DECIMALS.to_string(), storage::new_uref(18u8).into());
    named_keys.insert(LP_TOTAL_SUPPLY.to_string(), storage::new_uref(U256::zero()).into());

    let balances_dict = storage::new_dictionary(LP_BALANCES).unwrap_or_revert();
    named_keys.insert(LP_BALANCES.to_string(), balances_dict.into());

    let allowances_dict = storage::new_dictionary(LP_ALLOWANCES).unwrap_or_revert();
    named_keys.insert(LP_ALLOWANCES.to_string(), allowances_dict.into());

    let (contract_hash, _) = storage::new_contract(
        get_entry_points(),
        Some(named_keys),
        Some("ectoplasm_pair_package".to_string()),
        Some("ectoplasm_pair_access".to_string()),
    );

    runtime::put_key("ectoplasm_pair_contract", contract_hash.into());
}
