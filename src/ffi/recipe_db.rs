//! FFI bridge for recipe/crafting database.

use std::os::raw::{c_char, c_int, c_uint};
use std::ptr::null_mut;

use crate::database::recipe_db::{self as db, RecipeData};

#[no_mangle]
pub extern "C" fn rust_recipedb_init() -> c_int { ffi_catch!(-1, db::init()) }

#[no_mangle]
pub extern "C" fn rust_recipedb_term() { ffi_catch!((), db::term()) }

#[no_mangle]
pub extern "C" fn rust_recipedb_search(id: c_uint) -> *mut RecipeData { ffi_catch!(null_mut(), db::search(id)) }

#[no_mangle]
pub extern "C" fn rust_recipedb_searchexist(id: c_uint) -> *mut RecipeData { ffi_catch!(null_mut(), db::searchexist(id)) }

#[no_mangle]
pub extern "C" fn rust_recipedb_searchname(s: *const c_char) -> *mut RecipeData {
    if s.is_null() { return null_mut(); }
    ffi_catch!(null_mut(), db::searchname(s))
}
