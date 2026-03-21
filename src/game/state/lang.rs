//! Language message table — `map_msg[]` and `lang_read`.

use std::sync::OnceLock;

/// Number of named message slots.
pub const MSG_MAX: usize = 30;

/// One message entry in the language table.
#[repr(C)]
pub struct MapMsgData {
    pub message: [i8; 256],
    pub len: i32,
}

static MAP_MSG: OnceLock<Box<[MapMsgData; MSG_MAX]>> = OnceLock::new();

/// Access the global language message table.
#[inline]
pub fn map_msg() -> &'static [MapMsgData; MSG_MAX] {
    MAP_MSG.get_or_init(|| {
        const ZERO: MapMsgData = MapMsgData {
            message: [0; 256],
            len: 0,
        };
        Box::new([ZERO; MSG_MAX])
    })
}

static LANG_KEY_MAP: &[(&str, usize)] = &[
    ("MAP_WHISPFAIL", 0),
    ("MAP_ERRGHOST", 1),
    ("MAP_ERRITMLEVEL", 2),
    ("MAP_ERRITMMIGHT", 3),
    ("MAP_ERRITMGRACE", 4),
    ("MAP_ERRITMWILL", 5),
    ("MAP_ERRITMSEX", 6),
    ("MAP_ERRITMFULL", 7),
    ("MAP_ERRITMMAX", 8),
    ("MAP_ERRITMPATH", 9),
    ("MAP_ERRITMMARK", 10),
    ("MAP_ERRITM2H", 11),
    ("MAP_ERRMOUNT", 12),
    ("MAP_EQHELM", 13),
    ("MAP_EQWEAP", 14),
    ("MAP_EQARMOR", 15),
    ("MAP_EQSHIELD", 16),
    ("MAP_EQLEFT", 17),
    ("MAP_EQRIGHT", 18),
    ("MAP_EQSUBLEFT", 19),
    ("MAP_EQSUBRIGHT", 20),
    ("MAP_EQFACEACC", 21),
    ("MAP_EQCROWN", 22),
    ("MAP_EQMANTLE", 23),
    ("MAP_EQNECKLACE", 24),
    ("MAP_EQBOOTS", 25),
    ("MAP_EQCOAT", 26),
    ("MAP_ERRVITA", 27),
    ("MAP_ERRMANA", 28),
    ("MAP_ERRSUMMON", 29),
];

/// Parse the language config file and populate `map_msg[]`.
///
/// # Safety
/// `cfg_file` must be a valid, non-null, null-terminated C string.
pub unsafe fn lang_read(cfg_file: *const i8) -> i32 {
    use std::io::BufRead as _;

    let path = std::ffi::CStr::from_ptr(cfg_file).to_string_lossy();

    let file = match std::fs::File::open(path.as_ref()) {
        Ok(f) => f,
        Err(_) => {
            tracing::error!("CFG_ERR: Language file ({path}) not found.");
            return 1;
        }
    };

    const ZERO: MapMsgData = MapMsgData {
        message: [0; 256],
        len: 0,
    };
    let mut msgs = Box::new([ZERO; MSG_MAX]);

    for line in std::io::BufReader::new(file).lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        if line.starts_with("//") {
            continue;
        }

        let Some(colon_pos) = line.find(": ") else {
            continue;
        };
        let key = &line[..colon_pos];
        let value = line[colon_pos + 2..].trim_end_matches(['\r', '\n']);

        let key_up = key.to_ascii_uppercase();
        let Some(&(_, idx)) = LANG_KEY_MAP.iter().find(|(k, _)| *k == key_up.as_str()) else {
            continue;
        };

        let bytes = value.as_bytes();
        let copy_len = bytes.len().min(255);
        let slot = &mut msgs[idx];
        slot.message = [0; 256];
        for (i, &b) in bytes[..copy_len].iter().enumerate() {
            slot.message[i] = b as i8;
        }
        slot.message[copy_len] = 0;
        slot.len = copy_len as i32;
    }

    let _ = MAP_MSG.set(msgs);
    tracing::info!("Language messages loaded.");
    0
}
