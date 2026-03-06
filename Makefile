#!make

CC           ?= clang
NPROC        := $(shell nproc)
RUST_PROFILE ?= debug

ifeq ($(RUST_PROFILE),release)
CARGO_FLAGS  := --release
else
CARGO_FLAGS  :=
endif

# C source files are now compiled directly by cargo's build.rs (cc crate).
# No cmake step is needed.

all: binaries

libyuri:
	@echo "libyuri:"
	@cargo build --lib $(CARGO_FLAGS)

yuri.h:
	@cbindgen --config cbindgen.toml --crate yuri --output ./c_deps/yuri.h --lang c

# Build all binaries: cargo builds Rust binaries (build.rs compiles C game libs).
binaries:
	@cargo build --bin login_server --bin char_server $(CARGO_FLAGS)
	@cargo build --bin map_server --features map-game $(CARGO_FLAGS)
	@cp target/$(RUST_PROFILE)/login_server bin/login_server
	@cp target/$(RUST_PROFILE)/char_server bin/char_server
	@cp target/$(RUST_PROFILE)/map_server bin/map_server

# Individual targets kept for incremental builds.
char_server: libyuri
	@cargo build --bin char_server $(CARGO_FLAGS)
	@cp target/$(RUST_PROFILE)/char_server bin/char_server
login_server: libyuri
	@cargo build --bin login_server $(CARGO_FLAGS)
	@cp target/$(RUST_PROFILE)/login_server bin/login_server
map_server: libyuri
	@cargo build --bin map_server --features map-game $(CARGO_FLAGS)
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
	@rm -rf ./build

clean-crate:
	@cargo clean
