# Yuri MMORPG Server

## Project Overview

This fork of Yuri has detached from the existing abandoned Fork. This project is now fully in Rust and is generally functional, however many features are broken right now due to the incompatability between C patterns and idiomatic rust.

## Status

Yuri is **fully migrated to Rust** — zero C code remains. The codebase is currently undergoing modernization and bugfixing. Many patterns inherited from the original C codebase (raw pointers, `unsafe` blocks, C-style returns) are being replaced with idiomatic Rust. See [MODERNIZATION_PLAN.md](.claude/MODERNIZATION_PLAN.md) for the detailed roadmap.

**Current stats:**

- ~54,500 lines of Rust across 85 source files
- Single-threaded game loop on Tokio `LocalSet`, separate runtime for database I/O
- SQLx for type-safe database access with compile-time checked queries
- LuaJIT scripting via `mlua`
- Structured logging via `tracing`

## Building from Source

### Prerequisites

- **Rust 1.93+** (install via [rustup](https://rustup.rs/))
- **MySQL/MariaDB** for game state storage
- **SQLx CLI** for database migrations

### Build

```bash
cargo build --release
```

This produces three server binaries:

- `login_server` — handles authentication and character creation
- `char_server` — manages character state and global systems (boards, etc.)
- `map_server` — runs the game world (can run multiple instances for load distribution)

And two CLI utilities:

- `decrypt_cli` — packet decryption tool
- `metan_cli` — metadata CLI tool

## Database Setup

### Install SQLx CLI

```bash
cargo install sqlx-cli --features mysql
```

### Database Migration

#### Step 1: Create Database

```bash
mysql -u root -p
```

```sql
CREATE DATABASE tk CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

CREATE USER 'tk'@'localhost' IDENTIFIED BY 'your_secure_password';
CREATE USER 'tk'@'%' IDENTIFIED BY 'your_secure_password';

GRANT ALL PRIVILEGES ON tk.* TO 'tk'@'localhost';
GRANT ALL PRIVILEGES ON tk.* TO 'tk'@'%';
FLUSH PRIVILEGES;
```

#### Step 2: Configure Environment

```bash
cp .env.example .env
# Edit .env and set DATABASE_URL=mysql://tk:your_secure_password@localhost:3306/tk
```

#### Step 3: Run Migrations

```bash
sqlx migrate run --source migrations_sqlx
sqlx migrate info --source migrations_sqlx
```

## Configuration

All server configuration lives in `conf/server.yaml`. This single file covers database credentials, server IPs/ports, and authentication tokens.

## Architecture

The server uses a three-process architecture inherited from the original Mithia design:

1. **Login Server** — authenticates clients, handles character creation
2. **Character Server** — persists character state, manages global features (message boards, etc.)
3. **Map Server(s)** — runs the game world; multiple instances can each handle a subset of maps

The servers communicate via TCP using a custom binary protocol similar to the game client protocol. Game state is stored in MySQL; maps and Lua scripts are loaded from disk.

## Migration from Mithia

Yuri is compatible with existing Mithia Lua scripts, maps, and databases.

1. Migrate your configuration to `conf/server.yaml` (all config files have been merged into one)
2. Copy maps from Mithia's `maps/Accepted/` to `data/maps/`
3. Copy Lua scripts from Mithia's `lua/Accepted/` to `data/lua/`
4. Copy `sys.lua` and `scripts.lua` from Mithia's `lua/Developers/` to `data/lua/`
5. If you've modified `levels_db.txt`, copy it to `data/`
6. Logs are written to `STDOUT` as structured output. Use systemd or Docker for filesystem logging.

## Non-Goals

- Must retain backwards compatibility with existing Mithia Lua scripts. New Lua interfaces may be added, but existing ones must continue to work.
- No copyrighted material — no client, game maps, or storyline. This is a server emulator, not a game.

## Updating Client

Using a hex editor (HxD, 010 Editor, etc.):

1. Open `ddraw.dll` in the hex editor
2. Search for the original server IP (ASCII string)
3. Replace with your server IP (pad with null bytes if shorter)
4. Search for port `07D0` (hex for 2000) if hardcoded
5. Save and test
