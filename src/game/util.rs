/// Pure utility functions ported from c_src/sl_compat.c.
/// No C dependencies — safe to use anywhere in the Rust codebase.

use std::os::raw::c_int;

/// Map an equipment-type enum value (EQ_*) to the CLIF slot index sent in packets.
///
/// Matches `getclifslotfromequiptype` in sl_compat.c.
/// EQ_* enum values (from item_db.h, 0-based):
///   0=EQ_WEAP, 1=EQ_ARMOR, 2=EQ_SHIELD, 3=EQ_HELM,
///   4=EQ_LEFT, 5=EQ_RIGHT, 6=EQ_SUBLEFT, 7=EQ_SUBRIGHT,
///   8=EQ_FACEACC, 9=EQ_CROWN, 10=EQ_MANTLE, 11=EQ_NECKLACE,
///   12=EQ_BOOTS, 13=EQ_COAT
pub fn clif_slot_from_equip_type(equip_type: i32) -> i32 {
    match equip_type {
        0  => 0x01, // EQ_WEAP
        1  => 0x02, // EQ_ARMOR
        2  => 0x03, // EQ_SHIELD
        3  => 0x04, // EQ_HELM
        11 => 0x06, // EQ_NECKLACE
        4  => 0x07, // EQ_LEFT
        5  => 0x08, // EQ_RIGHT
        12 => 13,   // EQ_BOOTS
        10 => 14,   // EQ_MANTLE
        13 => 16,   // EQ_COAT
        6  => 20,   // EQ_SUBLEFT
        7  => 21,   // EQ_SUBRIGHT
        8  => 22,   // EQ_FACEACC
        9  => 23,   // EQ_CROWN
        _  => 0,
    }
}

/// Return `true` if `one` and `two` are on the same map and within `radius`
/// tiles on both axes.
///
/// Matches `CheckProximity(struct point one, struct point two, int radius)`.
/// Tuple fields: (map_id, x, y).
pub fn check_proximity(one: (i32, i32, i32), two: (i32, i32, i32), radius: i32) -> bool {
    one.0 == two.0
        && (one.1 - two.1).abs() <= radius
        && (one.2 - two.2).abs() <= radius
}

/// Truncate `s` to at most `max_len` bytes, returning a `&str` slice.
///
/// Matches `stringTruncate(char *buffer, int maxLength)`.
/// The C version writes `'\0'` at `buffer[maxLength]`; the Rust version
/// just returns a shorter slice — no allocation needed.
///
/// Panics if `max_len` falls inside a multi-byte UTF-8 character.
pub fn string_truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        &s[..max_len]
    }
}

/// Replace the first occurrence of `orig` in `s` with `rep`, returning the
/// result as a new `String`.  If `orig` is not found, returns a copy of `s`.
///
/// Matches `replace_str(char *str, char *orig, char *rep)` in sl_compat.c.
/// The C version used a 4096-byte static buffer; the Rust version allocates
/// exactly what is needed.
pub fn replace_first(s: &str, orig: &str, rep: &str) -> String {
    match s.find(orig) {
        Some(pos) => format!("{}{}{}", &s[..pos], rep, &s[pos + orig.len()..]),
        None => s.to_string(),
    }
}

// ---------------------------------------------------------------------------
// C ABI wrapper — called from combat.rs via `extern "C" { fn CheckProximity }`.
// ---------------------------------------------------------------------------

/// C-layout mirror of the `Point` struct declared in `src/game/map_parse/combat.rs`.
///
/// Note: `mmo.h`'s `struct point` uses `unsigned short` for all fields, but
/// `combat.rs`'s local `Point` uses `c_int`.  That discrepancy is pre-existing in
/// combat.rs.  Since `CheckProximity` has no C callers (all C code has been deleted),
/// the ABI is purely Rust↔Rust and both sides agree on `c_int`, so this is safe.
#[repr(C)]
pub struct CPoint {
    pub m: c_int,
    pub x: c_int,
    pub y: c_int,
}

/// C-callable wrapper around [`check_proximity`].
///
/// Called from `src/game/map_parse/combat.rs` via `extern "C"`.
#[no_mangle]
pub extern "C" fn CheckProximity(one: CPoint, two: CPoint, radius: c_int) -> c_int {
    let result = check_proximity(
        (one.m, one.x, one.y),
        (two.m, two.x, two.y),
        radius,
    );
    result as c_int
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clif_slot_known_values() {
        assert_eq!(clif_slot_from_equip_type(0), 0x01);  // EQ_WEAP
        assert_eq!(clif_slot_from_equip_type(11), 0x06); // EQ_NECKLACE
        assert_eq!(clif_slot_from_equip_type(12), 13);   // EQ_BOOTS
        assert_eq!(clif_slot_from_equip_type(9), 23);    // EQ_CROWN
        assert_eq!(clif_slot_from_equip_type(99), 0);    // unknown → 0
    }

    #[test]
    fn check_proximity_same_map() {
        assert!(check_proximity((1, 10, 10), (1, 10, 10), 0));
        assert!(check_proximity((1, 10, 10), (1, 13, 7), 3));
        assert!(!check_proximity((1, 10, 10), (1, 14, 10), 3));
    }

    #[test]
    fn check_proximity_different_map() {
        assert!(!check_proximity((1, 10, 10), (2, 10, 10), 100));
    }

    #[test]
    fn check_proximity_one_tile_offset_at_zero_radius() {
        assert!(!check_proximity((1, 10, 10), (1, 11, 10), 0));
    }

    #[test]
    fn string_truncate_shorter() {
        assert_eq!(string_truncate("hello", 10), "hello");
        assert_eq!(string_truncate("hello", 3), "hel");
        assert_eq!(string_truncate("hello", 5), "hello");
    }

    #[test]
    fn replace_first_found() {
        assert_eq!(replace_first("hello world", "world", "Rust"), "hello Rust");
    }

    #[test]
    fn replace_first_not_found() {
        assert_eq!(replace_first("hello world", "xyz", "Rust"), "hello world");
    }

    #[test]
    fn replace_first_only_first() {
        // Only the first occurrence should be replaced.
        assert_eq!(replace_first("aaa", "a", "b"), "baa");
    }
}
