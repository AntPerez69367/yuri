use sqlx::MySqlPool;
use anyhow::Result;
use md5::{Md5, Digest};

/// Compute MD5 of `input` and return it as a lowercase hex string.
fn md5_hex(input: &str) -> String {
    hex::encode(Md5::new().chain_update(input).finalize())
}

/// Verify password: checks MD5("lowercase_name password") or MD5(password).
/// Returns true if either form matches `stored_hash` from DB.
pub fn ispass(name: &str, pass: &str, stored_hash: &str) -> bool {
    let form1 = md5_hex(&format!("{} {}", name.to_lowercase(), pass));
    let form2 = md5_hex(pass);
    stored_hash == form1 || stored_hash == form2
}

/// Returns true if master password matches and hasn't expired.
pub fn ismastpass(pass: &str, mast_md5: &str, expire: i64) -> bool {
    md5_hex(pass) == mast_md5 && chrono::Utc::now().timestamp() <= expire
}

/// Load character as raw binary blob (placeholder — full impl in Task 7).
pub async fn load_char_bytes(_pool: &MySqlPool, _char_id: u32, _login_name: &str) -> Result<Vec<u8>> {
    Ok(vec![0u8; std::mem::size_of::<u32>() * 20])
}

/// Save character from raw binary blob (placeholder — full impl in Task 7).
pub async fn save_char_bytes(_pool: &MySqlPool, _raw: &[u8]) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ispass_form1() {
        let hash = md5_hex("alice password");
        assert!(ispass("Alice", "password", &hash));
    }

    #[test]
    fn test_ispass_form2() {
        let hash = md5_hex("mypass");
        assert!(ispass("bob", "mypass", &hash));
    }

    #[test]
    fn test_ispass_wrong() {
        let hash = md5_hex("correct");
        assert!(!ispass("bob", "wrong", &hash));
    }
}
