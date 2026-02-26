use mlua::{UserData, UserDataMethods};
use std::os::raw::c_void;

pub struct MobObject { pub ptr: *mut c_void }
unsafe impl Send for MobObject {}

impl UserData for MobObject {
    fn add_methods<M: UserDataMethods<Self>>(_methods: &mut M) {
        // TODO — Phase 5
    }
}
