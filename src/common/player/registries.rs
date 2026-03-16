use std::collections::HashMap;
use serde::{Serialize, Deserialize};

/// Script variable registries. HashMap-based (replaces linear search over fixed arrays).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlayerRegistries {
    pub global_reg: HashMap<String, i32>,
    pub global_regstring: HashMap<String, String>,
    pub acct_reg: HashMap<String, i32>,
    pub npc_int_reg: HashMap<String, i32>,
    pub quest_reg: HashMap<String, i32>,
    pub kill_reg: HashMap<u32, u32>,
}

impl PlayerRegistries {
    // ── Global integer registry ──

    pub fn get_reg(&self, key: &str) -> Option<i32> {
        self.global_reg.get(key).copied()
    }

    pub fn set_reg(&mut self, key: &str, val: i32) {
        self.global_reg.insert(key.to_owned(), val);
    }

    // ── Global string registry ──

    pub fn get_reg_str(&self, key: &str) -> Option<&str> {
        self.global_regstring.get(key).map(|s| s.as_str())
    }

    pub fn set_reg_str(&mut self, key: &str, val: &str) {
        self.global_regstring.insert(key.to_owned(), val.to_owned());
    }

    // ── NPC integer registry ──

    pub fn get_npc_reg(&self, key: &str) -> Option<i32> {
        self.npc_int_reg.get(key).copied()
    }

    pub fn set_npc_reg(&mut self, key: &str, val: i32) {
        self.npc_int_reg.insert(key.to_owned(), val);
    }

    // ── Account registry ──

    pub fn get_acct_reg(&self, key: &str) -> Option<i32> {
        self.acct_reg.get(key).copied()
    }

    pub fn set_acct_reg(&mut self, key: &str, val: i32) {
        self.acct_reg.insert(key.to_owned(), val);
    }

    // ── Quest registry ──

    pub fn get_quest_reg(&self, key: &str) -> Option<i32> {
        self.quest_reg.get(key).copied()
    }

    pub fn set_quest_reg(&mut self, key: &str, val: i32) {
        self.quest_reg.insert(key.to_owned(), val);
    }

    // ── Kill registry ──

    pub fn get_kill_count(&self, mob_id: u32) -> u32 {
        self.kill_reg.get(&mob_id).copied().unwrap_or(0)
    }

    pub fn add_kill(&mut self, mob_id: u32) {
        *self.kill_reg.entry(mob_id).or_insert(0) += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_set_reg() {
        let mut r = PlayerRegistries::default();
        assert_eq!(r.get_reg("test_var"), None);
        r.set_reg("test_var", 42);
        assert_eq!(r.get_reg("test_var"), Some(42));
    }

    #[test]
    fn set_reg_overwrites() {
        let mut r = PlayerRegistries::default();
        r.set_reg("x", 1);
        r.set_reg("x", 2);
        assert_eq!(r.get_reg("x"), Some(2));
    }

    #[test]
    fn get_set_reg_str() {
        let mut r = PlayerRegistries::default();
        assert_eq!(r.get_reg_str("key"), None);
        r.set_reg_str("key", "value");
        assert_eq!(r.get_reg_str("key"), Some("value"));
    }

    #[test]
    fn separate_namespaces() {
        let mut r = PlayerRegistries::default();
        r.set_reg("var", 10);
        r.set_npc_reg("var", 20);
        assert_eq!(r.get_reg("var"), Some(10));
        assert_eq!(r.get_npc_reg("var"), Some(20));
    }

    #[test]
    fn kill_reg_increment() {
        let mut r = PlayerRegistries::default();
        assert_eq!(r.get_kill_count(1001), 0);
        r.add_kill(1001);
        r.add_kill(1001);
        assert_eq!(r.get_kill_count(1001), 2);
    }

    #[test]
    fn default_is_empty() {
        let r = PlayerRegistries::default();
        assert!(r.global_reg.is_empty());
        assert!(r.global_regstring.is_empty());
        assert!(r.acct_reg.is_empty());
        assert!(r.npc_int_reg.is_empty());
        assert!(r.quest_reg.is_empty());
        assert!(r.kill_reg.is_empty());
    }
}
