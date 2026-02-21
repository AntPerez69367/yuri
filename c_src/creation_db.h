#pragma once

#include "map_server.h"

// Item creation is driven by Lua scripts; no DB table or storage layer exists.
int createdb_start(USER *);
