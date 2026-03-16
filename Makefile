#!make

RUST_PROFILE ?= debug

ifeq ($(RUST_PROFILE),release)
CARGO_FLAGS  := --release
else
CARGO_FLAGS  :=
endif

BINS := world_server metan_cli

all: binaries

# Build all binaries in one cargo invocation.
binaries:
	@cargo build $(addprefix --bin ,$(BINS)) $(CARGO_FLAGS)
	@cp target/$(RUST_PROFILE)/world_server  bin/world_server
	@cp target/$(RUST_PROFILE)/metan_cli     bin/metan_cli
	@ln -sf metan_cli bin/metan

world_server:
	@cargo build --bin world_server $(CARGO_FLAGS)
	@cp target/$(RUST_PROFILE)/world_server bin/world_server

metan_cli:
	@cargo build --bin metan_cli $(CARGO_FLAGS)
	@cp target/$(RUST_PROFILE)/metan_cli bin/metan_cli
	@ln -sf metan_cli bin/metan

clean:
	@rm -rf ./bin/*

clean-crate:
	@cargo clean
