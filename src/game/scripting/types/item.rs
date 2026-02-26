use mlua::{UserData, UserDataMethods};
use std::os::raw::c_void;

pub struct ItemObject     { pub ptr: *mut c_void }
pub struct BItemObject    { pub ptr: *mut c_void }
pub struct BankItemObject { pub ptr: *mut c_void }
pub struct ParcelObject   { pub ptr: *mut c_void }
pub struct RecipeObject   { pub ptr: *mut c_void }

unsafe impl Send for ItemObject {}
unsafe impl Send for BItemObject {}
unsafe impl Send for BankItemObject {}
unsafe impl Send for ParcelObject {}
unsafe impl Send for RecipeObject {}

macro_rules! stub_userdata {
    ($t:ty) => {
        impl UserData for $t {
            fn add_methods<M: UserDataMethods<Self>>(_: &mut M) {}
        }
    };
}
stub_userdata!(ItemObject);
stub_userdata!(BItemObject);
stub_userdata!(BankItemObject);
stub_userdata!(ParcelObject);
stub_userdata!(RecipeObject);
