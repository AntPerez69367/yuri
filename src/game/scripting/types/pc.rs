use mlua::{UserData, UserDataMethods};
use std::os::raw::c_void;

pub struct PcObject { pub ptr: *mut c_void }
unsafe impl Send for PcObject {}

impl UserData for PcObject {
    fn add_methods<M: UserDataMethods<Self>>(_methods: &mut M) {
        // TODO — Phase 6
    }
}
