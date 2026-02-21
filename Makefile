#!make

CC    ?= clang
NPROC := $(shell nproc)

all: clean libyuri cmake binaries

libyuri:
	@echo "libyuri:"
	@cargo build --lib

yuri.h:
	@cbindgen --config cbindgen.toml --crate yuri --output ./c_deps/yuri.h --lang c

cmake:
	@cmake -H. -Bbuild

common: deps
	@cmake --build build --target common --parallel $(NPROC)

deps: cmake libyuri yuri.h
	@cmake --build build --target deps --parallel $(NPROC)

# Build all five binaries in one cmake invocation so the internal parallel
# scheduler can saturate cores across targets simultaneously.
binaries: common
	@cmake --build build \
		--target metan_cli \
		--target decrypt_cli \
		--target char_server \
		--target login_server \
		--target map_server \
		--parallel $(NPROC)
	@ln -sf metan_cli bin/metan

# Individual targets kept for incremental builds.
metan_cli: common
	@cmake --build build --target metan_cli --parallel $(NPROC)
	@ln -sf metan_cli bin/metan
decrypt_cli: common
	@cmake --build build --target decrypt_cli --parallel $(NPROC)
char_server: common
	@cmake --build build --target char_server --parallel $(NPROC)
login_server: common
	@cmake --build build --target login_server --parallel $(NPROC)
map_server: common
	@cmake --build build --target map_server --parallel $(NPROC)

clean:
	@rm -rf ./bin/*
	@rm -rf ./build

clean-crate:
	@cargo clean