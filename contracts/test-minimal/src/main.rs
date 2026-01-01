#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::{String, ToString};
use casper_contract::{
    contract_api::{runtime, storage},
    unwrap_or_revert::UnwrapOrRevert,
};
use casper_types::{
    addressable_entity::{EntityEntryPoint as EntryPoint, EntryPoints},
    contracts::NamedKeys,
    CLType, CLValue, EntryPointAccess, EntryPointPayment, EntryPointType, U256,
};

#[no_mangle]
pub extern "C" fn get_value() {
    runtime::ret(CLValue::from_t(42u64).unwrap_or_revert());
}

fn get_entry_points() -> EntryPoints {
    let mut entry_points = EntryPoints::new();
    entry_points.add_entry_point(EntryPoint::new(
        "get_value",
        alloc::vec![],
        CLType::U64,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));
    entry_points
}

#[no_mangle]
pub extern "C" fn call() {
    let named_keys = NamedKeys::new();

    let (contract_hash, _) = storage::new_contract(
        get_entry_points(),
        Some(named_keys),
        Some("test_minimal_package5".to_string()),
        Some("test_minimal_access5".to_string()),
        None,
    );

    runtime::put_key("test_minimal_contract5", contract_hash.into());

    // After contract creation, use named_dictionary_put
    // to store values without new_dictionary
    storage::named_dictionary_put("balances", "test_key", 100u64);
}
