use mlua::{UserData, UserDataMethods};
use std::os::raw::c_void;

pub struct FloorListObject { pub ptr: *mut c_void }
unsafe impl Send for FloorListObject {}

impl UserData for FloorListObject {
    fn add_methods<M: UserDataMethods<Self>>(_methods: &mut M) {
        // TODO — Phase 4
    }
}
