UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
  ifeq ($(DEVELOPER_DIR),)
    ifneq ($(wildcard /Library/Developer/CommandLineTools),)
      export DEVELOPER_DIR := /Library/Developer/CommandLineTools
    endif
  endif
endif

.PHONY: fmt clippy test check-boundaries check-glossary check ts-bindings install-hooks

fmt:
	cargo fmt --all -- --check

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace

check-boundaries:
	./scripts/check-core-boundaries.sh

check-glossary:
	./scripts/check-glossary.sh

check:
	./scripts/check.sh

ts-bindings:
	UPDATE_TS_BINDINGS=1 cargo test -p mahjong-server --test ts_contract_drift

install-hooks:
	git config core.hooksPath .githooks
