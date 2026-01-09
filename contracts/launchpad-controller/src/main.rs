#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::ToString;
use alloc::vec;
use casper_contract::{
    contract_api::{runtime, storage},
    unwrap_or_revert::UnwrapOrRevert,
};
use casper_types::{
    addressable_entity::{EntityEntryPoint as EntryPoint, EntryPoints},
    bytesrepr::{FromBytes, ToBytes},
    contracts::NamedKeys,
    runtime_args, CLType, CLTyped, CLValue, EntryPointAccess, EntryPointPayment,
    EntryPointType, Key, Parameter, RuntimeArgs, U512,
};

// ============ Storage Keys ============

const SUPERADMIN: &str = "superadmin";
const DEFAULT_GRADUATION_THRESHOLD: &str = "default_graduation_threshold";
const DEFAULT_PLATFORM_FEE_BPS: &str = "default_platform_fee_bps";
const DEFAULT_DEADLINE_DAYS: &str = "default_deadline_days";
const TOKEN_FACTORY: &str = "token_factory";
const INITIALIZED: &str = "initialized";

// ============ Error Codes ============

const ERROR_UNAUTHORIZED: u16 = 1;
const ERROR_ALREADY_INITIALIZED: u16 = 2;
const ERROR_INVALID_FEE: u16 = 3;
const ERROR_INVALID_THRESHOLD: u16 = 4;
const ERROR_INVALID_DEADLINE: u16 = 5;

// Maximum platform fee: 10% (1000 basis points)
const MAX_PLATFORM_FEE_BPS: u64 = 1000;

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

fn require_superadmin() {
    let caller = Key::Account(runtime::get_caller());
    let superadmin: Key = read_from_uref(SUPERADMIN);
    if caller != superadmin {
        runtime::revert(casper_types::ApiError::User(ERROR_UNAUTHORIZED));
    }
}

// ============ Entry Points ============

/// Initialize the controller (called automatically after deployment)
#[no_mangle]
pub extern "C" fn init() {
    let initialized: bool = read_from_uref(INITIALIZED);
    if initialized {
        runtime::revert(casper_types::ApiError::User(ERROR_ALREADY_INITIALIZED));
    }
    write_to_uref(INITIALIZED, true);
}

/// Get the superadmin address
#[no_mangle]
pub extern "C" fn superadmin() {
    let admin: Key = read_from_uref(SUPERADMIN);
    runtime::ret(CLValue::from_t(admin).unwrap_or_revert());
}

/// Get the default graduation threshold (in CSPR motes)
#[no_mangle]
pub extern "C" fn default_graduation_threshold() {
    let threshold: U512 = read_from_uref(DEFAULT_GRADUATION_THRESHOLD);
    runtime::ret(CLValue::from_t(threshold).unwrap_or_revert());
}

/// Get the default platform fee in basis points
#[no_mangle]
pub extern "C" fn default_platform_fee_bps() {
    let fee: u64 = read_from_uref(DEFAULT_PLATFORM_FEE_BPS);
    runtime::ret(CLValue::from_t(fee).unwrap_or_revert());
}

/// Get the default deadline in days
#[no_mangle]
pub extern "C" fn default_deadline_days() {
    let days: u64 = read_from_uref(DEFAULT_DEADLINE_DAYS);
    runtime::ret(CLValue::from_t(days).unwrap_or_revert());
}

/// Get the token factory address
#[no_mangle]
pub extern "C" fn token_factory() {
    let factory: Option<Key> = match runtime::get_key(TOKEN_FACTORY) {
        Some(key) => {
            let uref = key.into_uref().unwrap_or_revert();
            storage::read(uref).unwrap_or_revert()
        }
        None => None,
    };
    runtime::ret(CLValue::from_t(factory).unwrap_or_revert());
}

/// Get all default configuration values at once
#[no_mangle]
pub extern "C" fn get_defaults() {
    let graduation_threshold: U512 = read_from_uref(DEFAULT_GRADUATION_THRESHOLD);
    let platform_fee_bps: u64 = read_from_uref(DEFAULT_PLATFORM_FEE_BPS);
    let deadline_days: u64 = read_from_uref(DEFAULT_DEADLINE_DAYS);

    // Return as a tuple (threshold, fee_bps, deadline_days)
    runtime::ret(
        CLValue::from_t((graduation_threshold, platform_fee_bps, deadline_days)).unwrap_or_revert(),
    );
}

/// Set the default graduation threshold (superadmin only)
#[no_mangle]
pub extern "C" fn set_default_graduation_threshold() {
    require_superadmin();

    let threshold: U512 = runtime::get_named_arg("threshold");
    if threshold.is_zero() {
        runtime::revert(casper_types::ApiError::User(ERROR_INVALID_THRESHOLD));
    }

    write_to_uref(DEFAULT_GRADUATION_THRESHOLD, threshold);
}

/// Set the default platform fee in basis points (superadmin only)
#[no_mangle]
pub extern "C" fn set_default_platform_fee() {
    require_superadmin();

    let fee_bps: u64 = runtime::get_named_arg("fee_bps");
    if fee_bps > MAX_PLATFORM_FEE_BPS {
        runtime::revert(casper_types::ApiError::User(ERROR_INVALID_FEE));
    }

    write_to_uref(DEFAULT_PLATFORM_FEE_BPS, fee_bps);
}

/// Set the default deadline in days (superadmin only)
#[no_mangle]
pub extern "C" fn set_default_deadline() {
    require_superadmin();

    let days: u64 = runtime::get_named_arg("days");
    if days == 0 {
        runtime::revert(casper_types::ApiError::User(ERROR_INVALID_DEADLINE));
    }

    write_to_uref(DEFAULT_DEADLINE_DAYS, days);
}

/// Set the token factory address (superadmin only, one-time)
#[no_mangle]
pub extern "C" fn set_token_factory() {
    require_superadmin();

    let factory: Key = runtime::get_named_arg("factory");
    write_to_uref(TOKEN_FACTORY, Some(factory));
}

/// Transfer superadmin role to a new account (superadmin only)
#[no_mangle]
pub extern "C" fn transfer_superadmin() {
    require_superadmin();

    let new_admin: Key = runtime::get_named_arg("new_admin");
    write_to_uref(SUPERADMIN, new_admin);
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
        "superadmin",
        vec![],
        CLType::Key,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "default_graduation_threshold",
        vec![],
        CLType::U512,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "default_platform_fee_bps",
        vec![],
        CLType::U64,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "default_deadline_days",
        vec![],
        CLType::U64,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "token_factory",
        vec![],
        CLType::Option(alloc::boxed::Box::new(CLType::Key)),
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "get_defaults",
        vec![],
        CLType::Tuple3([
            alloc::boxed::Box::new(CLType::U512),
            alloc::boxed::Box::new(CLType::U64),
            alloc::boxed::Box::new(CLType::U64),
        ]),
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    // Admin entry points
    entry_points.add_entry_point(EntryPoint::new(
        "set_default_graduation_threshold",
        vec![Parameter::new("threshold", CLType::U512)],
        CLType::Unit,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "set_default_platform_fee",
        vec![Parameter::new("fee_bps", CLType::U64)],
        CLType::Unit,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "set_default_deadline",
        vec![Parameter::new("days", CLType::U64)],
        CLType::Unit,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "set_token_factory",
        vec![Parameter::new("factory", CLType::Key)],
        CLType::Unit,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "transfer_superadmin",
        vec![Parameter::new("new_admin", CLType::Key)],
        CLType::Unit,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points
}

#[no_mangle]
pub extern "C" fn call() {
    // Get optional deployment arguments with defaults
    let initial_graduation_threshold: U512 = runtime::get_named_arg::<Option<U512>>(
        "initial_graduation_threshold",
    )
    .unwrap_or_else(|| U512::from(50_000_000_000_000u64)); // 50,000 CSPR default

    let initial_platform_fee_bps: u64 =
        runtime::get_named_arg::<Option<u64>>("initial_platform_fee_bps").unwrap_or(100); // 1% default

    let initial_deadline_days: u64 =
        runtime::get_named_arg::<Option<u64>>("initial_deadline_days").unwrap_or(30); // 30 days default

    let mut named_keys = NamedKeys::new();
    let deployer = Key::Account(runtime::get_caller());

    // Store configuration
    named_keys.insert(
        SUPERADMIN.to_string(),
        storage::new_uref(deployer).into(),
    );
    named_keys.insert(
        DEFAULT_GRADUATION_THRESHOLD.to_string(),
        storage::new_uref(initial_graduation_threshold).into(),
    );
    named_keys.insert(
        DEFAULT_PLATFORM_FEE_BPS.to_string(),
        storage::new_uref(initial_platform_fee_bps).into(),
    );
    named_keys.insert(
        DEFAULT_DEADLINE_DAYS.to_string(),
        storage::new_uref(initial_deadline_days).into(),
    );
    named_keys.insert(
        TOKEN_FACTORY.to_string(),
        storage::new_uref(Option::<Key>::None).into(),
    );
    named_keys.insert(
        INITIALIZED.to_string(),
        storage::new_uref(false).into(),
    );

    let (contract_hash, _) = storage::new_contract(
        get_entry_points(),
        Some(named_keys),
        Some("ectoplasm_launchpad_controller_package".to_string()),
        Some("ectoplasm_launchpad_controller_access".to_string()),
        None,
    );

    runtime::put_key("ectoplasm_launchpad_controller", contract_hash.into());

    // Call init to mark as initialized
    runtime::call_contract::<()>(contract_hash, "init", runtime_args! {});
}
