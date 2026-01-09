#![no_std]
#![no_main]

extern crate alloc;

mod curves;
mod error;

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec;
use casper_contract::{
    contract_api::{runtime, storage, system},
    unwrap_or_revert::UnwrapOrRevert,
};
use casper_types::{
    addressable_entity::{AddressableEntityHash, EntityEntryPoint as EntryPoint, EntryPoints},
    bytesrepr::{FromBytes, ToBytes},
    contracts::NamedKeys,
    runtime_args, CLType, CLTyped, CLValue, EntryPointAccess, EntryPointPayment,
    EntryPointType, Key, Parameter, RuntimeArgs, URef, U256, U512,
};

use curves::CurveType;
use error::BondingCurveError;

// ============ Storage Keys ============

const TOKEN_HASH: &str = "token_hash";
const CREATOR: &str = "creator";
const CURVE_TYPE: &str = "curve_type";
const GRADUATION_THRESHOLD: &str = "graduation_threshold";
const PLATFORM_FEE_BPS: &str = "platform_fee_bps";
const CREATOR_FEE_BPS: &str = "creator_fee_bps";
const DEADLINE: &str = "deadline";
const CSPR_RAISED: &str = "cspr_raised";
const TOKENS_SOLD: &str = "tokens_sold";
const TOTAL_SUPPLY: &str = "total_supply";
const BASE_PRICE: &str = "base_price";
const MAX_PRICE: &str = "max_price";
const STATUS: &str = "status";
const PURCHASES: &str = "purchases";
const PROMO_BUDGET: &str = "promo_budget";
const PROMO_RELEASED: &str = "promo_released";
const ACCUMULATED_FEES: &str = "accumulated_fees";
const PLATFORM_WALLET: &str = "platform_wallet";
const DEX_FACTORY: &str = "dex_factory";
const DEX_ROUTER: &str = "dex_router";
const LOCKED: &str = "locked";
const INITIALIZED: &str = "initialized";

// Status values
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

fn require_active() {
    let status: u8 = read_from_uref(STATUS);
    if status != STATUS_ACTIVE {
        runtime::revert(BondingCurveError::CurveNotActive);
    }
}

fn require_unlocked() {
    let locked: bool = read_from_uref(LOCKED);
    if locked {
        runtime::revert(BondingCurveError::LockedReentrancy);
    }
}

fn lock() {
    write_to_uref(LOCKED, true);
}

fn unlock() {
    write_to_uref(LOCKED, false);
}

fn get_current_time() -> u64 {
    // Use block time from runtime
    runtime::get_blocktime().into()
}

// ============ Entry Points ============

/// Initialize the bonding curve (called by factory)
#[no_mangle]
pub extern "C" fn init() {
    let initialized: bool = read_from_uref(INITIALIZED);
    if initialized {
        runtime::revert(BondingCurveError::AlreadyInitialized);
    }

    // Create purchases dictionary
    storage::new_dictionary(PURCHASES)
        .unwrap_or_revert_with(casper_types::ApiError::User(100));

    write_to_uref(INITIALIZED, true);
}

/// Get the token contract hash
#[no_mangle]
pub extern "C" fn token_hash() {
    let hash: Key = read_from_uref(TOKEN_HASH);
    runtime::ret(CLValue::from_t(hash).unwrap_or_revert());
}

/// Get the creator address
#[no_mangle]
pub extern "C" fn creator() {
    let creator: Key = read_from_uref(CREATOR);
    runtime::ret(CLValue::from_t(creator).unwrap_or_revert());
}

/// Get the curve type
#[no_mangle]
pub extern "C" fn curve_type() {
    let curve: u8 = read_from_uref(CURVE_TYPE);
    runtime::ret(CLValue::from_t(curve).unwrap_or_revert());
}

/// Get the graduation threshold
#[no_mangle]
pub extern "C" fn graduation_threshold() {
    let threshold: U512 = read_from_uref(GRADUATION_THRESHOLD);
    runtime::ret(CLValue::from_t(threshold).unwrap_or_revert());
}

/// Get current CSPR raised
#[no_mangle]
pub extern "C" fn cspr_raised() {
    let raised: U512 = read_from_uref(CSPR_RAISED);
    runtime::ret(CLValue::from_t(raised).unwrap_or_revert());
}

/// Get tokens sold
#[no_mangle]
pub extern "C" fn tokens_sold() {
    let sold: U256 = read_from_uref(TOKENS_SOLD);
    runtime::ret(CLValue::from_t(sold).unwrap_or_revert());
}

/// Get total supply
#[no_mangle]
pub extern "C" fn total_supply() {
    let supply: U256 = read_from_uref(TOTAL_SUPPLY);
    runtime::ret(CLValue::from_t(supply).unwrap_or_revert());
}

/// Get current status
#[no_mangle]
pub extern "C" fn status() {
    let status: u8 = read_from_uref(STATUS);
    runtime::ret(CLValue::from_t(status).unwrap_or_revert());
}

/// Get current spot price
#[no_mangle]
pub extern "C" fn get_price() {
    let curve_type_val: u8 = read_from_uref(CURVE_TYPE);
    let curve = CurveType::from_u8(curve_type_val).unwrap_or_revert();

    let tokens_sold: U256 = read_from_uref(TOKENS_SOLD);
    let total_supply: U256 = read_from_uref(TOTAL_SUPPLY);
    let base_price: U512 = read_from_uref(BASE_PRICE);
    let max_price: U512 = read_from_uref(MAX_PRICE);

    let price = curves::calculate_price(curve, tokens_sold, total_supply, base_price, max_price);
    runtime::ret(CLValue::from_t(price).unwrap_or_revert());
}

/// Get progress as tuple (cspr_raised, graduation_threshold, progress_bps)
#[no_mangle]
pub extern "C" fn get_progress() {
    let cspr_raised: U512 = read_from_uref(CSPR_RAISED);
    let graduation_threshold: U512 = read_from_uref(GRADUATION_THRESHOLD);

    let progress_bps: u64 = if graduation_threshold.is_zero() {
        0
    } else {
        // Progress in basis points (0-10000)
        let progress = (cspr_raised * U512::from(10000u64)) / graduation_threshold;
        progress.as_u64().min(10000)
    };

    runtime::ret(
        CLValue::from_t((cspr_raised, graduation_threshold, progress_bps)).unwrap_or_revert(),
    );
}

/// Get promo budget status
#[no_mangle]
pub extern "C" fn get_promo_status() {
    let budget: U512 = read_from_uref(PROMO_BUDGET);
    let released: U512 = read_from_uref(PROMO_RELEASED);

    // Calculate next milestone based on progress
    let cspr_raised: U512 = read_from_uref(CSPR_RAISED);
    let graduation_threshold: U512 = read_from_uref(GRADUATION_THRESHOLD);

    let progress_pct = if graduation_threshold.is_zero() {
        0u8
    } else {
        let pct = (cspr_raised * U512::from(100u64)) / graduation_threshold;
        pct.as_u64().min(100) as u8
    };

    // Milestones at 25%, 50%, 75%, 100%
    let next_milestone = if progress_pct >= 100 {
        100u8
    } else if progress_pct >= 75 {
        100u8
    } else if progress_pct >= 50 {
        75u8
    } else if progress_pct >= 25 {
        50u8
    } else {
        25u8
    };

    runtime::ret(CLValue::from_t((budget, released, next_milestone)).unwrap_or_revert());
}

/// Quote how many tokens can be bought with given CSPR
#[no_mangle]
pub extern "C" fn get_quote_buy() {
    let cspr_amount: U512 = runtime::get_named_arg("cspr_amount");

    let curve_type_val: u8 = read_from_uref(CURVE_TYPE);
    let curve = CurveType::from_u8(curve_type_val).unwrap_or_revert();

    let tokens_sold: U256 = read_from_uref(TOKENS_SOLD);
    let total_supply: U256 = read_from_uref(TOTAL_SUPPLY);
    let base_price: U512 = read_from_uref(BASE_PRICE);
    let max_price: U512 = read_from_uref(MAX_PRICE);

    // Deduct platform fee
    let platform_fee_bps: u64 = read_from_uref(PLATFORM_FEE_BPS);
    let creator_fee_bps: u64 = read_from_uref(CREATOR_FEE_BPS);
    let total_fee_bps = platform_fee_bps + creator_fee_bps;
    let fee = (cspr_amount * U512::from(total_fee_bps)) / U512::from(10000u64);
    let cspr_after_fee = cspr_amount - fee;

    let tokens = curves::calculate_tokens_for_cspr(
        curve,
        cspr_after_fee,
        tokens_sold,
        total_supply,
        base_price,
        max_price,
    );

    runtime::ret(CLValue::from_t(tokens).unwrap_or_revert());
}

/// Quote how much CSPR can be received for selling tokens
#[no_mangle]
pub extern "C" fn get_quote_sell() {
    let token_amount: U256 = runtime::get_named_arg("token_amount");

    let curve_type_val: u8 = read_from_uref(CURVE_TYPE);
    let curve = CurveType::from_u8(curve_type_val).unwrap_or_revert();

    let tokens_sold: U256 = read_from_uref(TOKENS_SOLD);
    let total_supply: U256 = read_from_uref(TOTAL_SUPPLY);
    let base_price: U512 = read_from_uref(BASE_PRICE);
    let max_price: U512 = read_from_uref(MAX_PRICE);

    let cspr_raw = curves::calculate_cspr_for_tokens(
        curve,
        token_amount,
        tokens_sold,
        total_supply,
        base_price,
        max_price,
    );

    // Deduct fees
    let platform_fee_bps: u64 = read_from_uref(PLATFORM_FEE_BPS);
    let creator_fee_bps: u64 = read_from_uref(CREATOR_FEE_BPS);
    let total_fee_bps = platform_fee_bps + creator_fee_bps;
    let fee = (cspr_raw * U512::from(total_fee_bps)) / U512::from(10000u64);
    let cspr_after_fee = cspr_raw - fee;

    runtime::ret(CLValue::from_t(cspr_after_fee).unwrap_or_revert());
}

/// Buy tokens with CSPR
#[no_mangle]
pub extern "C" fn buy() {
    require_active();
    require_unlocked();
    lock();

    let cspr_amount: U512 = runtime::get_named_arg("amount");
    if cspr_amount.is_zero() {
        unlock();
        runtime::revert(BondingCurveError::InvalidAmount);
    }

    let caller = Key::Account(runtime::get_caller());

    // Calculate fees
    let platform_fee_bps: u64 = read_from_uref(PLATFORM_FEE_BPS);
    let creator_fee_bps: u64 = read_from_uref(CREATOR_FEE_BPS);

    let platform_fee = (cspr_amount * U512::from(platform_fee_bps)) / U512::from(10000u64);
    let creator_fee = (cspr_amount * U512::from(creator_fee_bps)) / U512::from(10000u64);
    let cspr_for_curve = cspr_amount - platform_fee - creator_fee;

    // Calculate tokens to receive
    let curve_type_val: u8 = read_from_uref(CURVE_TYPE);
    let curve = CurveType::from_u8(curve_type_val).unwrap_or_revert();

    let tokens_sold: U256 = read_from_uref(TOKENS_SOLD);
    let total_supply: U256 = read_from_uref(TOTAL_SUPPLY);
    let base_price: U512 = read_from_uref(BASE_PRICE);
    let max_price: U512 = read_from_uref(MAX_PRICE);

    let tokens_to_buy = curves::calculate_tokens_for_cspr(
        curve,
        cspr_for_curve,
        tokens_sold,
        total_supply,
        base_price,
        max_price,
    );

    if tokens_to_buy.is_zero() {
        unlock();
        runtime::revert(BondingCurveError::InvalidAmount);
    }

    // Check remaining supply
    let remaining = total_supply - tokens_sold;
    if tokens_to_buy > remaining {
        unlock();
        runtime::revert(BondingCurveError::InsufficientLiquidity);
    }

    // Update state
    let new_tokens_sold = tokens_sold + tokens_to_buy;
    write_to_uref(TOKENS_SOLD, new_tokens_sold);

    let cspr_raised: U512 = read_from_uref(CSPR_RAISED);
    write_to_uref(CSPR_RAISED, cspr_raised + cspr_for_curve);

    // Accumulate creator fees
    let accumulated: U512 = read_from_uref(ACCUMULATED_FEES);
    write_to_uref(ACCUMULATED_FEES, accumulated + creator_fee);

    // Record purchase for potential refund
    let purchases_uref = get_dictionary_uref(PURCHASES);
    let caller_key = key_to_str(&caller);
    let existing: U512 = storage::dictionary_get(purchases_uref, &caller_key)
        .unwrap_or_default()
        .unwrap_or(U512::zero());
    storage::dictionary_put(purchases_uref, &caller_key, existing + cspr_for_curve);

    // Transfer platform fee to platform wallet
    if !platform_fee.is_zero() {
        let platform_wallet: Key = read_from_uref(PLATFORM_WALLET);
        if let Key::Account(account) = platform_wallet {
            system::transfer_to_account(account, platform_fee, None).unwrap_or_revert();
        }
    }

    // Mint tokens to buyer via token contract
    let token_hash: Key = read_from_uref(TOKEN_HASH);
    if let Key::AddressableEntity(entity_addr) = token_hash {
        let token_contract = AddressableEntityHash::new(entity_addr.value());
        runtime::call_contract::<()>(
            token_contract.into(),
            "mint",
            runtime_args! {
                "to" => caller,
                "amount" => tokens_to_buy
            },
        );
    }

    unlock();

    // Check if should graduate
    let graduation_threshold: U512 = read_from_uref(GRADUATION_THRESHOLD);
    let new_cspr_raised: U512 = read_from_uref(CSPR_RAISED);
    if new_cspr_raised >= graduation_threshold {
        // Auto-graduate could be triggered here or left for manual call
    }

    runtime::ret(CLValue::from_t(tokens_to_buy).unwrap_or_revert());
}

/// Sell tokens back to the curve
#[no_mangle]
pub extern "C" fn sell() {
    require_active();
    require_unlocked();
    lock();

    let token_amount: U256 = runtime::get_named_arg("amount");
    if token_amount.is_zero() {
        unlock();
        runtime::revert(BondingCurveError::InvalidAmount);
    }

    let caller = Key::Account(runtime::get_caller());

    // Calculate CSPR to return
    let curve_type_val: u8 = read_from_uref(CURVE_TYPE);
    let curve = CurveType::from_u8(curve_type_val).unwrap_or_revert();

    let tokens_sold: U256 = read_from_uref(TOKENS_SOLD);
    let total_supply: U256 = read_from_uref(TOTAL_SUPPLY);
    let base_price: U512 = read_from_uref(BASE_PRICE);
    let max_price: U512 = read_from_uref(MAX_PRICE);

    if token_amount > tokens_sold {
        unlock();
        runtime::revert(BondingCurveError::InsufficientTokens);
    }

    let cspr_raw = curves::calculate_cspr_for_tokens(
        curve,
        token_amount,
        tokens_sold,
        total_supply,
        base_price,
        max_price,
    );

    // Deduct fees
    let platform_fee_bps: u64 = read_from_uref(PLATFORM_FEE_BPS);
    let creator_fee_bps: u64 = read_from_uref(CREATOR_FEE_BPS);

    let platform_fee = (cspr_raw * U512::from(platform_fee_bps)) / U512::from(10000u64);
    let creator_fee = (cspr_raw * U512::from(creator_fee_bps)) / U512::from(10000u64);
    let cspr_to_return = cspr_raw - platform_fee - creator_fee;

    // Check curve has enough CSPR
    let cspr_raised: U512 = read_from_uref(CSPR_RAISED);
    if cspr_to_return > cspr_raised {
        unlock();
        runtime::revert(BondingCurveError::InsufficientLiquidity);
    }

    // Burn tokens from seller via token contract
    let token_hash: Key = read_from_uref(TOKEN_HASH);
    if let Key::AddressableEntity(entity_addr) = token_hash {
        let token_contract = AddressableEntityHash::new(entity_addr.value());
        runtime::call_contract::<()>(
            token_contract.into(),
            "burn",
            runtime_args! {
                "from" => caller,
                "amount" => token_amount
            },
        );
    }

    // Update state
    write_to_uref(TOKENS_SOLD, tokens_sold - token_amount);
    write_to_uref(CSPR_RAISED, cspr_raised - cspr_raw);

    // Accumulate creator fees
    let accumulated: U512 = read_from_uref(ACCUMULATED_FEES);
    write_to_uref(ACCUMULATED_FEES, accumulated + creator_fee);

    // Transfer CSPR to seller
    if let Key::Account(account) = caller {
        system::transfer_to_account(account, cspr_to_return, None).unwrap_or_revert();
    }

    // Transfer platform fee
    if !platform_fee.is_zero() {
        let platform_wallet: Key = read_from_uref(PLATFORM_WALLET);
        if let Key::Account(account) = platform_wallet {
            system::transfer_to_account(account, platform_fee, None).unwrap_or_revert();
        }
    }

    unlock();
    runtime::ret(CLValue::from_t(cspr_to_return).unwrap_or_revert());
}

/// Claim refund if deadline passed and not graduated
#[no_mangle]
pub extern "C" fn claim_refund() {
    require_unlocked();
    lock();

    let status: u8 = read_from_uref(STATUS);
    if status == STATUS_GRADUATED {
        unlock();
        runtime::revert(BondingCurveError::RefundNotAvailable);
    }

    // Check deadline
    let deadline: u64 = read_from_uref(DEADLINE);
    let current_time = get_current_time();

    if current_time < deadline {
        unlock();
        runtime::revert(BondingCurveError::DeadlineNotReached);
    }

    // Set status to refunding if not already
    if status == STATUS_ACTIVE {
        write_to_uref(STATUS, STATUS_REFUNDING);
    }

    let caller = Key::Account(runtime::get_caller());
    let caller_key = key_to_str(&caller);

    // Get caller's purchase amount
    let purchases_uref = get_dictionary_uref(PURCHASES);
    let purchase_amount: U512 = storage::dictionary_get(purchases_uref, &caller_key)
        .unwrap_or_default()
        .unwrap_or(U512::zero());

    if purchase_amount.is_zero() {
        unlock();
        runtime::revert(BondingCurveError::NoRefundAvailable);
    }

    // Clear purchase record
    storage::dictionary_put(purchases_uref, &caller_key, U512::zero());

    // Transfer refund
    if let Key::Account(account) = caller {
        system::transfer_to_account(account, purchase_amount, None).unwrap_or_revert();
    }

    unlock();
    runtime::ret(CLValue::from_t(purchase_amount).unwrap_or_revert());
}

/// Graduate the curve to DEX (creates pair and adds liquidity)
#[no_mangle]
pub extern "C" fn graduate() {
    require_active();
    require_unlocked();
    lock();

    let cspr_raised: U512 = read_from_uref(CSPR_RAISED);
    let graduation_threshold: U512 = read_from_uref(GRADUATION_THRESHOLD);

    if cspr_raised < graduation_threshold {
        unlock();
        runtime::revert(BondingCurveError::GraduationThresholdNotMet);
    }

    // Mark as graduated
    write_to_uref(STATUS, STATUS_GRADUATED);

    // TODO: Integrate with DEX to create pair and add liquidity
    // This would call the DEX factory to create a pair
    // and the router to add liquidity

    unlock();
    runtime::ret(CLValue::from_t(true).unwrap_or_revert());
}

/// Creator withdraws accumulated fees
#[no_mangle]
pub extern "C" fn withdraw_fees() {
    let caller = Key::Account(runtime::get_caller());
    let creator: Key = read_from_uref(CREATOR);

    if caller != creator {
        runtime::revert(BondingCurveError::Unauthorized);
    }

    let accumulated: U512 = read_from_uref(ACCUMULATED_FEES);
    if accumulated.is_zero() {
        runtime::revert(BondingCurveError::NoPromoToWithdraw);
    }

    write_to_uref(ACCUMULATED_FEES, U512::zero());

    if let Key::Account(account) = caller {
        system::transfer_to_account(account, accumulated, None).unwrap_or_revert();
    }

    runtime::ret(CLValue::from_t(accumulated).unwrap_or_revert());
}

/// Creator claims promo budget based on milestones
#[no_mangle]
pub extern "C" fn claim_promo_milestone() {
    let caller = Key::Account(runtime::get_caller());
    let creator: Key = read_from_uref(CREATOR);

    if caller != creator {
        runtime::revert(BondingCurveError::Unauthorized);
    }

    let promo_budget: U512 = read_from_uref(PROMO_BUDGET);
    let promo_released: U512 = read_from_uref(PROMO_RELEASED);
    let cspr_raised: U512 = read_from_uref(CSPR_RAISED);
    let graduation_threshold: U512 = read_from_uref(GRADUATION_THRESHOLD);

    // Calculate progress percentage
    let progress_pct = if graduation_threshold.is_zero() {
        0u64
    } else {
        let pct = (cspr_raised * U512::from(100u64)) / graduation_threshold;
        pct.as_u64().min(100)
    };

    // Calculate entitled amount based on milestones
    // 25% progress -> 25% of budget
    // 50% progress -> 50% of budget
    // 75% progress -> 75% of budget
    // 100% progress -> 100% of budget
    let entitled_pct = if progress_pct >= 100 {
        100u64
    } else if progress_pct >= 75 {
        75u64
    } else if progress_pct >= 50 {
        50u64
    } else if progress_pct >= 25 {
        25u64
    } else {
        0u64
    };

    let entitled_amount = (promo_budget * U512::from(entitled_pct)) / U512::from(100u64);
    let claimable = if entitled_amount > promo_released {
        entitled_amount - promo_released
    } else {
        U512::zero()
    };

    if claimable.is_zero() {
        runtime::revert(BondingCurveError::MilestoneNotUnlocked);
    }

    write_to_uref(PROMO_RELEASED, promo_released + claimable);

    if let Key::Account(account) = caller {
        system::transfer_to_account(account, claimable, None).unwrap_or_revert();
    }

    runtime::ret(CLValue::from_t(claimable).unwrap_or_revert());
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
        "token_hash",
        vec![],
        CLType::Key,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "creator",
        vec![],
        CLType::Key,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "curve_type",
        vec![],
        CLType::U8,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "graduation_threshold",
        vec![],
        CLType::U512,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "cspr_raised",
        vec![],
        CLType::U512,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "tokens_sold",
        vec![],
        CLType::U256,
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
        "status",
        vec![],
        CLType::U8,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "get_price",
        vec![],
        CLType::U512,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "get_progress",
        vec![],
        CLType::Tuple3([
            Box::new(CLType::U512),
            Box::new(CLType::U512),
            Box::new(CLType::U64),
        ]),
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "get_promo_status",
        vec![],
        CLType::Tuple3([
            Box::new(CLType::U512),
            Box::new(CLType::U512),
            Box::new(CLType::U8),
        ]),
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "get_quote_buy",
        vec![Parameter::new("cspr_amount", CLType::U512)],
        CLType::U256,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "get_quote_sell",
        vec![Parameter::new("token_amount", CLType::U256)],
        CLType::U512,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    // State-changing entry points
    entry_points.add_entry_point(EntryPoint::new(
        "buy",
        vec![Parameter::new("amount", CLType::U512)],
        CLType::U256,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "sell",
        vec![Parameter::new("amount", CLType::U256)],
        CLType::U512,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "claim_refund",
        vec![],
        CLType::U512,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "graduate",
        vec![],
        CLType::Bool,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "withdraw_fees",
        vec![],
        CLType::U512,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points.add_entry_point(EntryPoint::new(
        "claim_promo_milestone",
        vec![],
        CLType::U512,
        EntryPointAccess::Public,
        EntryPointType::Called,
        EntryPointPayment::Caller,
    ));

    entry_points
}

/// Contract deployment - called by Token Factory
#[no_mangle]
pub extern "C" fn call() {
    // Get deployment arguments
    let token_hash: Key = runtime::get_named_arg("token_hash");
    let creator: Key = runtime::get_named_arg("creator");
    let curve_type: u8 = runtime::get_named_arg("curve_type");
    let graduation_threshold: U512 = runtime::get_named_arg("graduation_threshold");
    let platform_fee_bps: u64 = runtime::get_named_arg("platform_fee_bps");
    let creator_fee_bps: u64 = runtime::get_named_arg("creator_fee_bps");
    let deadline: u64 = runtime::get_named_arg("deadline");
    let total_supply: U256 = runtime::get_named_arg("total_supply");
    let base_price: U512 = runtime::get_named_arg("base_price");
    let max_price: U512 = runtime::get_named_arg("max_price");
    let promo_budget: U512 = runtime::get_named_arg("promo_budget");
    let platform_wallet: Key = runtime::get_named_arg("platform_wallet");

    let mut named_keys = NamedKeys::new();

    named_keys.insert(TOKEN_HASH.to_string(), storage::new_uref(token_hash).into());
    named_keys.insert(CREATOR.to_string(), storage::new_uref(creator).into());
    named_keys.insert(CURVE_TYPE.to_string(), storage::new_uref(curve_type).into());
    named_keys.insert(
        GRADUATION_THRESHOLD.to_string(),
        storage::new_uref(graduation_threshold).into(),
    );
    named_keys.insert(
        PLATFORM_FEE_BPS.to_string(),
        storage::new_uref(platform_fee_bps).into(),
    );
    named_keys.insert(
        CREATOR_FEE_BPS.to_string(),
        storage::new_uref(creator_fee_bps).into(),
    );
    named_keys.insert(DEADLINE.to_string(), storage::new_uref(deadline).into());
    named_keys.insert(CSPR_RAISED.to_string(), storage::new_uref(U512::zero()).into());
    named_keys.insert(TOKENS_SOLD.to_string(), storage::new_uref(U256::zero()).into());
    named_keys.insert(TOTAL_SUPPLY.to_string(), storage::new_uref(total_supply).into());
    named_keys.insert(BASE_PRICE.to_string(), storage::new_uref(base_price).into());
    named_keys.insert(MAX_PRICE.to_string(), storage::new_uref(max_price).into());
    named_keys.insert(STATUS.to_string(), storage::new_uref(STATUS_ACTIVE).into());
    named_keys.insert(PROMO_BUDGET.to_string(), storage::new_uref(promo_budget).into());
    named_keys.insert(PROMO_RELEASED.to_string(), storage::new_uref(U512::zero()).into());
    named_keys.insert(
        ACCUMULATED_FEES.to_string(),
        storage::new_uref(U512::zero()).into(),
    );
    named_keys.insert(
        PLATFORM_WALLET.to_string(),
        storage::new_uref(platform_wallet).into(),
    );
    named_keys.insert(LOCKED.to_string(), storage::new_uref(false).into());
    named_keys.insert(INITIALIZED.to_string(), storage::new_uref(false).into());

    let (contract_hash, _) = storage::new_contract(
        get_entry_points(),
        Some(named_keys),
        Some("ectoplasm_bonding_curve_package".to_string()),
        Some("ectoplasm_bonding_curve_access".to_string()),
        None,
    );

    runtime::put_key("ectoplasm_bonding_curve", contract_hash.into());

    // Initialize (creates dictionaries)
    runtime::call_contract::<()>(contract_hash, "init", runtime_args! {});
}
