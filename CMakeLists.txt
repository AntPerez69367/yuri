cmake_minimum_required(VERSION 2.8)

project(yuri)

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

add_library(common STATIC c_src/config.c c_src/core.c c_src/net_crypt.c c_src/session.c)

add_executable(char_server c_src/char_db.c c_src/char_server.c c_src/char_login.c c_src/char_map.c)
target_link_libraries(char_server common)
target_link_libraries(char_server deps)
target_link_libraries(char_server ZLIB::ZLIB)
target_link_libraries(char_server ${PROJECT_SOURCE_DIR}/target/debug/libyuri.a m dl pthread)

add_executable(login_server c_src/login_server.c c_src/login_char.c c_src/login_parse.c)
target_link_libraries(login_server common)
target_link_libraries(login_server deps)
target_link_libraries(login_server ZLIB::ZLIB)
target_link_libraries(login_server ${PROJECT_SOURCE_DIR}/target/debug/libyuri.a m dl pthread)

add_executable(map_server c_src/board_db.c c_src/clan_db.c c_src/class_db.c c_src/map_char.c c_src/gm_command.c c_src/creation_db.c c_src/map_parse.c c_src/item_db.c c_src/magic_db.c c_src/map_server.c c_src/mob.c c_src/npc.c c_src/pc.c c_src/recipe_db.c c_src/scripting.c)
target_link_libraries(map_server common)
target_link_libraries(map_server deps)
target_link_libraries(map_server ZLIB::ZLIB)
target_link_libraries(map_server PkgConfig::LUAJIT)
target_link_libraries(map_server ${PROJECT_SOURCE_DIR}/target/debug/libyuri.a m dl pthread)

add_executable(decrypt_cli c_src/decrypt_cli.c)
target_link_libraries(decrypt_cli common)
target_link_libraries(decrypt_cli ${PROJECT_SOURCE_DIR}/target/debug/libyuri.a m dl pthread)
target_link_libraries(decrypt_cli deps)

add_executable(metan_cli c_src/metan_cli.c c_src/item_db.c c_src/class_db.c)
target_link_libraries(metan_cli common)
target_link_libraries(metan_cli deps)
target_link_libraries(metan_cli ${PROJECT_SOURCE_DIR}/target/debug/libyuri.a m dl pthread)