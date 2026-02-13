# Yuri MMORPG Server

## Project Goals

Provide a clean fork/drop-in replacement of the Mithia server that is 100% compatible with existing LUA files and database. Slowly improve the codebase from spaghetti C to C11 to Rust.

### Benefits over Mithia

- Builds/runs as a 64-bit binary on a modern toolchain
- Significantly cleaned up, unsafe C removed (buffer overflows, etc), ZERO COMPILER WARNINGS!
- Uses LuaJIT instead of interpreted Lua, providing 2-20x speed-up in lua execution
- Dead code is actively being removed. Existing code refactored and ported to Rust
- Eventual goal of async networking and moving database writes to an external thread for higher performance
- Fixes many confusing bugs, like NPCs not loading when there are gaps in the database

## Building from source

You will need to a C compiler for the Mithia Code. We currently target `clang`, but gcc should work fine as well.
You will need to install rust.

The C currently has the following external library dependencies that must be installed.

- libmysqlclient
- luajit-2.1
- zlib

If you plan on developing the rust, you will need to install `cbindgen` in order to regenerate yuri.h (`cargo install cbindgen`)

Once the dependencies are installed, just run `make all`

## Database Setup

Yuri uses **SQLx** for database migrations with compile-time checked SQL queries. SQLx provides type-safe database access and automatic schema migrations.

### Install SQLx CLI

```bash
cargo install sqlx-cli --features mysql
```

### Database Migration

#### Step 1: Create Database Manually

```bash
# Connect to MySQL/MariaDB as root
mysql -u root -p
```

```sql
-- Create database
CREATE DATABASE tk CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

-- Create user (change password!)
CREATE USER 'tk'@'localhost' IDENTIFIED BY 'your_secure_password';
CREATE USER 'tk'@'%' IDENTIFIED BY 'your_secure_password';

-- Grant privileges
GRANT ALL PRIVILEGES ON tk.* TO 'tk'@'localhost';
GRANT ALL PRIVILEGES ON tk.* TO 'tk'@'%';
FLUSH PRIVILEGES;

EXIT;
```

#### Step 2: Configure Environment

```bash
# Copy example config
cp .env.example .env

# Edit .env and update DATABASE_URL with your credentials
# DATABASE_URL=mysql://tk:your_secure_password@localhost:3306/tk
```

#### Step 3: Run Migrations

```bash
# Run migrations
sqlx migrate run --source migrations_sqlx

# Check migration status
sqlx migrate info --source migrations_sqlx
```

## Non Goals

- Will not accept any C++ code. The end goal is to port the C to Rust, and using C++ significantly complicates calling between the two languages
- Must be able to run Mithia lua code out of the box. New lua interfaces may be added, but must retain backwards compatability
- No copyrighted material - including client, game maps, or storyline. This is not a game, just a server emulator

## Migration from Mithia

If you have not modified the Mithia C source, Yuri is 100% drop-in compatible with your existing Lua, Maps, and Database.

1. Migrate your Mithia configuration to the `config` directory. All config files have merged into a single `server.yaml` and options have shrunk, as most unused variables have been removed.
2. Copy your maps from Mithia's `maps/Accepted` folder into the `data/maps` directory
3. Copy your lua from Mithia's `lua/Accepted` folder into the `data/lua` directory. There is no notion of Accepted/Developers/Deprecated in Yuri. This will be addressed in a future version with git integration.
4. Copy your sys.lua and scripts.lua from Mithia's `lua/Developers` folder into the `data/lua` directory. This file is the initial lua file loaded before all others.
5. If you've modified the levels_db.txt file, copy it into `data/`. The default values are included.
6. All logs are now written to `STDOUT` as structured logs instead of the filesystem. Run under systemd or docker to log to filesystem.

## Current Architecture

The current inherited architecture requires running 3 servers- the character server (handles character state + global state such as boards), the login server (handles logging in, creating new characters), and the map servers (runs the game world). You can run multiple map servers to handle subsets of the game-world and distribute the load.

The servers communicate with eachother via TCP with a custom protocol that is similar to the game client protocol.

Game state is stored in MySQL, maps and lua scripts are stored on disk.

### Issues with Current Codebase

- Tons of dead code, copy/pasted code, and just largely a mess (`sl.c` is 13K lines long!)
- Very few defined structures
- Difficult to follow logical flow between network parsing & proxying to other servers
- Some game logic implemented in C, some implemented in Lua
- Very unsafe C - thousands of unsafe casts and easily preventable buffer overflows cause the server to very segfault prone
- At a scale of 30-40 players, operators begin to see lag issues with no clear cause
- Runs as a single, serial thread! Code is too unsafe to run multi-threaded, but even slow/blocking processes are executed on the main thread.

## Cleanup TODO

- [x] Remove bundled zlib
- [x] Fix pointer to int casts for 64-bit support
- [x] Compile with Clang
- [x] clang-fmt/clang-modernize codebase
- [x] Clean up and document configs
- [x] Flatten source directory
- [x] All logging to STDOUT
- [x] Fix SQL autoincrement / numbering issues
- [x] Fix all clang warnings
- [x] Switch to LuaJIT
- [x] Unhardcode asset paths
- [ ] Compile with -O3 without segfaulting (almost there)
- [ ] Compile without stack smashing off (almost there)
- [ ] Remove dead code
- [ ] BCrypt Passwords
- [ ] Receive mysql / net config as cli flag and env
- [ ] Produce a single server binary instead of 3x
- [ ] Use OpenSSL MD5
- [ ] Unit Tests
- [ ] Automated CI
- [ ] Include SQL Migration & Minimum amount of LUA to run server

## Future State

- Capture performance metrics for slow lua, slow queries, etc.
- Fully defined structures for all client packets
- Concurrent packet parsing and networking
- Move all game logic into Lua
- Web editor
- Use HTTP or GRPC for internal server communication
- Allow for map server to be gracefully restarted without dropping clients or game state

# Updating client to point to your server

Using a Hex Editor (HxD, 010 Editor, etc.):

1. Open ddraw.dll in hex editor
2. Search for the original server IP (as ASCII string, e.g., game.nexon.com or 208.100.42.193)
3. Replace with your IP: 192.168.1.68 (pad with null bytes if shorter)
4. Search for port 07D0 (hex for 2000) if hardcoded
5. Save and test
