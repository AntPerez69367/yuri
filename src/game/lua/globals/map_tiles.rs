use mlua::prelude::*;

use crate::database::map_db::{map_data_mut, MapData};
use super::map_dimensions::loaded_map;

/// Bounds-checked tile index, or None if out of range.
fn tile_idx(md: &MapData, x: i32, y: i32) -> Option<usize> {
    if x < 0 || y < 0 || x >= md.xs as i32 || y >= md.ys as i32 {
        return None;
    }
    Some((x + y * md.xs as i32) as usize)
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    let g = lua.globals();

    g.set("getObjectsMap", lua.create_function(|lua, _: LuaMultiValue| {
        lua.create_table()
    })?)?;

    g.set("getObject", lua.create_function(|_, (m, x, y): (i32, i32, i32)| {
        let Some(md) = loaded_map(m) else { return Ok(None) };
        let Some(idx) = tile_idx(md, x, y) else { return Ok(None) };
        Ok(Some(unsafe { *md.obj.add(idx) as i64 }))
    })?)?;

    g.set("setObject", lua.create_function(|_, (m, x, y, val): (i32, i32, i32, i32)| {
        let Some(md) = map_data_mut(m as usize).filter(|md| !md.registry.is_null()) else { return Ok(()) };
        let Some(idx) = tile_idx(md, x, y) else { return Ok(()) };
        unsafe { *md.obj.add(idx) = val as u16; }
        Ok(())
    })?)?;

    g.set("getTile", lua.create_function(|_, (m, x, y): (i32, i32, i32)| {
        let Some(md) = loaded_map(m) else { return Ok(None) };
        let Some(idx) = tile_idx(md, x, y) else { return Ok(None) };
        Ok(Some(unsafe { *md.tile.add(idx) as i64 }))
    })?)?;

    g.set("setTile", lua.create_function(|_, (m, x, y, val): (i32, i32, i32, i32)| {
        let Some(md) = map_data_mut(m as usize).filter(|md| !md.registry.is_null()) else { return Ok(()) };
        let Some(idx) = tile_idx(md, x, y) else { return Ok(()) };
        unsafe { *md.tile.add(idx) = val as u16; }
        Ok(())
    })?)?;

    g.set("getPass", lua.create_function(|_, (m, x, y): (i32, i32, i32)| {
        let Some(md) = loaded_map(m) else { return Ok(None) };
        let Some(idx) = tile_idx(md, x, y) else { return Ok(None) };
        Ok(Some(unsafe { *md.pass.add(idx) as i64 }))
    })?)?;

    g.set("setPass", lua.create_function(|_, (m, x, y, val): (i32, i32, i32, i32)| {
        let Some(md) = map_data_mut(m as usize).filter(|md| !md.registry.is_null()) else { return Ok(()) };
        let Some(idx) = tile_idx(md, x, y) else { return Ok(()) };
        unsafe { *md.pass.add(idx) = val as u16; }
        Ok(())
    })?)?;

    Ok(())
}
