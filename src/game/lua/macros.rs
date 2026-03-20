/// Generates field accessor registrations for an entity type.
///
/// Produces a `register_fields` method that registers `add_field_method_get`
/// (and `add_field_method_set` for writable fields) on the entity's
/// `TealDataFields`. mlua resolves these before `__index`, so known fields
/// bypass string matching entirely. tealr captures type info and doc strings
/// for API documentation generation.
///
/// Annotations (separated by `;`):
///
///   `@direct "doc" name: Type => |arc| expr;`
///     No lock. Closure receives `&Arc<Entity>`. For fields directly on the
///     entity struct or atomic loads. Read-only.
///
///   `@read "doc" name: Type => |guard| expr;`
///     Acquires read lock. Closure receives the guard reference. Guard is
///     explicitly dropped before returning. Read-only.
///
///   `@read_write "doc" name: Type => |guard| expr, |guard, val| body;`
///     Readable (read lock) and writable (write lock). First closure reads,
///     second receives the guard and `val` (already converted from Lua) and
///     performs the assignment.
///
/// # Example
///
/// ```ignore
/// define_fields!(LuaMob, EntityType::Mob, map_id2mob_ref, {
///     @read "Minimum damage" minDam: i64 => |g| g.mindam as i64;
///     @read_write "Current HP" hp: i32 => |g| g.hp, |g, val| { g.hp = val; }
/// });
///
/// define_fields!(LuaPlayer, EntityType::Player, map_id2sd_pc, {
///     @direct "Character name" name: String => |arc| arc.name.clone();
///     @direct "Current health (atomic)" health: i32 => |arc| arc.hp_atomic.load(Ordering::Relaxed);
///     @read "Base level" level: u16 => |g| g.player.progression.level;
///     @read_write "Currency" money: u32 => |g| g.player.money, |g, val| { g.player.money = val; }
/// });
/// ```
#[macro_export]
macro_rules! define_fields {
    ($entity:ident, $etype:expr, $lookup:path, { $($rest:tt)* }) => {
        $crate::define_fields!(@parse $entity, $etype, $lookup,
            directs: []
            reads: []
            read_writes: []
            rest: [ $($rest)* ]
        );
    };

    // ── Parse @direct (with trailing ;) ──
    (@parse $entity:ident, $etype:expr, $lookup:path,
        directs: [ $($directs:tt)* ]
        reads: [ $($reads:tt)* ]
        read_writes: [ $($rws:tt)* ]
        rest: [ @direct $doc:literal $name:ident : $ty:ty => |$arc:ident| $expr:expr ; $($tail:tt)* ]
    ) => {
        $crate::define_fields!(@parse $entity, $etype, $lookup,
            directs: [ $($directs)* ($doc, $name, $ty, $arc, $expr) ]
            reads: [ $($reads)* ]
            read_writes: [ $($rws)* ]
            rest: [ $($tail)* ]
        );
    };
    // ── Parse @direct (last, no trailing ;) ──
    (@parse $entity:ident, $etype:expr, $lookup:path,
        directs: [ $($directs:tt)* ]
        reads: [ $($reads:tt)* ]
        read_writes: [ $($rws:tt)* ]
        rest: [ @direct $doc:literal $name:ident : $ty:ty => |$arc:ident| $expr:expr ]
    ) => {
        $crate::define_fields!(@parse $entity, $etype, $lookup,
            directs: [ $($directs)* ($doc, $name, $ty, $arc, $expr) ]
            reads: [ $($reads)* ]
            read_writes: [ $($rws)* ]
            rest: [ ]
        );
    };

    // ── Parse @read (with trailing ;) ──
    (@parse $entity:ident, $etype:expr, $lookup:path,
        directs: [ $($directs:tt)* ]
        reads: [ $($reads:tt)* ]
        read_writes: [ $($rws:tt)* ]
        rest: [ @read $doc:literal $name:ident : $ty:ty => |$g:ident| $expr:expr ; $($tail:tt)* ]
    ) => {
        $crate::define_fields!(@parse $entity, $etype, $lookup,
            directs: [ $($directs)* ]
            reads: [ $($reads)* ($doc, $name, $ty, $g, $expr) ]
            read_writes: [ $($rws)* ]
            rest: [ $($tail)* ]
        );
    };
    // ── Parse @read (last) ──
    (@parse $entity:ident, $etype:expr, $lookup:path,
        directs: [ $($directs:tt)* ]
        reads: [ $($reads:tt)* ]
        read_writes: [ $($rws:tt)* ]
        rest: [ @read $doc:literal $name:ident : $ty:ty => |$g:ident| $expr:expr ]
    ) => {
        $crate::define_fields!(@parse $entity, $etype, $lookup,
            directs: [ $($directs)* ]
            reads: [ $($reads)* ($doc, $name, $ty, $g, $expr) ]
            read_writes: [ $($rws)* ]
            rest: [ ]
        );
    };

    // ── Parse @read_write (with trailing ;) ──
    (@parse $entity:ident, $etype:expr, $lookup:path,
        directs: [ $($directs:tt)* ]
        reads: [ $($reads:tt)* ]
        read_writes: [ $($rws:tt)* ]
        rest: [ @read_write $doc:literal $name:ident : $ty:ty => |$rg:ident| $rexpr:expr , |$wg:ident , $wval:ident| $wbody:block ; $($tail:tt)* ]
    ) => {
        $crate::define_fields!(@parse $entity, $etype, $lookup,
            directs: [ $($directs)* ]
            reads: [ $($reads)* ]
            read_writes: [ $($rws)* ($doc, $name, $ty, $rg, $rexpr, $wg, $wval, $wbody) ]
            rest: [ $($tail)* ]
        );
    };
    // ── Parse @read_write (last) ──
    (@parse $entity:ident, $etype:expr, $lookup:path,
        directs: [ $($directs:tt)* ]
        reads: [ $($reads:tt)* ]
        read_writes: [ $($rws:tt)* ]
        rest: [ @read_write $doc:literal $name:ident : $ty:ty => |$rg:ident| $rexpr:expr , |$wg:ident , $wval:ident| $wbody:block ]
    ) => {
        $crate::define_fields!(@parse $entity, $etype, $lookup,
            directs: [ $($directs)* ]
            reads: [ $($reads)* ]
            read_writes: [ $($rws)* ($doc, $name, $ty, $rg, $rexpr, $wg, $wval, $wbody) ]
            rest: [ ]
        );
    };

    // ── Terminal: emit impl ──
    (@parse $entity:ident, $etype:expr, $lookup:path,
        directs: [ $( ($d_doc:literal, $d_name:ident, $d_ty:ty, $d_arc:ident, $d_expr:expr) )* ]
        reads: [ $( ($r_doc:literal, $r_name:ident, $r_ty:ty, $r_g:ident, $r_expr:expr) )* ]
        read_writes: [ $( ($rw_doc:literal, $rw_name:ident, $rw_ty:ty, $rw_rg:ident, $rw_rexpr:expr, $rw_wg:ident, $rw_wval:ident, $rw_wbody:block) )* ]
        rest: [ ]
    ) => {
        impl $entity {
            /// Register field accessors on the UserData.
            /// Call this from `add_fields` in your `TealData` impl.
            pub fn register_fields<F: tealr::mlu::TealDataFields<Self>>(fields: &mut F) {
                // @direct — no lock, read-only
                $(
                    fields.document(&format!("{} *(read-only)*", $d_doc));
                    fields.add_field_method_get(stringify!($d_name), |_, this| {
                        let $d_arc = $lookup(this.id)
                            .ok_or_else(|| $crate::game::lua::error::entity_not_found($etype, this.id))?;
                        let val: $d_ty = $d_expr;
                        Ok(val)
                    });
                )*

                // @read — read lock, read-only
                $(
                    fields.document(&format!("{} *(read-only)*", $r_doc));
                    fields.add_field_method_get(stringify!($r_name), |_, this| {
                        let arc = $lookup(this.id)
                            .ok_or_else(|| $crate::game::lua::error::entity_not_found($etype, this.id))?;
                        let $r_g = arc.read();
                        let val: $r_ty = $r_expr;
                        drop($r_g);
                        Ok(val)
                    });
                )*

                // @read_write — read lock for get, write lock for set
                $(
                    fields.document(&format!("{} *(get)*", $rw_doc));
                    fields.add_field_method_get(stringify!($rw_name), |_, this| {
                        let arc = $lookup(this.id)
                            .ok_or_else(|| $crate::game::lua::error::entity_not_found($etype, this.id))?;
                        let $rw_rg = arc.read();
                        let val: $rw_ty = $rw_rexpr;
                        drop($rw_rg);
                        Ok(val)
                    });
                    fields.document(&format!("{} *(set)*", $rw_doc));
                    fields.add_field_method_set(stringify!($rw_name), |_, this, $rw_wval: $rw_ty| {
                        let arc = $lookup(this.id)
                            .ok_or_else(|| $crate::game::lua::error::entity_not_found($etype, this.id))?;
                        let mut $rw_wg = arc.write();
                        $rw_wbody
                        drop($rw_wg);
                        Ok(())
                    });
                )*
            }
        }
    };
}
