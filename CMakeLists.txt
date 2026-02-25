cmake_minimum_required(VERSION 2.8)

project(yuri)

# Derive Rust output directory from CMAKE_BUILD_TYPE (default: debug)
if(CMAKE_BUILD_TYPE STREQUAL "Release")
  set(RUST_PROFILE "release")
else()
  set(RUST_PROFILE "debug")
endif()

set(CMAKE_BINARY_DIR ${CMAKE_SOURCE_DIR}/bin)
set(EXECUTABLE_OUTPUT_PATH ${CMAKE_BINARY_DIR})
set(LIBRARY_OUTPUT_PATH ${CMAKE_BINARY_DIR})

set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -std=gnu17 -pipe -Wall -Werror=implicit-function-declaration -Werror=unused-variable -Werror=return-type")
set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -g3 -DDEBUG -DFD_SETSIZE=1024")

# Currently has to be build with stack protecter turned off because of numerous buffer overflows.
# Eventually disable this
set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -fno-stack-protector")

find_package(ZLIB)
# find_package(Lua5.1)

# For LuaJIT
find_package(PkgConfig REQUIRED) 
pkg_check_modules(LUAJIT REQUIRED IMPORTED_TARGET luajit)

include_directories(${LUA_INCLUDE_DIR})

include_directories("${PROJECT_SOURCE_DIR}/c_deps")
include_directories("${PROJECT_SOURCE_DIR}/c_src")
add_subdirectory("${PROJECT_SOURCE_DIR}/c_deps")

add_library(common STATIC c_src/config.c c_src/core.c c_src/net_crypt.c)

# common_nocore: same as common but without core.c (no main()).
# Used by the Rust map_server binary which provides its own main().
add_library(common_nocore STATIC c_src/config.c c_src/net_crypt.c)

# libyuri.a and libcommon.a have circular symbol dependencies (Rust config FFI
# references C globals defined in common). --start-group/--end-group tells the
# linker to make multiple passes until all references are resolved.
set(YURI_LIB ${PROJECT_SOURCE_DIR}/target/${RUST_PROFILE}/libyuri.a)
set(YURI_LINK_GROUP -Wl,--start-group common ${YURI_LIB} deps -Wl,--end-group)

add_executable(char_server c_src/char_db.c c_src/char_server.c c_src/char_login.c c_src/char_map.c)
target_link_libraries(char_server ${YURI_LINK_GROUP} ZLIB::ZLIB m dl pthread)

# login_server is now a Rust binary (src/bin/login_server.rs); C files removed

# map_server is now a Rust binary (src/bin/map_server.rs).
# map_game contains all C game-logic files that Rust links against.
add_library(map_game STATIC
    c_src/map_char.c
    c_src/gm_command.c
    c_src/creation_db.c
    c_src/map_parse.c
    c_src/map_server.c
    c_src/mob.c
    c_src/npc.c
    c_src/pc.c
    c_src/scripting.c
)
target_link_libraries(map_game ${YURI_LINK_GROUP} ZLIB::ZLIB PkgConfig::LUAJIT m dl pthread)

add_executable(decrypt_cli c_src/decrypt_cli.c)
target_link_libraries(decrypt_cli ${YURI_LINK_GROUP} m dl pthread)

add_executable(metan_cli c_src/metan_cli.c)
target_link_libraries(metan_cli ${YURI_LINK_GROUP} m dl pthread)