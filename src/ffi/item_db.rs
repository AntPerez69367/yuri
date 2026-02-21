//! FFI bridge for item database.

use std::os::raw::{c_char, c_int, c_uint};
use std::ptr::null_mut;

use crate::database::item_db as db;

macro_rules! ffi_catch {
    ($default:expr, $body:expr) => {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body)) {
            Ok(v) => v,
            Err(_) => $default,
        }
    };
}

#[no_mangle]
pub extern "C" fn rust_itemdb_init() -> c_int { ffi_catch!(-1, db::init()) }

#[no_mangle]
pub extern "C" fn rust_itemdb_term() { ffi_catch!((), db::term()) }

#[no_mangle]
pub extern "C" fn rust_itemdb_search(id: c_uint) -> *mut db::ItemData { ffi_catch!(null_mut(), db::search(id)) }

#[no_mangle]
pub extern "C" fn rust_itemdb_searchexist(id: c_uint) -> *mut db::ItemData { ffi_catch!(null_mut(), db::searchexist(id)) }

#[no_mangle]
pub extern "C" fn rust_itemdb_searchname(s: *const c_char) -> *mut db::ItemData { ffi_catch!(null_mut(), db::searchname(s)) }

#[no_mangle]
pub extern "C" fn rust_itemdb_id(s: *const c_char) -> c_uint {
    if s.is_null() { return 0; }
    ffi_catch!(0, {
        let ptr = db::searchname(s);
        if !ptr.is_null() {
            unsafe { (*ptr).id }
        } else {
            let str_val = unsafe { std::ffi::CStr::from_ptr(s) }.to_string_lossy();
            if let Ok(n) = str_val.trim().parse::<u32>() {
                if n > 0 {
                    let p = db::searchexist(n);
                    if !p.is_null() { unsafe { (*p).id } } else { 0 }
                } else { 0 }
            } else { 0 }
        }
    })
}

#[no_mangle]
pub extern "C" fn rust_itemdb_type(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).typ as c_int } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_name(id: c_uint) -> *mut c_char {
    ffi_catch!(null_mut(), { let p = db::search(id); if p.is_null() { null_mut() } else { unsafe { (*p).name.as_mut_ptr() } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_yname(id: c_uint) -> *mut c_char {
    ffi_catch!(null_mut(), { let p = db::search(id); if p.is_null() { null_mut() } else { unsafe { (*p).yname.as_mut_ptr() } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_text(id: c_uint) -> *mut c_char {
    ffi_catch!(null_mut(), { let p = db::search(id); if p.is_null() { null_mut() } else { unsafe { (*p).text.as_mut_ptr() } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_buytext(id: c_uint) -> *mut c_char {
    ffi_catch!(null_mut(), { let p = db::search(id); if p.is_null() { null_mut() } else { unsafe { (*p).buytext.as_mut_ptr() } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_price(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).price } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_sell(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).sell } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_rank(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).rank } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_stackamount(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).stack_amount } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_look(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).look } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_lookcolor(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).look_color } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_icon(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).icon } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_iconcolor(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).icon_color as c_int } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_sound(id: c_uint) -> c_uint {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).sound } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_soundhit(id: c_uint) -> c_uint {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).sound_hit } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_dura(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).dura } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_might(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).might } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_will(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).will } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_grace(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).grace } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_ac(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).ac } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_dam(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).dam } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_hit(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).hit } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_vita(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).vita } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_mana(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).mana } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_protection(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).protection } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_protected(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).protected } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_minSdam(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).min_sdam as c_int } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_maxSdam(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).max_sdam as c_int } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_minLdam(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).min_ldam as c_int } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_maxLdam(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).max_ldam as c_int } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_mindam(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).min_sdam as c_int } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_maxdam(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).max_sdam as c_int } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_mincritdam(_id: c_uint) -> c_int { 0 }
#[no_mangle]
pub extern "C" fn rust_itemdb_maxcritdam(_id: c_uint) -> c_int { 0 }
#[no_mangle]
pub extern "C" fn rust_itemdb_mightreq(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).mightreq } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_depositable(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).depositable } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_exchangeable(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).exchangeable } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_droppable(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).droppable } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_thrown(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).thrown } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_thrownconfirm(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).thrownconfirm } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_repairable(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).repairable } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_maxamount(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).max_amount } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_skinnable(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).skinnable } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_unequip(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).unequip as c_int } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_ethereal(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).ethereal as c_int } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_healing(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).healing } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_wisdom(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).wisdom } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_con(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).con } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_attackspeed(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).attack_speed } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_level(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).level as c_int } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_class(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).class as c_int } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_sex(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).sex as c_int } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_time(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).time as c_int } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_script(_id: c_uint) -> *mut c_char { null_mut() }
#[no_mangle]
pub extern "C" fn rust_itemdb_equipscript(_id: c_uint) -> *mut c_char { null_mut() }
#[no_mangle]
pub extern "C" fn rust_itemdb_unequipscript(_id: c_uint) -> *mut c_char { null_mut() }
#[no_mangle]
pub extern "C" fn rust_itemdb_breakondeath(id: c_uint) -> c_int {
    ffi_catch!(0, { let p = db::search(id); if p.is_null() { 0 } else { unsafe { (*p).bod } } })
}
#[no_mangle]
pub extern "C" fn rust_itemdb_reqvita(_id: c_uint) -> c_int { 0 }
#[no_mangle]
pub extern "C" fn rust_itemdb_reqmana(_id: c_uint) -> c_int { 0 }
#[no_mangle]
pub extern "C" fn rust_itemdb_dodge(_id: c_uint) -> c_int { 0 }
#[no_mangle]
pub extern "C" fn rust_itemdb_block(_id: c_uint) -> c_int { 0 }
#[no_mangle]
pub extern "C" fn rust_itemdb_parry(_id: c_uint) -> c_int { 0 }
#[no_mangle]
pub extern "C" fn rust_itemdb_resist(_id: c_uint) -> c_int { 0 }
#[no_mangle]
pub extern "C" fn rust_itemdb_physdeduct(_id: c_uint) -> c_int { 0 }
