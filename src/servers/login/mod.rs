pub mod client;
pub mod db;
pub mod interserver;
pub mod packet;

use anyhow::Result;

/// The 11 localised error messages, indexed by LGN_* constants.
#[derive(Debug, Clone, Default)]
pub struct LoginMessages(pub [String; 11]);

// Message key indices — mirror C enum in login_server.h
pub const LGN_ERRSERVER: usize = 0;
pub const LGN_WRONGPASS: usize = 1;
pub const LGN_WRONGUSER: usize = 2;
pub const LGN_ERRDB:     usize = 3;
pub const LGN_USEREXIST: usize = 4;
pub const LGN_ERRPASS:   usize = 5;
pub const LGN_ERRUSER:   usize = 6;
pub const LGN_NEWCHAR:   usize = 7;
pub const LGN_CHGPASS:   usize = 8;
pub const LGN_DBLLOGIN:  usize = 9;
pub const LGN_BANNED:    usize = 10;

/// Parses a `key: value` lang file (same format as C `lang_read`).
/// Lines starting with `//` are comments. Unknown keys are silently ignored.
pub fn parse_lang_file(content: &str) -> Result<LoginMessages> {
    let mut msgs = LoginMessages::default();
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("//") || line.is_empty() {
            continue;
        }
        if let Some((key, val)) = line.split_once(':') {
            let val = val.trim().to_string();
            match key.trim().to_ascii_uppercase().as_str() {
                "LGN_ERRSERVER" => msgs.0[LGN_ERRSERVER] = val,
                "LGN_WRONGPASS" => msgs.0[LGN_WRONGPASS] = val,
                "LGN_WRONGUSER" => msgs.0[LGN_WRONGUSER] = val,
                "LGN_ERRDB"     => msgs.0[LGN_ERRDB]     = val,
                "LGN_USEREXIST" => msgs.0[LGN_USEREXIST] = val,
                "LGN_ERRPASS"   => msgs.0[LGN_ERRPASS]   = val,
                "LGN_ERRUSER"   => msgs.0[LGN_ERRUSER]   = val,
                "LGN_NEWCHAR"   => msgs.0[LGN_NEWCHAR]   = val,
                "LGN_CHGPASS"   => msgs.0[LGN_CHGPASS]   = val,
                "LGN_DBLLOGIN"  => msgs.0[LGN_DBLLOGIN]  = val,
                "LGN_BANNED"    => msgs.0[LGN_BANNED]     = val,
                _ => {}
            }
        }
    }
    Ok(msgs)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"
// Login server lang file
LGN_ERRSERVER: Server error
LGN_WRONGPASS: Wrong password
LGN_WRONGUSER: Wrong username
LGN_ERRDB: Database error
LGN_USEREXIST: User already exists
LGN_ERRPASS: Bad password format
LGN_ERRUSER: Bad username format
LGN_NEWCHAR: Character created
LGN_CHGPASS: Password changed
LGN_DBLLOGIN: Already logged in
LGN_BANNED: IP is banned
"#;

    #[test]
    fn test_parse_lang_file_all_keys() {
        let msgs = parse_lang_file(FIXTURE).unwrap();
        assert_eq!(msgs.0[LGN_ERRSERVER], "Server error");
        assert_eq!(msgs.0[LGN_WRONGPASS], "Wrong password");
        assert_eq!(msgs.0[LGN_BANNED],    "IP is banned");
    }

    #[test]
    fn test_parse_lang_file_ignores_comments() {
        let msgs = parse_lang_file("// comment\nLGN_BANNED: x").unwrap();
        assert_eq!(msgs.0[LGN_BANNED], "x");
        // non-banned messages stay empty
        assert_eq!(msgs.0[LGN_ERRDB], "");
    }
}
