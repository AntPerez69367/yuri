#!make

CC ?= clang

all: clean libyuri cmake metan_cli decrypt_cli char_server login_server map_server
libyuri:
	@echo "libyuri:"
	@cargo build --lib
yuri.h:
	@cbindgen --config cbindgen.toml --crate yuri --output ./c_deps/yuri.h --lang c
cmake:
	@cmake -H. -Bbuild
common: deps
	@cmake --build build --target common --parallel --
deps: cmake libyuri
	@cmake --build build --target deps --parallel --
metan_cli: common
	@cmake --build build --target metan_cli --parallel --
	@ln -sf metan_cli bin/metan
decrypt_cli: common 
	@cmake --build build --target decrypt_cli --parallel --
char_server: common
	@cmake --build build --target char_server --parallel --
login_server: common
	@cmake --build build --target login_server --parallel --
map_server: common
	@cmake --build build --target map_server --parallel --
clean:
	@rm -rf ./bin/*
	@rm -rf ./build
clean-crate:
	@cargo clean