UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
  ifeq ($(DEVELOPER_DIR),)
    ifneq ($(wildcard /Library/Developer/CommandLineTools),)
      export DEVELOPER_DIR := /Library/Developer/CommandLineTools
    endif
  endif
endif

.PHONY: fmt clippy test test-server bench-ai check-boundaries check-web-safeguards check-glossary check check-sandbox ts-bindings install-hooks web-install web-typecheck web-test web-build dist-mac

fmt:
	cargo fmt --all -- --check

clippy: web-build
	cargo clippy --workspace --all-targets -- -D warnings

test: web-build
	cargo test --workspace

test-server: web-build
	cargo test -p mahjong-server

bench-ai:
	cargo run --release -p mahjong-ai --bin bench-ai -- gate

check-boundaries:
	./scripts/check-core-boundaries.sh

check-web-safeguards:
	./scripts/check-web-safeguards.sh

check-glossary:
	./scripts/check-glossary.sh

check:
	./scripts/check.sh

check-sandbox:
	./scripts/check.sh --sandbox

ts-bindings:
	UPDATE_TS_BINDINGS=1 cargo test -p mahjong-server --test ts_contract_drift

install-hooks:
	git config core.hooksPath .githooks

web-install:
	npm ci --prefix web

web-typecheck:
	npm --prefix web run typecheck

web-test:
	npm --prefix web test

web-build:
	npm --prefix web run build

dist-mac: web-build
	cargo build --release -p mahjong-server
	cargo test -p mahjong-server --lib static_assets::tests
	cargo test -p mahjong-server --test lifecycle_and_embed default_server_serves_embedded_assets_with_cache_headers_and_guards
	@test -x target/release/mahjong-server
	@echo "Embedded web/dist size: $$(du -sh web/dist | awk '{print $$1}')"
	@echo "Single binary size: $$(ls -lh target/release/mahjong-server | awk '{print $$5, $$9}')"
