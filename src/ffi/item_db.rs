//! FFI bridge for item database.

use std::os::raw::{c_char, c_int, c_uint};
use std::ptr::null_mut;

use crate::database::item_db as db;

#[no_mangle]
pub extern "C" fn rust_itemdb_init() -> c_int {
    db::init()
}

#[no_mangle]
pub extern "C" fn rust_itemdb_term() {
    db::term()
}

#[no_mangle]
pub extern "C" fn rust_itemdb_search(id: c_uint) -> *mut db::ItemData {
    db::search(id)
}

#[no_mangle]
pub extern "C" fn rust_itemdb_searchexist(id: c_uint) -> *mut db::ItemData {
    db::searchexist(id)
}

#[no_mangle]
pub extern "C" fn rust_itemdb_searchname(s: *const c_char) -> *mut db::ItemData {
    db::searchname(s)
}

#[no_mangle]
pub extern "C" fn rust_itemdb_id(s: *const c_char) -> c_uint {
    let ptr = db::searchname(s);
    if !ptr.is_null() {
        return unsafe { (*ptr).id };
    }
    let str_val = unsafe { std::ffi::CStr::from_ptr(s) }.to_string_lossy();
    if let Ok(n) = str_val.trim().parse::<u32>() {
        if n > 0 {
            let p = db::searchexist(n);
            if !p.is_null() {
                return unsafe { (*p).id };
            }
        }
    }
    0
}

#[no_mangle]
pub extern "C" fn rust_itemdb_type(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).typ as c_int }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_name(id: c_uint) -> *mut c_char {
    unsafe { (*db::search(id)).name.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_yname(id: c_uint) -> *mut c_char {
    unsafe { (*db::search(id)).yname.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_text(id: c_uint) -> *mut c_char {
    unsafe { (*db::search(id)).text.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_buytext(id: c_uint) -> *mut c_char {
    unsafe { (*db::search(id)).buytext.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_price(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).price }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_sell(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).sell }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_rank(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).rank }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_stackamount(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).stack_amount }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_look(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).look }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_lookcolor(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).look_color }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_icon(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).icon }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_iconcolor(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).icon_color as c_int }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_sound(id: c_uint) -> c_uint {
    unsafe { (*db::search(id)).sound }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_soundhit(id: c_uint) -> c_uint {
    unsafe { (*db::search(id)).sound_hit }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_dura(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).dura }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_might(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).might }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_will(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).will }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_grace(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).grace }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_ac(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).ac }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_dam(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).dam }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_hit(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).hit }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_vita(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).vita }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_mana(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).mana }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_protection(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).protection }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_protected(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).protected }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_minSdam(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).min_sdam as c_int }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_maxSdam(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).max_sdam as c_int }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_minLdam(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).min_ldam as c_int }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_maxLdam(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).max_ldam as c_int }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_mindam(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).dam }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_maxdam(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).dam }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_mincritdam(_id: c_uint) -> c_int {
    0
}
#[no_mangle]
pub extern "C" fn rust_itemdb_maxcritdam(_id: c_uint) -> c_int {
    0
}
#[no_mangle]
pub extern "C" fn rust_itemdb_mightreq(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).mightreq }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_depositable(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).depositable }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_exchangeable(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).exchangeable }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_droppable(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).droppable }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_thrown(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).thrown }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_thrownconfirm(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).thrownconfirm }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_repairable(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).repairable }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_maxamount(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).max_amount }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_skinnable(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).skinnable }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_unequip(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).unequip as c_int }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_ethereal(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).ethereal as c_int }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_healing(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).healing }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_wisdom(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).wisdom }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_con(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).con }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_attackspeed(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).attack_speed }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_level(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).level as c_int }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_class(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).class as c_int }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_sex(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).sex as c_int }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_time(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).time as c_int }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_script(_id: c_uint) -> *mut c_char {
    null_mut()
}
#[no_mangle]
pub extern "C" fn rust_itemdb_equipscript(_id: c_uint) -> *mut c_char {
    null_mut()
}
#[no_mangle]
pub extern "C" fn rust_itemdb_unequipscript(_id: c_uint) -> *mut c_char {
    null_mut()
}
#[no_mangle]
pub extern "C" fn rust_itemdb_breakondeath(id: c_uint) -> c_int {
    unsafe { (*db::search(id)).bod }
}
#[no_mangle]
pub extern "C" fn rust_itemdb_reqvita(_id: c_uint) -> c_int {
    0
}
#[no_mangle]
pub extern "C" fn rust_itemdb_reqmana(_id: c_uint) -> c_int {
    0
}
#[no_mangle]
pub extern "C" fn rust_itemdb_dodge(_id: c_uint) -> c_int {
    0
}
#[no_mangle]
pub extern "C" fn rust_itemdb_block(_id: c_uint) -> c_int {
    0
}
#[no_mangle]
pub extern "C" fn rust_itemdb_parry(_id: c_uint) -> c_int {
    0
}
#[no_mangle]
pub extern "C" fn rust_itemdb_resist(_id: c_uint) -> c_int {
    0
}
#[no_mangle]
pub extern "C" fn rust_itemdb_physdeduct(_id: c_uint) -> c_int {
    0
}
