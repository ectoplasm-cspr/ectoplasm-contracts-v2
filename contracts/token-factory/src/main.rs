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
    addressable_entity::{AddressableEntityHash, EntityEntryPoint as EntryPoint, EntryPoints},
    bytesrepr::{FromBytes, ToBytes},
    contracts::NamedKeys,
    runtime_args, CLType, CLTyped, CLValue, EntryPointAccess, EntryPointPayment,
    EntryPointType, Key, Parameter, RuntimeArgs, URef, U256, U512,
};

// ============ Storage Keys ============

const CONTROLLER: &str = "controller";
const PLATFORM_WALLET: &str = "platform_wallet";
const LAUNCHES: &str = "launches";
const LAUNCHES_META: &str = "launches_meta";
const TOKEN_TO_LAUNCH: &str = "token_to_launch";
const LAUNCH_COUNT: &str = "launch_count";
const INITIALIZED: &str = "initialized";

// Default curve parameters
const DEFAULT_BASE_PRICE: u64 = 1_000_000_000; // 1 CSPR per token
const DEFAULT_MAX_PRICE: u64 = 100_000_000_000; // 100 CSPR per token
const DEFAULT_TOTAL_SUPPLY: u128 = 1_000_000_000_000_000_000_000_000; // 1 million tokens (18 decimals)
const TOKEN_DECIMALS: u8 = 18;

// ============ Error Codes ============

const ERROR_ALREADY_INITIALIZED: u16 = 1;
const ERROR_NOT_INITIALIZED: u16 = 2;
const ERROR_INVALID_CURVE_TYPE: u16 = 3;
const ERROR_INVALID_SYMBOL: u16 = 4;
const ERROR_INVALID_NAME: u16 = 5;
const ERROR_LAUNCH_NOT_FOUND: u16 = 6;
const ERROR_INDEX_OUT_OF_BOUNDS: u16 = 7;
const ERROR_FAILED_TO_CREATE_DICTIONARY: u16 = 8;

// ============ Launch Status ============

const STATUS_ACTIVE: u8 = 0;
const STATUS_GRADUATED: u8 = 1;
const STATUS_REFUNDING: u8 = 2;

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
    runtime::get_key(name)
        .unwrap_or_revert()
        .into_uref()
        .unwrap_or_revert()
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

fn get_current_time() -> u64 {
    runtime::get_blocktime().into()
}

// ============ Entry Points ============

/// Initialize the factory (creates dictionaries)
#[no_mangle]
pub extern "C" fn init() {
    let initialized: bool = read_from_uref(INITIALIZED);
    if initialized {
        runtime::revert(casper_types::ApiError::User(ERROR_ALREADY_INITIALIZED));
    }

    // Create dictionaries
    storage::new_dictionary(LAUNCHES)
        .unwrap_or_revert_with(casper_types::ApiError::User(ERROR_FAILED_TO_CREATE_DICTIONARY));
    storage::new_dictionary(LAUNCHES_META)
        .unwrap_or_revert_with(casper_types::ApiError::User(ERROR_FAILED_TO_CREATE_DICTIONARY));
    storage::new_dictionary(TOKEN_TO_LAUNCH)
        .unwrap_or_revert_with(casper_types::ApiError::User(ERROR_FAILED_TO_CREATE_DICTIONARY));

    write_to_uref(INITIALIZED, true);
}

/// Get the controller contract address
#[no_mangle]
pub extern "C" fn controller() {
    let controller: Key = read_from_uref(CONTROLLER);
    runtime::ret(CLValue::from_t(controller).unwrap_or_revert());
}

/// Get the total number of launches
#[no_mangle]
pub extern "C" fn launch_count() {
    let count: u64 = read_from_uref(LAUNCH_COUNT);
    runtime::ret(CLValue::from_t(count).unwrap_or_revert());
}

/// Get launch info by ID
/// Returns tuple: (token_hash, curve_hash, creator)
#[no_mangle]
pub extern "C" fn get_launch() {
    let launch_id: u64 = runtime::get_named_arg("launch_id");
    let count: u64 = read_from_uref(LAUNCH_COUNT);

    if launch_id >= count {
        runtime::revert(casper_types::ApiError::User(ERROR_INDEX_OUT_OF_BOUNDS));
    }

    let launches_uref = get_dictionary_uref(LAUNCHES);
    let launch_key = launch_id.to_string();

    // Launch core data: (token, curve, creator)
    let core_data: Option<(Key, Key, Key)> =
        storage::dictionary_get(launches_uref, &launch_key).unwrap_or_default();

    runtime::ret(CLValue::from_t(core_data).unwrap_or_revert());
}

/// Get launch metadata by ID
/// Returns nested tuple: (name, symbol, (curve_type, status, created_at))
#[no_mangle]
pub extern "C" fn get_launch_meta() {
    let launch_id: u64 = runtime::get_named_arg("launch_id");
    let count: u64 = read_from_uref(LAUNCH_COUNT);

    if launch_id >= count {
        runtime::revert(casper_types::ApiError::User(ERROR_INDEX_OUT_OF_BOUNDS));
    }

    let meta_uref = get_dictionary_uref(LAUNCHES_META);
    let launch_key = launch_id.to_string();

    // Metadata: (name, symbol, (curve_type, status, created_at))
    let meta_data: Option<(String, String, (u8, u8, u64))> =
        storage::dictionary_get(meta_uref, &launch_key).unwrap_or_default();

    runtime::ret(CLValue::from_t(meta_data).unwrap_or_revert());
}

/// Get launch ID by token hash
#[no_mangle]
pub extern "C" fn get_launch_by_token() {
    let token_hash: Key = runtime::get_named_arg("token_hash");
    let token_key = key_to_str(&token_hash);

    let token_to_launch_uref = get_dictionary_uref(TOKEN_TO_LAUNCH);
    let launch_id: Option<u64> =
        storage::dictionary_get(token_to_launch_uref, &token_key).unwrap_or_default();

    runtime::ret(CLValue::from_t(launch_id).unwrap_or_revert());
}

/// Get multiple launches (paginated)
/// Returns array of launch IDs in the specified range
#[no_mangle]
pub extern "C" fn get_launches() {
    let offset: u64 = runtime::get_named_arg("offset");
    let limit: u64 = runtime::get_named_arg("limit");
    let _status_filter: Option<u8> = runtime::get_named_arg("status_filter");

    let count: u64 = read_from_uref(LAUNCH_COUNT);

    let mut result: Vec<u64> = Vec::new();
    let mut checked = 0u64;
    let mut index = offset;

    // Simple pagination without status filter for now
    // Status filtering would require additional dictionary lookups
    while checked < limit && index < count {
        result.push(index);
        checked += 1;
        index += 1;
    }

    runtime::ret(CLValue::from_t(result).unwrap_or_revert());
}

/// Create a new token launch
/// This deploys a new CEP-18 token and bonding curve
#[no_mangle]
pub extern "C" fn create_launch() {
    // Get launch parameters
    let name: String = runtime::get_named_arg("name");
    let symbol: String = runtime::get_named_arg("symbol");
    let curve_type: u8 = runtime::get_named_arg("curve_type");

    // Optional overrides
    let graduation_threshold: Option<U512> = runtime::get_named_arg("graduation_threshold");
    let creator_fee_bps: Option<u64> = runtime::get_named_arg("creator_fee_bps");
    let deadline_days: Option<u64> = runtime::get_named_arg("deadline_days");
    let promo_budget: U512 = runtime::get_named_arg::<Option<U512>>("promo_budget")
        .unwrap_or(U512::zero());

    // Optional metadata
    let _description: Option<String> = runtime::get_named_arg("description");
    let _website: Option<String> = runtime::get_named_arg("website");
    let _twitter: Option<String> = runtime::get_named_arg("twitter");

    // Validate inputs
    if name.is_empty() || name.len() > 50 {
        runtime::revert(casper_types::ApiError::User(ERROR_INVALID_NAME));
    }
    if symbol.is_empty() || symbol.len() > 6 {
        runtime::revert(casper_types::ApiError::User(ERROR_INVALID_SYMBOL));
    }
    if curve_type > 2 {
        runtime::revert(casper_types::ApiError::User(ERROR_INVALID_CURVE_TYPE));
    }

    let creator = Key::Account(runtime::get_caller());
    let controller: Key = read_from_uref(CONTROLLER);
    let platform_wallet: Key = read_from_uref(PLATFORM_WALLET);

    // Get defaults from controller
    let (default_threshold, default_fee_bps, default_deadline_days): (U512, u64, u64) =
        if let Key::AddressableEntity(entity_addr) = controller {
            let controller_contract = AddressableEntityHash::new(entity_addr.value());
            runtime::call_contract(controller_contract.into(), "get_defaults", runtime_args! {})
        } else {
            // Fallback defaults
            (U512::from(50_000_000_000_000u64), 100u64, 30u64)
        };

    // Use overrides or defaults
    let final_threshold = graduation_threshold.unwrap_or(default_threshold);
    let final_creator_fee = creator_fee_bps.unwrap_or(0u64); // Creator fee defaults to 0
    let final_deadline_days = deadline_days.unwrap_or(default_deadline_days);

    // Calculate deadline timestamp
    let current_time = get_current_time();
    let deadline = current_time + (final_deadline_days * 24 * 60 * 60 * 1000); // Convert days to milliseconds

    // Deploy token contract
    // Note: In practice, this would use runtime::put_key and stored contract WASM
    // For now, we'll simulate by storing the token data

    let total_supply = U256::from(DEFAULT_TOTAL_SUPPLY);
    let base_price = U512::from(DEFAULT_BASE_PRICE);
    let max_price = U512::from(DEFAULT_MAX_PRICE);

    // Get next launch ID
    let launch_id: u64 = read_from_uref(LAUNCH_COUNT);

    // Create placeholder keys for token and curve
    // In a real implementation, these would be the actual deployed contract hashes
    let token_placeholder = Key::Hash([launch_id as u8; 32]);
    let curve_placeholder = Key::Hash([(launch_id + 128) as u8; 32]);

    // Store launch info (simplified to 3-tuple for CLTyped compatibility)
    let launches_uref = get_dictionary_uref(LAUNCHES);
    let launch_data = (token_placeholder, curve_placeholder, creator);
    storage::dictionary_put(launches_uref, &launch_id.to_string(), launch_data);

    // Store launch metadata (name, symbol, (curve_type, status, created_at))
    let meta_uref = get_dictionary_uref(LAUNCHES_META);
    let meta_data = (name, symbol, (curve_type, STATUS_ACTIVE, current_time));
    storage::dictionary_put(meta_uref, &launch_id.to_string(), meta_data);

    // Map token to launch
    let token_to_launch_uref = get_dictionary_uref(TOKEN_TO_LAUNCH);
    let token_key = key_to_str(&token_placeholder);
    storage::dictionary_put(token_to_launch_uref, &token_key, launch_id);

    // Increment launch count
    write_to_uref(LAUNCH_COUNT, launch_id + 1);

    // Return launch ID and token/curve hashes
    runtime::ret(
        CLValue::from_t((launch_id, token_placeholder, curve_placeholder)).unwrap_or_revert(),
    );
}

/// Update launch status (placeholder - status is stored in bonding curve contract)
/// This entry point exists for future extensibility
#[no_mangle]
pub extern "C" fn update_launch_status() {
    let launch_id: u64 = runtime::get_named_arg("launch_id");
    let _new_status: u8 = runtime::get_named_arg("status");

    let count: u64 = read_from_uref(LAUNCH_COUNT);
    if launch_id >= count {
        runtime::revert(casper_types::ApiError::User(ERROR_INDEX_OUT_OF_BOUNDS));
    }

    // Status is managed by the bonding curve contract directly
    // This function validates the launch exists
}

// ============ Contract Installation ============

fn get_entry_points() -> EntryPoints {
    let mut entry_points = EntryPoints::new();

    // Init
    entry_points.add_entry_point(EntryPoint::new(
        "init",
        vec![],
        CLType::Unit,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    // Read-only entry points
    entry_points.add_entry_point(EntryPoint::new(
        "controller",
        vec![],
        CLType::Key,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "launch_count",
        vec![],
        CLType::U64,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "get_launch",
        vec![Parameter::new("launch_id", CLType::U64)],
        CLType::Option(Box::new(CLType::Tuple3([
            Box::new(CLType::Key),
            Box::new(CLType::Key),
            Box::new(CLType::Key),
        ]))),
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    // get_launch_meta returns (name, symbol, curve_type, status, created_at)
    entry_points.add_entry_point(EntryPoint::new(
        "get_launch_meta",
        vec![Parameter::new("launch_id", CLType::U64)],
        CLType::Option(Box::new(CLType::Tuple3([
            Box::new(CLType::String),
            Box::new(CLType::String),
            Box::new(CLType::Tuple3([
                Box::new(CLType::U8),
                Box::new(CLType::U8),
                Box::new(CLType::U64),
            ])),
        ]))),
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "get_launch_by_token",
        vec![Parameter::new("token_hash", CLType::Key)],
        CLType::Option(Box::new(CLType::U64)),
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "get_launches",
        vec![
            Parameter::new("offset", CLType::U64),
            Parameter::new("limit", CLType::U64),
            Parameter::new("status_filter", CLType::Option(Box::new(CLType::U8))),
        ],
        CLType::List(Box::new(CLType::U64)),
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    // State-changing entry points
    entry_points.add_entry_point(EntryPoint::new(
        "create_launch",
        vec![
            Parameter::new("name", CLType::String),
            Parameter::new("symbol", CLType::String),
            Parameter::new("curve_type", CLType::U8),
            Parameter::new("graduation_threshold", CLType::Option(Box::new(CLType::U512))),
            Parameter::new("creator_fee_bps", CLType::Option(Box::new(CLType::U64))),
            Parameter::new("deadline_days", CLType::Option(Box::new(CLType::U64))),
            Parameter::new("promo_budget", CLType::Option(Box::new(CLType::U512))),
            Parameter::new("description", CLType::Option(Box::new(CLType::String))),
            Parameter::new("website", CLType::Option(Box::new(CLType::String))),
            Parameter::new("twitter", CLType::Option(Box::new(CLType::String))),
        ],
        CLType::Tuple3([
            Box::new(CLType::U64),
            Box::new(CLType::Key),
            Box::new(CLType::Key),
        ]),
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "update_launch_status",
        vec![
            Parameter::new("launch_id", CLType::U64),
            Parameter::new("status", CLType::U8),
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
    let controller: Key = runtime::get_named_arg("controller");
    let platform_wallet: Key = runtime::get_named_arg("platform_wallet");

    let mut named_keys = NamedKeys::new();

    named_keys.insert(CONTROLLER.to_string(), storage::new_uref(controller).into());
    named_keys.insert(
        PLATFORM_WALLET.to_string(),
        storage::new_uref(platform_wallet).into(),
    );
    named_keys.insert(LAUNCH_COUNT.to_string(), storage::new_uref(0u64).into());
    named_keys.insert(INITIALIZED.to_string(), storage::new_uref(false).into());

    let (contract_hash, _) = storage::new_contract(
        get_entry_points(),
        Some(named_keys),
        Some("ectoplasm_token_factory_package".to_string()),
        Some("ectoplasm_token_factory_access".to_string()),
        None,
    );

    runtime::put_key("ectoplasm_token_factory", contract_hash.into());

    // Initialize (creates dictionaries)
    runtime::call_contract::<()>(contract_hash, "init", runtime_args! {});
}
