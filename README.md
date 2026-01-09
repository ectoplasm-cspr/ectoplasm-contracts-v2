# Ectoplasm Contracts V2

Casper 2.0 native smart contracts for the Ectoplasm DEX. These contracts are built without frameworks, using pure `casper-contract` and `casper-types` crates for maximum compatibility with Casper 2.0.

## Overview

This project implements a Uniswap V2-style AMM (Automated Market Maker) DEX with:

- **Factory**: Creates and manages trading pairs
- **Pair**: AMM pools with constant product formula (x * y = k)
- **CEP-18 Tokens**: Standard fungible tokens for testing

## Architecture

### Two-Phase Initialization Pattern

Casper 2.0 requires dictionaries to be created within a contract context (entry point), not during the `call()` function. This project uses a two-phase initialization pattern:

```
1. call() - Creates the contract with basic named keys
2. init() - Called immediately after to create dictionaries
```

This is necessary because `storage::new_dictionary()` only works within an entry point context in Casper 2.0.

### Contract Structure

```
contracts/
├── factory/          # Factory contract - creates pairs
│   └── src/main.rs
├── pair/             # Pair contract - AMM pool
│   └── src/main.rs
└── cep18/            # CEP-18 token implementation
    └── src/main.rs
```

## Prerequisites

- Rust 1.70+
- `wasm32-unknown-unknown` target
- Casper client (`casper-client`)

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add WASM target
rustup target add wasm32-unknown-unknown

# Install Casper client
cargo install casper-client
```

## Building

```bash
# Build all contracts
./build.sh

# Or build individually
cd contracts/factory && cargo build --release --target wasm32-unknown-unknown
cd contracts/pair && cargo build --release --target wasm32-unknown-unknown
cd contracts/cep18 && cargo build --release --target wasm32-unknown-unknown
```

Compiled WASM files are output to `target/wasm32-unknown-unknown/release/`.

## Deployment

### 1. Deploy Factory

```bash
casper-client put-transaction session \
  --node-address https://node.testnet.casper.network \
  --chain-name casper-test \
  --secret-key /path/to/secret_key.pem \
  --payment-amount 150000000000 \
  --session-path target/wasm32-unknown-unknown/release/factory.wasm \
  --session-arg "fee_to_setter:key='account-hash-YOUR_ACCOUNT_HASH'"
```

After deployment, call `init`:

```bash
casper-client put-transaction invocable-entity \
  --node-address https://node.testnet.casper.network \
  --chain-name casper-test \
  --secret-key /path/to/secret_key.pem \
  --payment-amount 5000000000 \
  --entity-hash entity-contract-FACTORY_HASH \
  --entry-point init
```

### 2. Deploy CEP-18 Tokens

```bash
casper-client put-transaction session \
  --node-address https://node.testnet.casper.network \
  --chain-name casper-test \
  --secret-key /path/to/secret_key.pem \
  --payment-amount 150000000000 \
  --session-path target/wasm32-unknown-unknown/release/cep18.wasm \
  --session-arg "name:string='Token Name'" \
  --session-arg "symbol:string='TKN'" \
  --session-arg "decimals:u8='18'" \
  --session-arg "total_supply:u256='1000000000000000000000000'"
```

Then call `init`:

```bash
casper-client put-transaction invocable-entity \
  --node-address https://node.testnet.casper.network \
  --chain-name casper-test \
  --secret-key /path/to/secret_key.pem \
  --payment-amount 5000000000 \
  --entity-hash entity-contract-TOKEN_HASH \
  --entry-point init
```

### 3. Create Trading Pairs

Use the Factory's `create_pair` entry point:

```bash
casper-client put-transaction invocable-entity \
  --node-address https://node.testnet.casper.network \
  --chain-name casper-test \
  --secret-key /path/to/secret_key.pem \
  --payment-amount 200000000000 \
  --entity-hash entity-contract-FACTORY_HASH \
  --entry-point create_pair \
  --session-arg "token_a:key='hash-TOKEN_A_HASH'" \
  --session-arg "token_b:key='hash-TOKEN_B_HASH'" \
  --session-arg "pair_wasm:bytes='$(xxd -p target/wasm32-unknown-unknown/release/pair.wasm | tr -d '\n')'"
```

## Deployed Contracts (Testnet)

### Factory

| Contract | Hash |
|----------|------|
| Factory | `hash-8a4f4ffeab7a7c831359ee593b2edb5ee34333b7223b63f5ec906e42bc325ced` |

### Tokens

| Token | Hash | Decimals |
|-------|------|----------|
| ECTO | `hash-01b5a8092c45fb6276c5c3cf6b4c22730856cf0fc0051b078cf86010147d7a6f` | 18 |
| USDC | `hash-da800ac07a00e316bc84e3c1b614cfd9ff2db87b90904e30fa3a1bc5a632c2f0` | 6 |
| WETH | `hash-38fa5e20e2f80fb777e6036e2582adb98b387d785828a672ff2cea4aeb9fa990` | 18 |
| WBTC | `hash-e7ff916e02b42268d755b8aaffa9e8ae09e00c8d99c0db628d02c925020bd8fb` | 8 |

### Trading Pairs

| Pair | Hash |
|------|------|
| ECTO/USDC | `hash-2c2287ee64b4b372227fcd9b448d664e270d949e9b37830dd28a0b8e8e5401b9` |
| WETH/USDC | `hash-6759b832fe25e36288f9e63591242b54fc3a8b141a09b232a5a48ee2698d0e20` |
| WBTC/USDC | `hash-0fb2b764080ef5d8912c94c7cc305625e83999f77e8f7088741dc62e8b65ecc7` |

## Contract Entry Points

### Factory

| Entry Point | Description |
|-------------|-------------|
| `init` | Initialize dictionaries (call once after deploy) |
| `create_pair` | Create a new trading pair |
| `set_fee_to` | Set fee recipient address |
| `set_fee_to_setter` | Transfer fee setter role |

### Pair

| Entry Point | Description |
|-------------|-------------|
| `init` | Initialize dictionaries and state |
| `mint` | Mint LP tokens (add liquidity) |
| `burn` | Burn LP tokens (remove liquidity) |
| `swap` | Execute token swap |
| `sync` | Sync reserves with balances |
| `skim` | Skim excess tokens |
| `transfer` | Transfer LP tokens |
| `approve` | Approve LP token allowance |
| `transfer_from` | Transfer LP tokens with allowance |

### CEP-18 Token

| Entry Point | Description |
|-------------|-------------|
| `init` | Initialize balances dictionary |
| `transfer` | Transfer tokens |
| `approve` | Approve allowance |
| `transfer_from` | Transfer with allowance |
| `mint` | Mint new tokens (admin only) |

## Querying State

### Get Token Balance

```bash
casper-client get-dictionary-item \
  --node-address https://node.testnet.casper.network \
  --state-root-hash STATE_ROOT_HASH \
  --contract-hash hash-TOKEN_HASH \
  --dictionary-name balances \
  --dictionary-item-key "ACCOUNT_HASH_HEX"
```

### Get Pair Reserves

```bash
casper-client query-global-state \
  --node-address https://node.testnet.casper.network \
  --state-root-hash STATE_ROOT_HASH \
  --key hash-PAIR_HASH \
  -q "reserve0"
```

## Key Technical Details

### Dictionary Key Format

- **Balances**: Raw account hash hex (32 bytes, no prefix)
  - Example: `d79d4d353417cbeff0c7b3f586089115e6bc7776b237f542e92c846654b773ee`

### Dependencies

```toml
[dependencies]
casper-contract = "5.1.1"
casper-types = "6.1.0"
```

### Casper 2.0 Compatibility

These contracts are built for Casper 2.0 (post-Condor upgrade) and use:
- `put-transaction` instead of `put-deploy`
- `entity-contract-` prefix for contract calls
- Two-phase initialization for dictionary creation

## License

MIT
