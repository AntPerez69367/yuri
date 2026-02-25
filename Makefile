#!make

CC           ?= clang
NPROC        := $(shell nproc)
RUST_PROFILE ?= debug

ifeq ($(RUST_PROFILE),release)
CARGO_FLAGS     := --release
CMAKE_BUILD_TYPE := Release
else
CARGO_FLAGS     :=
CMAKE_BUILD_TYPE := Debug
endif

all: clean libyuri cmake binaries

libyuri:
	@echo "libyuri:"
	@cargo build --lib $(CARGO_FLAGS)

yuri.h:
	@cbindgen --config cbindgen.toml --crate yuri --output ./c_deps/yuri.h --lang c

cmake:
	@cmake -H. -Bbuild -DCMAKE_BUILD_TYPE=$(CMAKE_BUILD_TYPE)

common: deps
	@cmake --build build --target common --parallel $(NPROC)

deps: cmake libyuri yuri.h
	@cmake --build build --target deps --parallel $(NPROC)

# Build all binaries: cmake builds C game lib + CLI tools; cargo builds Rust binaries.
binaries: common
	@cmake --build build \
		--target metan_cli \
		--target decrypt_cli \
		--target map_game \
		--target common_nocore \
		--parallel $(NPROC)
	@ln -sf metan_cli bin/metan
	@cargo build --bin login_server --bin char_server --bin map_server $(CARGO_FLAGS)
	@cp target/$(RUST_PROFILE)/login_server bin/login_server
	@cp target/$(RUST_PROFILE)/char_server bin/char_server
	@cp target/$(RUST_PROFILE)/map_server bin/map_server

# Individual targets kept for incremental builds.
metan_cli: common
	@cmake --build build --target metan_cli --parallel $(NPROC)
	@ln -sf metan_cli bin/metan
decrypt_cli: common
	@cmake --build build --target decrypt_cli --parallel $(NPROC)
char_server: common
	@cmake --build build --target char_server --parallel $(NPROC)
char_server_rust: libyuri
	@cargo build --bin char_server $(CARGO_FLAGS)
	@cp target/$(RUST_PROFILE)/char_server bin/char_server
login_server: libyuri
	@cargo build --bin login_server $(CARGO_FLAGS)
	@cp target/$(RUST_PROFILE)/login_server bin/login_server
map_game: common
	@cmake --build build --target map_game --target common_nocore --parallel $(NPROC)
map_server: libyuri map_game
	@cargo build --bin map_server $(CARGO_FLAGS)
	@cp target/$(RUST_PROFILE)/map_server bin/map_server

clean:
	@rm -rf ./bin/*
	@rm -rf ./build

clean-crate:
	@cargo clean