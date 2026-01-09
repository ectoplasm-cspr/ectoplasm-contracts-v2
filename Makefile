.PHONY: all build clean prepare test

CONTRACTS = cep18-token ecto-token usdc-token weth-token wbtc-token factory router pair launchpad-controller bonding-curve token-factory
TARGET = wasm32-unknown-unknown

all: build

prepare:
	rustup target add $(TARGET)

build: prepare
	cargo build --release --target $(TARGET)
	@mkdir -p wasm
	@for contract in $(CONTRACTS); do \
		if [ -f "target/$(TARGET)/release/$${contract//-/_}.wasm" ]; then \
			wasm-strip target/$(TARGET)/release/$${contract//-/_}.wasm 2>/dev/null || true; \
			cp target/$(TARGET)/release/$${contract//-/_}.wasm wasm/; \
			echo "Built: wasm/$${contract//-/_}.wasm"; \
		fi; \
	done

build-cep18:
	cargo build --release --target $(TARGET) -p cep18-token
	@mkdir -p wasm
	wasm-strip target/$(TARGET)/release/cep18_token.wasm 2>/dev/null || true
	cp target/$(TARGET)/release/cep18_token.wasm wasm/

clean:
	cargo clean
	rm -rf wasm

test:
	cargo test
