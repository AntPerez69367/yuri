use mlua::prelude::*;

/// Globals that are stubs, no-ops, or not yet implemented.
/// As each one gets a real implementation, move it to the appropriate module.
pub fn register(lua: &Lua) -> LuaResult<()> {
    let g = lua.globals();

    // ── No-ops (intentionally empty) ──
    g.set("getKanDonationPoints", lua.create_function(|_, ()| Ok(0i64))?)?;
    g.set("setKanDonationPoints", lua.create_function(|_, _: i32| Ok(()))?)?;
    g.set("addKanDonationPoints", lua.create_function(|_, _: i32| Ok(()))?)?;
    g.set("guitext", lua.create_function(|_, _: LuaMultiValue| Ok(()))?)?;
    g.set("setOfflinePlayerRegistry", lua.create_function(|_, _: LuaMultiValue| Ok(()))?)?;

    // ── Not yet implemented ──
    stub_warn(lua, "processKanDonations")?;
    stub_warn(lua, "addToBoard")?;
    stub_warn(lua, "selectBulletinBoard")?;
    stub_warn(lua, "copyPoemToPoetry")?;
    stub_warn(lua, "getClanRoster")?;

    stub_table(lua, "getMapModifiers")?;
    stub_table(lua, "getPoems")?;
    stub_table(lua, "getAuctions")?;
    stub_table(lua, "getSetItems")?;

    stub_warn(lua, "clearPoems")?;
    stub_warn(lua, "listAuction")?;
    stub_warn(lua, "removeAuction")?;

    // ── Not yet ported (need DB functions first) ──
    stub_warn(lua, "getSpellLevel")?;
    stub_warn(lua, "getMobAttributes")?;
    stub_warn(lua, "addMob")?;
    stub_warn(lua, "checkOnline")?;
    stub_warn(lua, "getOfflineID")?;
    stub_warn(lua, "addMapModifier")?;
    stub_warn(lua, "removeMapModifier")?;
    stub_warn(lua, "removeMapModifierId")?;
    stub_warn(lua, "getFreeMapModifierId")?;
    stub_warn(lua, "getWisdomStarMultiplier")?;
    stub_warn(lua, "setWisdomStarMultiplier")?;
    stub_warn(lua, "getClanTribute")?;
    stub_warn(lua, "setClanTribute")?;
    stub_warn(lua, "addClanTribute")?;
    stub_warn(lua, "getClanName")?;
    stub_warn(lua, "setClanName")?;
    stub_warn(lua, "getClanBankSlots")?;
    stub_warn(lua, "setClanBankSlots")?;
    stub_warn(lua, "removeClanMember")?;
    stub_warn(lua, "addClanMember")?;
    stub_warn(lua, "updateClanMemberRank")?;
    stub_warn(lua, "updateClanMemberTitle")?;
    stub_warn(lua, "removePathMember")?;
    stub_warn(lua, "addPathMember")?;
    stub_warn(lua, "getXPforLevel")?;

    Ok(())
}

/// Register a stub that logs a warning and returns nil.
fn stub_warn(lua: &Lua, name: &str) -> LuaResult<()> {
    let owned_name = name.to_owned();
    lua.globals().set(name, lua.create_function(move |_, _: LuaMultiValue| {
        tracing::warn!("[lua] {}: not yet implemented", owned_name);
        Ok(LuaValue::Nil)
    })?)?;
    Ok(())
}

/// Register a stub that logs a warning and returns an empty table.
fn stub_table(lua: &Lua, name: &str) -> LuaResult<()> {
    let owned_name = name.to_owned();
    lua.globals().set(name, lua.create_function(move |lua, _: LuaMultiValue| {
        tracing::warn!("[lua] {}: not yet implemented", owned_name);
        lua.create_table()
    })?)?;
    Ok(())
}
