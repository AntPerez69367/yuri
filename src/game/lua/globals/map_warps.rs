use mlua::prelude::*;

use crate::database::map_db::{get_map_ptr, WarpList, BLOCK_SIZE, MAP_SLOTS};

use super::map_dimensions::loaded_map;

pub fn register(lua: &Lua) -> LuaResult<()> {
    let g = lua.globals();

    g.set("getWarp", lua.create_function(|_, (m, x, y): (i32, i32, i32)| {
        if m < 0 || m as usize >= MAP_SLOTS { return Ok(false); }
        let Some(md) = loaded_map(m) else { return Ok(false) };
        if md.warp.is_null() { return Ok(false); }
        let x = x.clamp(0, md.xs as i32 - 1) as usize;
        let y = y.clamp(0, md.ys as i32 - 1) as usize;
        let idx = x / BLOCK_SIZE + (y / BLOCK_SIZE) * md.bxs as usize;
        let mut node = unsafe { *md.warp.add(idx) };
        while !node.is_null() {
            let n = unsafe { &*node };
            if n.x == x as i32 && n.y == y as i32 {
                return Ok(true);
            }
            node = n.next;
        }
        Ok(false)
    })?)?;

    g.set("setWarps", lua.create_function(|_, (mm, mx, my, tm, tx, ty): (i32, i32, i32, i32, i32, i32)| {
        if mm < 0 || mm as usize >= MAP_SLOTS { return Ok(false); }
        if tm < 0 || tm as usize >= MAP_SLOTS { return Ok(false); }
        let mm_ptr = get_map_ptr(mm as u16);
        let tm_ptr = get_map_ptr(tm as u16);
        if mm_ptr.is_null() || tm_ptr.is_null() { return Ok(false); }
        let md = unsafe { &mut *mm_ptr };
        if md.xs == 0 || unsafe { (*tm_ptr).xs } == 0 { return Ok(false); }
        if mx < 0 || my < 0 || mx >= md.xs as i32 || my >= md.ys as i32 { return Ok(false); }
        if md.warp.is_null() { return Ok(false); }
        let idx = mx as usize / BLOCK_SIZE + (my as usize / BLOCK_SIZE) * md.bxs as usize;
        let existing = unsafe { *md.warp.add(idx) };
        let war = Box::into_raw(Box::new(WarpList {
            x: mx, y: my, tm, tx, ty,
            next: existing,
            prev: std::ptr::null_mut(),
        }));
        unsafe {
            if !existing.is_null() { (*existing).prev = war; }
            *md.warp.add(idx) = war;
        }
        Ok(true)
    })?)?;

    g.set("getWarps", lua.create_function(|lua, _m: i32| {
        tracing::warn!("[lua] getWarps: not yet implemented");
        lua.create_table()
    })?)?;

    Ok(())
}
