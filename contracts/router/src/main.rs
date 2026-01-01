#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use casper_contract::{
    contract_api::{runtime, storage},
    unwrap_or_revert::UnwrapOrRevert,
};
use casper_types::{
    addressable_entity::{EntityEntryPoint as EntryPoint, EntryPoints},
    bytesrepr::{FromBytes, ToBytes},
    contracts::{ContractHash, NamedKeys},
    runtime_args, RuntimeArgs,
    CLType, CLTyped, CLValue, EntryPointAccess, EntryPointPayment, EntryPointType, Key, Parameter, U256,
};

// Storage keys
const FACTORY: &str = "factory";

// Error codes
const ERROR_EXPIRED: u16 = 1;
const ERROR_INSUFFICIENT_A_AMOUNT: u16 = 2;
const ERROR_INSUFFICIENT_B_AMOUNT: u16 = 3;
const ERROR_INSUFFICIENT_OUTPUT_AMOUNT: u16 = 4;
const ERROR_EXCESSIVE_INPUT_AMOUNT: u16 = 5;
const ERROR_INVALID_PATH: u16 = 6;
const ERROR_PAIR_NOT_FOUND: u16 = 7;
const ERROR_INSUFFICIENT_LIQUIDITY: u16 = 8;

// ============ Helper Functions ============

fn read_from_uref<T: CLTyped + FromBytes>(name: &str) -> T {
    let key = runtime::get_key(name).unwrap_or_revert();
    let uref = key.into_uref().unwrap_or_revert();
    storage::read(uref).unwrap_or_revert().unwrap_or_revert()
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

fn sort_tokens(token_a: Key, token_b: Key) -> (Key, Key) {
    let key_a = key_to_str(&token_a);
    let key_b = key_to_str(&token_b);

    if key_a < key_b {
        (token_a, token_b)
    } else {
        (token_b, token_a)
    }
}

fn get_contract_hash(key: Key) -> ContractHash {
    match key {
        Key::Hash(hash) => ContractHash::new(hash),
        _ => runtime::revert(casper_types::ApiError::User(ERROR_PAIR_NOT_FOUND)),
    }
}

// ============ External Contract Calls ============

fn call_factory_get_pair(factory: Key, token_a: Key, token_b: Key) -> Option<Key> {
    let contract_hash = get_contract_hash(factory);
    runtime::call_contract(
        contract_hash,
        "get_pair",
        runtime_args! {
            "token_a" => token_a,
            "token_b" => token_b
        },
    )
}

fn call_pair_get_reserves(pair: Key) -> (U256, U256, u64) {
    let contract_hash = get_contract_hash(pair);
    runtime::call_contract(
        contract_hash,
        "get_reserves",
        runtime_args! {},
    )
}

fn call_pair_token0(pair: Key) -> Key {
    let contract_hash = get_contract_hash(pair);
    runtime::call_contract(
        contract_hash,
        "token0",
        runtime_args! {},
    )
}

fn call_pair_mint(pair: Key, to: Key) -> U256 {
    let contract_hash = get_contract_hash(pair);
    runtime::call_contract(
        contract_hash,
        "mint",
        runtime_args! {
            "to" => to
        },
    )
}

fn call_pair_burn(pair: Key, to: Key) -> (U256, U256) {
    let contract_hash = get_contract_hash(pair);
    runtime::call_contract(
        contract_hash,
        "burn",
        runtime_args! {
            "to" => to
        },
    )
}

fn call_pair_swap(pair: Key, amount0_out: U256, amount1_out: U256, to: Key) {
    let contract_hash = get_contract_hash(pair);
    runtime::call_contract::<()>(
        contract_hash,
        "swap",
        runtime_args! {
            "amount0_out" => amount0_out,
            "amount1_out" => amount1_out,
            "to" => to
        },
    );
}

fn call_token_transfer(token: Key, recipient: Key, amount: U256) {
    let contract_hash = get_contract_hash(token);
    runtime::call_contract::<()>(
        contract_hash,
        "transfer",
        runtime_args! {
            "recipient" => recipient,
            "amount" => amount
        },
    );
}

fn call_token_transfer_from(token: Key, owner: Key, recipient: Key, amount: U256) {
    let contract_hash = get_contract_hash(token);
    runtime::call_contract::<()>(
        contract_hash,
        "transfer_from",
        runtime_args! {
            "owner" => owner,
            "recipient" => recipient,
            "amount" => amount
        },
    );
}

// ============ Library Functions ============

/// Given some asset amount and reserves, returns an equivalent amount of the other asset
fn quote_internal(amount_a: U256, reserve_a: U256, reserve_b: U256) -> U256 {
    if amount_a == U256::zero() || reserve_a == U256::zero() {
        return U256::zero();
    }
    (amount_a * reserve_b) / reserve_a
}

/// Given an input amount and reserves, returns the maximum output amount
fn get_amount_out_internal(amount_in: U256, reserve_in: U256, reserve_out: U256) -> U256 {
    if amount_in == U256::zero() || reserve_in == U256::zero() || reserve_out == U256::zero() {
        return U256::zero();
    }
    let amount_in_with_fee = amount_in * 997;
    let numerator = amount_in_with_fee * reserve_out;
    let denominator = reserve_in * 1000 + amount_in_with_fee;
    numerator / denominator
}

/// Given an output amount and reserves, returns the required input amount
fn get_amount_in_internal(amount_out: U256, reserve_in: U256, reserve_out: U256) -> U256 {
    if amount_out == U256::zero() || reserve_in == U256::zero() || reserve_out == U256::zero() {
        return U256::zero();
    }
    let numerator = reserve_in * amount_out * 1000;
    let denominator = (reserve_out - amount_out) * 997;
    (numerator / denominator) + 1
}

/// Get reserves for a pair, sorted by token order
fn get_reserves_sorted(factory: Key, token_a: Key, token_b: Key) -> (U256, U256) {
    let pair = call_factory_get_pair(factory, token_a, token_b);
    if pair.is_none() {
        runtime::revert(casper_types::ApiError::User(ERROR_PAIR_NOT_FOUND));
    }
    let pair = pair.unwrap_or_revert();

    let (reserve0, reserve1, _) = call_pair_get_reserves(pair);
    let token0 = call_pair_token0(pair);

    if key_to_str(&token_a) == key_to_str(&token0) {
        (reserve0, reserve1)
    } else {
        (reserve1, reserve0)
    }
}

/// Calculate optimal amounts for adding liquidity
fn calculate_liquidity_amounts(
    amount_a_desired: U256,
    amount_b_desired: U256,
    amount_a_min: U256,
    amount_b_min: U256,
    reserve_a: U256,
    reserve_b: U256,
) -> (U256, U256) {
    if reserve_a == U256::zero() && reserve_b == U256::zero() {
        return (amount_a_desired, amount_b_desired);
    }

    let amount_b_optimal = quote_internal(amount_a_desired, reserve_a, reserve_b);
    if amount_b_optimal <= amount_b_desired {
        if amount_b_optimal < amount_b_min {
            runtime::revert(casper_types::ApiError::User(ERROR_INSUFFICIENT_B_AMOUNT));
        }
        (amount_a_desired, amount_b_optimal)
    } else {
        let amount_a_optimal = quote_internal(amount_b_desired, reserve_b, reserve_a);
        if amount_a_optimal > amount_a_desired {
            runtime::revert(casper_types::ApiError::User(ERROR_INSUFFICIENT_A_AMOUNT));
        }
        if amount_a_optimal < amount_a_min {
            runtime::revert(casper_types::ApiError::User(ERROR_INSUFFICIENT_A_AMOUNT));
        }
        (amount_a_optimal, amount_b_desired)
    }
}

// ============ Entry Points ============

#[no_mangle]
pub extern "C" fn factory() {
    let factory: Key = read_from_uref(FACTORY);
    runtime::ret(CLValue::from_t(factory).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn quote() {
    let amount_a: U256 = runtime::get_named_arg("amount_a");
    let reserve_a: U256 = runtime::get_named_arg("reserve_a");
    let reserve_b: U256 = runtime::get_named_arg("reserve_b");
    let result = quote_internal(amount_a, reserve_a, reserve_b);
    runtime::ret(CLValue::from_t(result).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn get_amount_out() {
    let amount_in: U256 = runtime::get_named_arg("amount_in");
    let reserve_in: U256 = runtime::get_named_arg("reserve_in");
    let reserve_out: U256 = runtime::get_named_arg("reserve_out");
    let result = get_amount_out_internal(amount_in, reserve_in, reserve_out);
    runtime::ret(CLValue::from_t(result).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn get_amount_in() {
    let amount_out: U256 = runtime::get_named_arg("amount_out");
    let reserve_in: U256 = runtime::get_named_arg("reserve_in");
    let reserve_out: U256 = runtime::get_named_arg("reserve_out");
    let result = get_amount_in_internal(amount_out, reserve_in, reserve_out);
    runtime::ret(CLValue::from_t(result).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn get_amounts_out() {
    let factory: Key = read_from_uref(FACTORY);
    let amount_in: U256 = runtime::get_named_arg("amount_in");
    let path: Vec<Key> = runtime::get_named_arg("path");

    if path.len() < 2 {
        runtime::revert(casper_types::ApiError::User(ERROR_INVALID_PATH));
    }

    let mut amounts = vec![amount_in];
    for i in 0..(path.len() - 1) {
        let (reserve_in, reserve_out) = get_reserves_sorted(factory, path[i], path[i + 1]);
        amounts.push(get_amount_out_internal(amounts[i], reserve_in, reserve_out));
    }

    runtime::ret(CLValue::from_t(amounts).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn get_amounts_in() {
    let factory: Key = read_from_uref(FACTORY);
    let amount_out: U256 = runtime::get_named_arg("amount_out");
    let path: Vec<Key> = runtime::get_named_arg("path");

    if path.len() < 2 {
        runtime::revert(casper_types::ApiError::User(ERROR_INVALID_PATH));
    }

    let mut amounts = vec![U256::zero(); path.len()];
    amounts[path.len() - 1] = amount_out;

    for i in (1..path.len()).rev() {
        let (reserve_in, reserve_out) = get_reserves_sorted(factory, path[i - 1], path[i]);
        amounts[i - 1] = get_amount_in_internal(amounts[i], reserve_in, reserve_out);
    }

    runtime::ret(CLValue::from_t(amounts).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn add_liquidity() {
    let factory: Key = read_from_uref(FACTORY);
    let token_a: Key = runtime::get_named_arg("token_a");
    let token_b: Key = runtime::get_named_arg("token_b");
    let amount_a_desired: U256 = runtime::get_named_arg("amount_a_desired");
    let amount_b_desired: U256 = runtime::get_named_arg("amount_b_desired");
    let amount_a_min: U256 = runtime::get_named_arg("amount_a_min");
    let amount_b_min: U256 = runtime::get_named_arg("amount_b_min");
    let to: Key = runtime::get_named_arg("to");
    let deadline: u64 = runtime::get_named_arg("deadline");

    // Note: deadline check would need block timestamp access

    // Get pair
    let pair = call_factory_get_pair(factory, token_a, token_b);
    if pair.is_none() {
        runtime::revert(casper_types::ApiError::User(ERROR_PAIR_NOT_FOUND));
    }
    let pair = pair.unwrap_or_revert();

    // Calculate optimal amounts
    let (reserve_a, reserve_b) = get_reserves_sorted(factory, token_a, token_b);
    let (amount_a, amount_b) = calculate_liquidity_amounts(
        amount_a_desired,
        amount_b_desired,
        amount_a_min,
        amount_b_min,
        reserve_a,
        reserve_b,
    );

    // Transfer tokens from sender to pair
    let sender = Key::Account(runtime::get_caller());
    call_token_transfer_from(token_a, sender, pair, amount_a);
    call_token_transfer_from(token_b, sender, pair, amount_b);

    // Mint LP tokens
    let liquidity = call_pair_mint(pair, to);

    runtime::ret(CLValue::from_t((amount_a, amount_b, liquidity)).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn remove_liquidity() {
    let factory: Key = read_from_uref(FACTORY);
    let token_a: Key = runtime::get_named_arg("token_a");
    let token_b: Key = runtime::get_named_arg("token_b");
    let liquidity: U256 = runtime::get_named_arg("liquidity");
    let amount_a_min: U256 = runtime::get_named_arg("amount_a_min");
    let amount_b_min: U256 = runtime::get_named_arg("amount_b_min");
    let to: Key = runtime::get_named_arg("to");
    let deadline: u64 = runtime::get_named_arg("deadline");

    // Get pair
    let pair = call_factory_get_pair(factory, token_a, token_b);
    if pair.is_none() {
        runtime::revert(casper_types::ApiError::User(ERROR_PAIR_NOT_FOUND));
    }
    let pair = pair.unwrap_or_revert();

    // Transfer LP tokens from sender to pair
    let sender = Key::Account(runtime::get_caller());
    call_token_transfer_from(pair, sender, pair, liquidity);

    // Burn LP tokens and receive underlying tokens
    let (amount0, amount1) = call_pair_burn(pair, to);

    // Sort amounts according to token order
    let token0 = call_pair_token0(pair);
    let (amount_a, amount_b) = if key_to_str(&token_a) == key_to_str(&token0) {
        (amount0, amount1)
    } else {
        (amount1, amount0)
    };

    if amount_a < amount_a_min {
        runtime::revert(casper_types::ApiError::User(ERROR_INSUFFICIENT_A_AMOUNT));
    }
    if amount_b < amount_b_min {
        runtime::revert(casper_types::ApiError::User(ERROR_INSUFFICIENT_B_AMOUNT));
    }

    runtime::ret(CLValue::from_t((amount_a, amount_b)).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn swap_exact_tokens_for_tokens() {
    let factory: Key = read_from_uref(FACTORY);
    let amount_in: U256 = runtime::get_named_arg("amount_in");
    let amount_out_min: U256 = runtime::get_named_arg("amount_out_min");
    let path: Vec<Key> = runtime::get_named_arg("path");
    let to: Key = runtime::get_named_arg("to");
    let deadline: u64 = runtime::get_named_arg("deadline");

    if path.len() < 2 {
        runtime::revert(casper_types::ApiError::User(ERROR_INVALID_PATH));
    }

    // Calculate amounts
    let mut amounts = vec![amount_in];
    for i in 0..(path.len() - 1) {
        let (reserve_in, reserve_out) = get_reserves_sorted(factory, path[i], path[i + 1]);
        amounts.push(get_amount_out_internal(amounts[i], reserve_in, reserve_out));
    }

    if amounts[amounts.len() - 1] < amount_out_min {
        runtime::revert(casper_types::ApiError::User(ERROR_INSUFFICIENT_OUTPUT_AMOUNT));
    }

    // Transfer input tokens from sender to first pair
    let sender = Key::Account(runtime::get_caller());
    let first_pair = call_factory_get_pair(factory, path[0], path[1]).unwrap_or_revert();
    call_token_transfer_from(path[0], sender, first_pair, amounts[0]);

    // Execute swaps
    for i in 0..(path.len() - 1) {
        let (input, output) = (path[i], path[i + 1]);
        let pair = call_factory_get_pair(factory, input, output).unwrap_or_revert();
        let token0 = call_pair_token0(pair);

        let amount_out = amounts[i + 1];
        let (amount0_out, amount1_out) = if key_to_str(&input) == key_to_str(&token0) {
            (U256::zero(), amount_out)
        } else {
            (amount_out, U256::zero())
        };

        // Determine recipient: next pair or final recipient
        let recipient = if i < path.len() - 2 {
            call_factory_get_pair(factory, output, path[i + 2]).unwrap_or_revert()
        } else {
            to
        };

        call_pair_swap(pair, amount0_out, amount1_out, recipient);
    }

    runtime::ret(CLValue::from_t(amounts).unwrap_or_revert());
}

#[no_mangle]
pub extern "C" fn swap_tokens_for_exact_tokens() {
    let factory: Key = read_from_uref(FACTORY);
    let amount_out: U256 = runtime::get_named_arg("amount_out");
    let amount_in_max: U256 = runtime::get_named_arg("amount_in_max");
    let path: Vec<Key> = runtime::get_named_arg("path");
    let to: Key = runtime::get_named_arg("to");
    let deadline: u64 = runtime::get_named_arg("deadline");

    if path.len() < 2 {
        runtime::revert(casper_types::ApiError::User(ERROR_INVALID_PATH));
    }

    // Calculate amounts backwards
    let mut amounts = vec![U256::zero(); path.len()];
    amounts[path.len() - 1] = amount_out;

    for i in (1..path.len()).rev() {
        let (reserve_in, reserve_out) = get_reserves_sorted(factory, path[i - 1], path[i]);
        amounts[i - 1] = get_amount_in_internal(amounts[i], reserve_in, reserve_out);
    }

    if amounts[0] > amount_in_max {
        runtime::revert(casper_types::ApiError::User(ERROR_EXCESSIVE_INPUT_AMOUNT));
    }

    // Transfer input tokens from sender to first pair
    let sender = Key::Account(runtime::get_caller());
    let first_pair = call_factory_get_pair(factory, path[0], path[1]).unwrap_or_revert();
    call_token_transfer_from(path[0], sender, first_pair, amounts[0]);

    // Execute swaps
    for i in 0..(path.len() - 1) {
        let (input, output) = (path[i], path[i + 1]);
        let pair = call_factory_get_pair(factory, input, output).unwrap_or_revert();
        let token0 = call_pair_token0(pair);

        let amount_out_swap = amounts[i + 1];
        let (amount0_out, amount1_out) = if key_to_str(&input) == key_to_str(&token0) {
            (U256::zero(), amount_out_swap)
        } else {
            (amount_out_swap, U256::zero())
        };

        let recipient = if i < path.len() - 2 {
            call_factory_get_pair(factory, output, path[i + 2]).unwrap_or_revert()
        } else {
            to
        };

        call_pair_swap(pair, amount0_out, amount1_out, recipient);
    }

    runtime::ret(CLValue::from_t(amounts).unwrap_or_revert());
}

// ============ Contract Installation ============

fn get_entry_points() -> EntryPoints {
    let mut ep = EntryPoints::new();

    ep.add_entry_point(EntryPoint::new("factory", vec![], CLType::Key, EntryPointAccess::Public, EntryPointType::Called, EntryPointPayment::Caller));

    ep.add_entry_point(EntryPoint::new(
        "quote",
        vec![
            Parameter::new("amount_a", CLType::U256),
            Parameter::new("reserve_a", CLType::U256),
            Parameter::new("reserve_b", CLType::U256),
        ],
        CLType::U256,
        EntryPointAccess::Public, EntryPointType::Called, EntryPointPayment::Caller,
    ));

    ep.add_entry_point(EntryPoint::new(
        "get_amount_out",
        vec![
            Parameter::new("amount_in", CLType::U256),
            Parameter::new("reserve_in", CLType::U256),
            Parameter::new("reserve_out", CLType::U256),
        ],
        CLType::U256,
        EntryPointAccess::Public, EntryPointType::Called, EntryPointPayment::Caller,
    ));

    ep.add_entry_point(EntryPoint::new(
        "get_amount_in",
        vec![
            Parameter::new("amount_out", CLType::U256),
            Parameter::new("reserve_in", CLType::U256),
            Parameter::new("reserve_out", CLType::U256),
        ],
        CLType::U256,
        EntryPointAccess::Public, EntryPointType::Called, EntryPointPayment::Caller,
    ));

    ep.add_entry_point(EntryPoint::new(
        "get_amounts_out",
        vec![
            Parameter::new("amount_in", CLType::U256),
            Parameter::new("path", CLType::List(Box::new(CLType::Key))),
        ],
        CLType::List(Box::new(CLType::U256)),
        EntryPointAccess::Public, EntryPointType::Called, EntryPointPayment::Caller,
    ));

    ep.add_entry_point(EntryPoint::new(
        "get_amounts_in",
        vec![
            Parameter::new("amount_out", CLType::U256),
            Parameter::new("path", CLType::List(Box::new(CLType::Key))),
        ],
        CLType::List(Box::new(CLType::U256)),
        EntryPointAccess::Public, EntryPointType::Called, EntryPointPayment::Caller,
    ));

    ep.add_entry_point(EntryPoint::new(
        "add_liquidity",
        vec![
            Parameter::new("token_a", CLType::Key),
            Parameter::new("token_b", CLType::Key),
            Parameter::new("amount_a_desired", CLType::U256),
            Parameter::new("amount_b_desired", CLType::U256),
            Parameter::new("amount_a_min", CLType::U256),
            Parameter::new("amount_b_min", CLType::U256),
            Parameter::new("to", CLType::Key),
            Parameter::new("deadline", CLType::U64),
        ],
        CLType::Tuple3([Box::new(CLType::U256), Box::new(CLType::U256), Box::new(CLType::U256)]),
        EntryPointAccess::Public, EntryPointType::Called, EntryPointPayment::Caller,
    ));

    ep.add_entry_point(EntryPoint::new(
        "remove_liquidity",
        vec![
            Parameter::new("token_a", CLType::Key),
            Parameter::new("token_b", CLType::Key),
            Parameter::new("liquidity", CLType::U256),
            Parameter::new("amount_a_min", CLType::U256),
            Parameter::new("amount_b_min", CLType::U256),
            Parameter::new("to", CLType::Key),
            Parameter::new("deadline", CLType::U64),
        ],
        CLType::Tuple2([Box::new(CLType::U256), Box::new(CLType::U256)]),
        EntryPointAccess::Public, EntryPointType::Called, EntryPointPayment::Caller,
    ));

    ep.add_entry_point(EntryPoint::new(
        "swap_exact_tokens_for_tokens",
        vec![
            Parameter::new("amount_in", CLType::U256),
            Parameter::new("amount_out_min", CLType::U256),
            Parameter::new("path", CLType::List(Box::new(CLType::Key))),
            Parameter::new("to", CLType::Key),
            Parameter::new("deadline", CLType::U64),
        ],
        CLType::List(Box::new(CLType::U256)),
        EntryPointAccess::Public, EntryPointType::Called, EntryPointPayment::Caller,
    ));

    ep.add_entry_point(EntryPoint::new(
        "swap_tokens_for_exact_tokens",
        vec![
            Parameter::new("amount_out", CLType::U256),
            Parameter::new("amount_in_max", CLType::U256),
            Parameter::new("path", CLType::List(Box::new(CLType::Key))),
            Parameter::new("to", CLType::Key),
            Parameter::new("deadline", CLType::U64),
        ],
        CLType::List(Box::new(CLType::U256)),
        EntryPointAccess::Public, EntryPointType::Called, EntryPointPayment::Caller,
    ));

    ep
}

#[no_mangle]
pub extern "C" fn call() {
    let factory: Key = runtime::get_named_arg("factory");

    let mut named_keys = NamedKeys::new();
    named_keys.insert(FACTORY.to_string(), storage::new_uref(factory).into());

    let (contract_hash, _) = storage::new_contract(
        get_entry_points(),
        Some(named_keys),
        Some("ectoplasm_router_package".to_string()),
        Some("ectoplasm_router_access".to_string()),
        None,
    );

    runtime::put_key("ectoplasm_router_contract", contract_hash.into());
}
