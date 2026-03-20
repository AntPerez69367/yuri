//! Static object collision-flag table loaded from `static_objects.tbl`.

use std::sync::OnceLock;

use crate::config::config;

static OBJECT_FLAGS: OnceLock<Box<[u8]>> = OnceLock::new();

/// Access the object collision flags table.
/// Returns `None` if `object_flag_init` hasn't been called yet.
pub fn object_flags() -> Option<&'static [u8]> {
    OBJECT_FLAGS.get().map(|b| b.as_ref())
}

/// Load the static object flag table from `static_objects.tbl`.
///
/// # Safety
/// Must be called during server init (single-threaded).
pub unsafe fn object_flag_init() -> i32 {
    let filename = b"static_objects.tbl\0";
    let dir_bytes = config().data_dir.as_bytes();

    let mut path_bytes: Vec<u8> = Vec::with_capacity(dir_bytes.len() + filename.len() - 1);
    path_bytes.extend_from_slice(dir_bytes);
    path_bytes.extend_from_slice(&filename[..filename.len() - 1]);
    let path_cstr = match std::ffi::CString::new(path_bytes) {
        Ok(s) => s,
        Err(_) => {
            tracing::error!("[map] [object_flag_init] path contains interior nul byte");
            std::process::exit(1);
        }
    };

    let path_str = path_cstr.to_string_lossy();
    println!("[map] [object_flag_init] reading static obj table path={}", path_str);

    let fi = libc::fopen(path_cstr.as_ptr(), c"rb".as_ptr());
    if fi.is_null() {
        tracing::error!("[map] [error] cannot read static object table path={}", path_str);
        std::process::exit(1);
    }

    let mut num: i32 = 0;
    libc::fread(std::ptr::addr_of_mut!(num).cast(), 4, 1, fi);

    let flags_vec: Vec<u8> = vec![0u8; (num as usize) + 1];
    let _ = OBJECT_FLAGS.set(flags_vec.into_boxed_slice());

    let mut flag: i8 = 0;
    libc::fread(std::ptr::addr_of_mut!(flag).cast(), 1, 1, fi);

    let mut _z: i32 = 1;
    while libc::feof(fi) == 0 {
        let mut count: i8 = 0;
        libc::fread(std::ptr::addr_of_mut!(count).cast(), 1, 1, fi);
        let mut remaining = count;
        while remaining != 0 {
            let mut tile: i16 = 0;
            libc::fread(std::ptr::addr_of_mut!(tile).cast(), 2, 1, fi);
            remaining -= 1;
        }

        let mut nothing = [0u8; 5];
        libc::fread(nothing.as_mut_ptr().cast(), 5, 1, fi);
        libc::fread(std::ptr::addr_of_mut!(flag).cast(), 1, 1, fi);
        // flag assignment intentionally omitted, matching original behavior
        _z += 1;
    }

    libc::fclose(fi);
    0
}
