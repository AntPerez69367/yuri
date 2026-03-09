#!make

RUST_PROFILE ?= debug

ifeq ($(RUST_PROFILE),release)
CARGO_FLAGS  := --release
else
CARGO_FLAGS  :=
endif

BINS := login_server char_server map_server decrypt_cli metan_cli

all: binaries

# Build all five binaries in one cargo invocation.
binaries:
	@cargo build $(addprefix --bin ,$(BINS)) $(CARGO_FLAGS)
	@cp target/$(RUST_PROFILE)/login_server  bin/login_server
	@cp target/$(RUST_PROFILE)/char_server   bin/char_server
	@cp target/$(RUST_PROFILE)/map_server    bin/map_server
	@cp target/$(RUST_PROFILE)/decrypt_cli   bin/decrypt_cli
	@cp target/$(RUST_PROFILE)/metan_cli     bin/metan_cli
	@ln -sf metan_cli bin/metan

# Individual targets for incremental rebuilds.
login_server:
	@cargo build --bin login_server $(CARGO_FLAGS)
	@cp target/$(RUST_PROFILE)/login_server bin/login_server

char_server:
	@cargo build --bin char_server $(CARGO_FLAGS)
	@cp target/$(RUST_PROFILE)/char_server bin/char_server

map_server:
	@cargo build --bin map_server $(CARGO_FLAGS)
	@cp target/$(RUST_PROFILE)/map_server bin/map_server

decrypt_cli:
	@cargo build --bin decrypt_cli $(CARGO_FLAGS)
	@cp target/$(RUST_PROFILE)/decrypt_cli bin/decrypt_cli

metan_cli:
	@cargo build --bin metan_cli $(CARGO_FLAGS)
	@cp target/$(RUST_PROFILE)/metan_cli bin/metan_cli
	@ln -sf metan_cli bin/metan

clean:
	@rm -rf ./bin/*

clean-crate:
	@cargo clean
