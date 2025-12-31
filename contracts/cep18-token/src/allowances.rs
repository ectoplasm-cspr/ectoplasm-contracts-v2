use casper_contract::contract_api::storage;
use casper_types::{Key, U256};

use crate::data::{allowance_key, get_dictionary_uref, ALLOWANCES};

/// Read allowance from the allowances dictionary
pub fn read_allowance(owner: &Key, spender: &Key) -> U256 {
    let dict_uref = get_dictionary_uref(ALLOWANCES);
    let key = allowance_key(owner, spender);
    storage::dictionary_get(dict_uref, &key)
        .unwrap_or_default()
        .unwrap_or_default()
}

/// Write allowance to the allowances dictionary
pub fn write_allowance(owner: &Key, spender: &Key, amount: U256) {
    let dict_uref = get_dictionary_uref(ALLOWANCES);
    let key = allowance_key(owner, spender);
    storage::dictionary_put(dict_uref, &key, amount);
}
