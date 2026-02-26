use mlua::{UserData, UserDataMethods};
use std::os::raw::c_void;

pub struct RegObject       { pub ptr: *mut c_void }
pub struct RegStringObject { pub ptr: *mut c_void }
pub struct NpcRegObject    { pub ptr: *mut c_void }
pub struct MobRegObject    { pub ptr: *mut c_void }
pub struct MapRegObject    { pub ptr: *mut c_void }
pub struct GameRegObject   { pub ptr: *mut c_void }
pub struct QuestRegObject  { pub ptr: *mut c_void }

unsafe impl Send for RegObject {}
unsafe impl Send for RegStringObject {}
unsafe impl Send for NpcRegObject {}
unsafe impl Send for MobRegObject {}
unsafe impl Send for MapRegObject {}
unsafe impl Send for GameRegObject {}
unsafe impl Send for QuestRegObject {}

macro_rules! stub_userdata {
    ($t:ty) => {
        impl UserData for $t {
            fn add_methods<M: UserDataMethods<Self>>(_: &mut M) {}
        }
    };
}
stub_userdata!(RegObject);
stub_userdata!(RegStringObject);
stub_userdata!(NpcRegObject);
stub_userdata!(MobRegObject);
stub_userdata!(MapRegObject);
stub_userdata!(GameRegObject);
stub_userdata!(QuestRegObject);
