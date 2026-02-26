use mlua::{UserData, UserDataMethods};
use std::os::raw::c_void;

pub struct NpcObject { pub ptr: *mut c_void }
unsafe impl Send for NpcObject {}

impl UserData for NpcObject {
    fn add_methods<M: UserDataMethods<Self>>(_methods: &mut M) {
        // TODO — Phase 3
    }
}
