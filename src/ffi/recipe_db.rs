//! FFI bridge for recipe/crafting database.

use std::os::raw::{c_char, c_int, c_uint};

use crate::database::recipe_db::{self as db, RecipeData};

#[no_mangle]
pub extern "C" fn rust_recipedb_init() -> c_int { db::init() }

#[no_mangle]
pub extern "C" fn rust_recipedb_term() { db::term() }

#[no_mangle]
pub extern "C" fn rust_recipedb_search(id: c_uint) -> *mut RecipeData { db::search(id) }

#[no_mangle]
pub extern "C" fn rust_recipedb_searchexist(id: c_uint) -> *mut RecipeData { db::searchexist(id) }

#[no_mangle]
pub extern "C" fn rust_recipedb_searchname(s: *const c_char) -> *mut RecipeData { db::searchname(s) }
