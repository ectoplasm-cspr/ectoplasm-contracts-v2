use casper_contract::contract_api::storage;
use casper_types::{Key, U256};

use crate::data::{get_dictionary_uref, key_to_str, BALANCES};

/// Read balance from the balances dictionary
pub fn read_balance(owner: &Key) -> U256 {
    let dict_uref = get_dictionary_uref(BALANCES);
    let key = key_to_str(owner);
    storage::dictionary_get(dict_uref, &key)
        .unwrap_or_default()
        .unwrap_or_default()
}

/// Write balance to the balances dictionary
pub fn write_balance(owner: &Key, amount: U256) {
    let dict_uref = get_dictionary_uref(BALANCES);
    let key = key_to_str(owner);
    storage::dictionary_put(dict_uref, &key, amount);
}
