use super::*;
use crate::servers::char::charstatus::MmoCharStatus;

/// Convert a C-style i8 array to a Rust String, stopping at the first null byte.
fn cstr_to_string(bytes: &[i8]) -> String {
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    bytes[..len].iter().map(|&b| b as u8 as char).collect()
}

/// Convert a Rust String to a C-style i8 array. Null-terminated, truncated to fit.
/// Inverse of `cstr_to_string`.
fn string_to_i8_array<const N: usize>(src: &str, dst: &mut [i8; N]) {
    let bytes = src.as_bytes();
    let n = bytes.len().min(N - 1);
    for i in 0..n {
        dst[i] = bytes[i] as i8;
    }
    dst[n] = 0;
}

impl PlayerData {
    /// Convert to the legacy MmoCharStatus. Used for wire compatibility (0x3803).
    pub fn to_mmo_char_status(&self) -> Box<MmoCharStatus> {
        use crate::servers::char::charstatus::*;

        let mut s = alloc_zeroed_charstatus();

        // ── Identity ──
        s.id = self.identity.id;
        string_to_i8_array(&self.identity.name, &mut s.name);
        string_to_i8_array(&self.identity.pass, &mut s.pass);
        string_to_i8_array(&self.identity.f1name, &mut s.f1name);
        string_to_i8_array(&self.identity.title, &mut s.title);
        string_to_i8_array(&self.identity.ipaddress, &mut s.ipaddress);
        s.gm_level = self.identity.gm_level;
        s.sex = self.identity.sex;
        s.map_server = self.identity.map_server;
        s.dest_pos = self.identity.dest_pos;
        s.last_pos = self.identity.last_pos;

        // ── Combat ──
        s.hp = self.combat.hp;
        s.basehp = self.combat.max_hp;
        s.mp = self.combat.mp;
        s.basemp = self.combat.max_mp;
        s.might = self.combat.might;
        s.will = self.combat.will;
        s.grace = self.combat.grace;
        s.basemight = self.combat.base_might;
        s.basewill = self.combat.base_will;
        s.basegrace = self.combat.base_grace;
        s.basearmor = self.combat.base_armor;
        s.state = self.combat.state;
        s.side = self.combat.side;

        // ── Progression ──
        s.level = self.progression.level;
        s.class = self.progression.class;
        s.tier = self.progression.tier;
        s.mark = self.progression.mark;
        s.totem = self.progression.totem;
        s.country = self.progression.country;
        s.magic_number = self.progression.magic_number;
        s.exp = self.progression.exp;
        s.tnl = self.progression.tnl;
        s.nextlevelxp = self.progression.next_level_xp;
        s.maxtnl = self.progression.max_tnl;
        s.realtnl = self.progression.real_tnl;
        s.class_rank = self.progression.class_rank;
        s.clan_rank = self.progression.clan_rank;
        s.percentage = self.progression.percentage;
        s.int_percentage = self.progression.int_percentage;
        s.expsold_magic = self.progression.expsold_magic;
        s.expsold_health = self.progression.expsold_health;
        s.expsold_stats = self.progression.expsold_stats;

        // ── Spells ──
        let n = self.spells.skills.len().min(MAX_SPELLS);
        s.skill[..n].copy_from_slice(&self.spells.skills[..n]);
        let n = self.spells.dura_aether.len().min(MAX_MAGIC_TIMERS);
        s.dura_aether[..n].copy_from_slice(&self.spells.dura_aether[..n]);

        // ── Inventory ──
        let n = self.inventory.equip.len().min(MAX_EQUIP);
        s.equip[..n].copy_from_slice(&self.inventory.equip[..n]);
        let n = self.inventory.inventory.len().min(MAX_INVENTORY);
        s.inventory[..n].copy_from_slice(&self.inventory.inventory[..n]);
        let n = self.inventory.banks.len().min(MAX_BANK_SLOTS);
        s.banks[..n].copy_from_slice(&self.inventory.banks[..n]);
        s.money = self.inventory.money;
        s.bankmoney = self.inventory.bank_money;
        s.maxinv = self.inventory.max_inv;
        s.maxslots = self.inventory.max_slots;

        // ── Appearance ──
        s.face = self.appearance.face;
        s.hair = self.appearance.hair;
        s.face_color = self.appearance.face_color;
        s.hair_color = self.appearance.hair_color;
        s.armor_color = self.appearance.armor_color;
        s.skin_color = self.appearance.skin_color;
        s.disguise = self.appearance.disguise;
        s.disguise_color = self.appearance.disguise_color;
        s.setting_flags = self.appearance.setting_flags;
        s.heroes = self.appearance.heroes;
        s.mini_map_toggle = self.appearance.mini_map_toggle;
        s.profile_vitastats = self.appearance.profile_vitastats;
        s.profile_equiplist = self.appearance.profile_equiplist;
        s.profile_legends = self.appearance.profile_legends;
        s.profile_spells = self.appearance.profile_spells;
        s.profile_inventory = self.appearance.profile_inventory;
        s.profile_bankitems = self.appearance.profile_bankitems;

        // ── Social ──
        s.partner = self.social.partner;
        s.clan = self.social.clan;
        string_to_i8_array(&self.social.clan_title, &mut s.clan_title);
        s.clan_chat = self.social.clan_chat;
        s.pk = self.social.pk;
        s.killedby = self.social.killed_by;
        s.killspk = self.social.kills_pk;
        s.pkduration = self.social.pk_duration;
        s.karma = self.social.karma;
        s.alignment = self.social.alignment;
        s.novice_chat = self.social.novice_chat;
        s.subpath_chat = self.social.subpath_chat;
        s.mute = self.social.mute;
        s.tutor = self.social.tutor;
        string_to_i8_array(&self.social.afk_message, &mut s.afkmessage);

        // ── Registries → fixed arrays ──
        write_registries_to_mmo(&self.registries, &mut s);

        // ── Legends ──
        let n = self.legends.legends.len().min(MAX_LEGENDS);
        s.legends[..n].copy_from_slice(&self.legends.legends[..n]);

        s
    }

    /// Convert from the legacy MmoCharStatus. Used during migration and for testing.
    pub fn from_mmo_char_status(old: &MmoCharStatus) -> Self {
        PlayerData {
            identity: PlayerIdentity {
                id: old.id,
                name: cstr_to_string(&old.name),
                pass: cstr_to_string(&old.pass),
                f1name: cstr_to_string(&old.f1name),
                title: cstr_to_string(&old.title),
                ipaddress: cstr_to_string(&old.ipaddress),
                gm_level: old.gm_level,
                sex: old.sex,
                map_server: old.map_server,
                dest_pos: old.dest_pos,
                last_pos: old.last_pos,
            },
            combat: PlayerCombat {
                hp: old.hp,
                max_hp: old.basehp,
                mp: old.mp,
                max_mp: old.basemp,
                might: old.might,
                will: old.will,
                grace: old.grace,
                base_might: old.basemight,
                base_will: old.basewill,
                base_grace: old.basegrace,
                base_armor: old.basearmor,
                state: old.state,
                side: old.side,
            },
            progression: PlayerProgression {
                level: old.level,
                class: old.class,
                tier: old.tier,
                mark: old.mark,
                totem: old.totem,
                country: old.country,
                magic_number: old.magic_number,
                exp: old.exp,
                tnl: old.tnl,
                next_level_xp: old.nextlevelxp,
                max_tnl: old.maxtnl,
                real_tnl: old.realtnl,
                class_rank: old.class_rank,
                clan_rank: old.clan_rank,
                percentage: old.percentage,
                int_percentage: old.int_percentage,
                expsold_magic: old.expsold_magic,
                expsold_health: old.expsold_health,
                expsold_stats: old.expsold_stats,
            },
            spells: PlayerSpells {
                skills: old.skill.to_vec(),
                dura_aether: old.dura_aether.to_vec(),
            },
            inventory: PlayerInventory {
                equip: old.equip.to_vec(),
                inventory: old.inventory.to_vec(),
                banks: old.banks.to_vec(),
                money: old.money,
                bank_money: old.bankmoney,
                max_inv: old.maxinv,
                max_slots: old.maxslots,
            },
            appearance: PlayerAppearance {
                face: old.face,
                hair: old.hair,
                face_color: old.face_color,
                hair_color: old.hair_color,
                armor_color: old.armor_color,
                skin_color: old.skin_color,
                disguise: old.disguise,
                disguise_color: old.disguise_color,
                setting_flags: old.setting_flags,
                heroes: old.heroes,
                mini_map_toggle: old.mini_map_toggle,
                profile_vitastats: old.profile_vitastats,
                profile_equiplist: old.profile_equiplist,
                profile_legends: old.profile_legends,
                profile_spells: old.profile_spells,
                profile_inventory: old.profile_inventory,
                profile_bankitems: old.profile_bankitems,
            },
            social: PlayerSocial {
                partner: old.partner,
                clan: old.clan,
                clan_title: cstr_to_string(&old.clan_title),
                clan_chat: old.clan_chat,
                pk: old.pk,
                killed_by: old.killedby,
                kills_pk: old.killspk,
                pk_duration: old.pkduration,
                karma: old.karma,
                alignment: old.alignment,
                novice_chat: old.novice_chat,
                subpath_chat: old.subpath_chat,
                mute: old.mute,
                tutor: old.tutor,
                afk_message: cstr_to_string(&old.afkmessage),
            },
            registries: convert_registries(old),
            legends: PlayerLegends {
                legends: old.legends.to_vec(),
            },
        }
    }
}

/// Allocate a zeroed MmoCharStatus on the heap (3MB — too large for stack).
fn alloc_zeroed_charstatus() -> Box<MmoCharStatus> {
    unsafe {
        let layout = std::alloc::Layout::new::<MmoCharStatus>();
        let ptr = std::alloc::alloc_zeroed(layout) as *mut MmoCharStatus;
        if ptr.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        Box::from_raw(ptr)
    }
}

/// Write HashMap-based registries back into MmoCharStatus fixed arrays.
fn write_registries_to_mmo(regs: &PlayerRegistries, s: &mut MmoCharStatus) {
    use crate::servers::char::charstatus::MAX_GLOBALREG;
    use crate::servers::char::charstatus::MAX_GLOBALQUESTREG;
    use crate::servers::char::charstatus::MAX_KILLREG;

    // Global integer registries
    for (i, (key, &val)) in regs.global_reg.iter().enumerate().take(MAX_GLOBALREG) {
        string_to_i8_array(key, &mut s.global_reg[i].str);
        s.global_reg[i].val = val;
    }
    s.global_reg_num = regs.global_reg.len().min(MAX_GLOBALREG) as i32;

    // Global string registries
    for (i, (key, val)) in regs.global_regstring.iter().enumerate().take(MAX_GLOBALREG) {
        string_to_i8_array(key, &mut s.global_regstring[i].str);
        string_to_i8_array(val, &mut s.global_regstring[i].val);
    }
    s.global_regstring_num = regs.global_regstring.len().min(MAX_GLOBALREG) as i32;

    // Account registries
    for (i, (key, &val)) in regs.acct_reg.iter().enumerate().take(MAX_GLOBALREG) {
        string_to_i8_array(key, &mut s.acctreg[i].str);
        s.acctreg[i].val = val;
    }

    // NPC integer registries
    for (i, (key, &val)) in regs.npc_int_reg.iter().enumerate().take(MAX_GLOBALREG) {
        string_to_i8_array(key, &mut s.npcintreg[i].str);
        s.npcintreg[i].val = val;
    }

    // Quest registries
    for (i, (key, &val)) in regs.quest_reg.iter().enumerate().take(MAX_GLOBALQUESTREG) {
        string_to_i8_array(key, &mut s.questreg[i].str);
        s.questreg[i].val = val;
    }

    // Kill registries
    for (i, (&mob_id, &amount)) in regs.kill_reg.iter().enumerate().take(MAX_KILLREG) {
        s.killreg[i].mob_id = mob_id;
        s.killreg[i].amount = amount;
    }
}

/// Convert fixed-array registries to HashMaps.
/// Only non-empty entries (key string not empty) are included.
fn convert_registries(old: &MmoCharStatus) -> PlayerRegistries {
    let mut regs = PlayerRegistries::default();

    // Global integer registries
    for i in 0..old.global_reg_num.max(0) as usize {
        if i >= old.global_reg.len() { break; }
        let key = cstr_to_string(&old.global_reg[i].str);
        if !key.is_empty() {
            regs.global_reg.insert(key, old.global_reg[i].val);
        }
    }

    // Global string registries
    for i in 0..old.global_regstring_num.max(0) as usize {
        if i >= old.global_regstring.len() { break; }
        let key = cstr_to_string(&old.global_regstring[i].str);
        if !key.is_empty() {
            let val = cstr_to_string(&old.global_regstring[i].val);
            regs.global_regstring.insert(key, val);
        }
    }

    // Account registries (scan all — no count field)
    for reg in old.acctreg.iter() {
        let key = cstr_to_string(&reg.str);
        if !key.is_empty() {
            regs.acct_reg.insert(key, reg.val);
        }
    }

    // NPC integer registries
    for reg in old.npcintreg.iter() {
        let key = cstr_to_string(&reg.str);
        if !key.is_empty() {
            regs.npc_int_reg.insert(key, reg.val);
        }
    }

    // Quest registries
    for reg in old.questreg.iter() {
        let key = cstr_to_string(&reg.str);
        if !key.is_empty() {
            regs.quest_reg.insert(key, reg.val);
        }
    }

    // Kill registries
    for kill in old.killreg.iter() {
        if kill.mob_id != 0 {
            regs.kill_reg.insert(kill.mob_id, kill.amount);
        }
    }

    regs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::types::Point;

    #[test]
    fn roundtrip_preserves_identity() {
        let mut old = alloc_zeroed_charstatus();
        old.id = 12345;
        old.gm_level = 5;
        old.sex = 1;
        old.map_server = 3;
        let name = b"TestPlayer\0";
        for (i, &b) in name.iter().enumerate() {
            old.name[i] = b as i8;
        }

        let new = PlayerData::from_mmo_char_status(&old);
        assert_eq!(new.identity.id, 12345);
        assert_eq!(new.identity.name, "TestPlayer");
        assert_eq!(new.identity.gm_level, 5);
        assert_eq!(new.identity.sex, 1);
        assert_eq!(new.identity.map_server, 3);
    }

    #[test]
    fn roundtrip_preserves_combat() {
        let mut old = alloc_zeroed_charstatus();
        old.hp = 500;
        old.basehp = 600;
        old.mp = 200;
        old.basemp = 300;
        old.might = 50;
        old.basemight = 45;
        old.state = -1;
        old.side = 2;

        let new = PlayerData::from_mmo_char_status(&old);
        assert_eq!(new.combat.hp, 500);
        assert_eq!(new.combat.max_hp, 600);
        assert_eq!(new.combat.mp, 200);
        assert_eq!(new.combat.max_mp, 300);
        assert_eq!(new.combat.might, 50);
        assert_eq!(new.combat.base_might, 45);
        assert_eq!(new.combat.state, -1);
        assert_eq!(new.combat.side, 2);
    }

    #[test]
    fn roundtrip_preserves_progression() {
        let mut old = alloc_zeroed_charstatus();
        old.level = 99;
        old.class = 3;
        old.tier = 2;
        old.exp = 1_000_000;

        let new = PlayerData::from_mmo_char_status(&old);
        assert_eq!(new.progression.level, 99);
        assert_eq!(new.progression.class, 3);
        assert_eq!(new.progression.tier, 2);
        assert_eq!(new.progression.exp, 1_000_000);
    }

    #[test]
    fn roundtrip_preserves_registries_as_hashmaps() {
        let mut old = alloc_zeroed_charstatus();
        let key = b"test_var\0";
        for (i, &b) in key.iter().enumerate() {
            old.global_reg[0].str[i] = b as i8;
        }
        old.global_reg[0].val = 42;
        old.global_reg_num = 1;

        let new = PlayerData::from_mmo_char_status(&old);
        assert_eq!(new.registries.get_reg("test_var"), Some(42));
    }

    #[test]
    fn roundtrip_preserves_inventory_slots() {
        let mut old = alloc_zeroed_charstatus();
        old.inventory[0].id = 100;
        old.inventory[0].amount = 5;
        old.equip[3].id = 200;
        old.money = 50000;

        let new = PlayerData::from_mmo_char_status(&old);
        assert_eq!(new.inventory.inventory[0].id, 100);
        assert_eq!(new.inventory.inventory[0].amount, 5);
        assert_eq!(new.inventory.equip[3].id, 200);
        assert_eq!(new.inventory.money, 50000);
    }

    #[test]
    fn roundtrip_preserves_spells() {
        let mut old = alloc_zeroed_charstatus();
        old.skill[0] = 42;
        old.skill[5] = 99;
        old.dura_aether[0].id = 7;
        old.dura_aether[0].duration = 300;

        let new = PlayerData::from_mmo_char_status(&old);
        assert_eq!(new.spells.skills[0], 42);
        assert_eq!(new.spells.skills[5], 99);
        assert_eq!(new.spells.dura_aether[0].id, 7);
        assert_eq!(new.spells.dura_aether[0].duration, 300);
    }

    #[test]
    fn roundtrip_preserves_appearance() {
        let mut old = alloc_zeroed_charstatus();
        old.face = 3;
        old.hair = 5;
        old.hair_color = 12;
        old.setting_flags = 0b1010;

        let new = PlayerData::from_mmo_char_status(&old);
        assert_eq!(new.appearance.face, 3);
        assert_eq!(new.appearance.hair, 5);
        assert_eq!(new.appearance.hair_color, 12);
        assert_eq!(new.appearance.setting_flags, 0b1010);
    }

    #[test]
    fn roundtrip_preserves_social() {
        let mut old = alloc_zeroed_charstatus();
        old.partner = 999;
        old.clan = 42;
        old.pk = 1;
        old.karma = 3.14;
        let title = b"Knight\0";
        for (i, &b) in title.iter().enumerate() {
            old.clan_title[i] = b as i8;
        }

        let new = PlayerData::from_mmo_char_status(&old);
        assert_eq!(new.social.partner, 999);
        assert_eq!(new.social.clan, 42);
        assert_eq!(new.social.pk, 1);
        assert!((new.social.karma - 3.14).abs() < 0.001);
        assert_eq!(new.social.clan_title, "Knight");
    }

    #[test]
    fn roundtrip_preserves_legends() {
        let mut old = alloc_zeroed_charstatus();
        old.legends[0].icon = 5;
        old.legends[0].color = 3;

        let new = PlayerData::from_mmo_char_status(&old);
        assert_eq!(new.legends.legends[0].icon, 5);
        assert_eq!(new.legends.legends[0].color, 3);
        assert_eq!(new.legends.legends.len(), 1000);
    }

    #[test]
    fn string_to_i8_roundtrip() {
        let original = "TestPlayer";
        let mut buf = [0i8; 16];
        string_to_i8_array(original, &mut buf);
        let back = cstr_to_string(&buf);
        assert_eq!(back, original);
    }

    #[test]
    fn string_to_i8_truncates() {
        let long = "ThisStringIsTooLongForTheBuffer";
        let mut buf = [0i8; 10];
        string_to_i8_array(long, &mut buf);
        let back = cstr_to_string(&buf);
        assert_eq!(back, "ThisStrin"); // 9 chars + null
    }

    #[test]
    fn string_to_i8_empty() {
        let mut buf = [0x7Fi8; 16]; // fill with non-zero
        string_to_i8_array("", &mut buf);
        assert_eq!(buf[0], 0); // null terminator at index 0
    }

    #[test]
    fn reverse_bridge_roundtrip_identity() {
        let mut pd = PlayerData::default();
        pd.identity.id = 12345;
        pd.identity.name = "TestPlayer".to_string();
        pd.identity.gm_level = 5;
        pd.identity.sex = 1;
        pd.identity.map_server = 3;
        pd.identity.dest_pos = Point::new(100, 50, 25);

        let mmo = pd.to_mmo_char_status();
        let back = PlayerData::from_mmo_char_status(&mmo);
        assert_eq!(back.identity.id, 12345);
        assert_eq!(back.identity.name, "TestPlayer");
        assert_eq!(back.identity.gm_level, 5);
        assert_eq!(back.identity.sex, 1);
        assert_eq!(back.identity.dest_pos, Point::new(100, 50, 25));
    }

    #[test]
    fn reverse_bridge_roundtrip_combat() {
        let mut pd = PlayerData::default();
        pd.combat.hp = 500;
        pd.combat.max_hp = 600;
        pd.combat.state = -1;
        pd.combat.base_armor = -10;

        let mmo = pd.to_mmo_char_status();
        let back = PlayerData::from_mmo_char_status(&mmo);
        assert_eq!(back.combat.hp, 500);
        assert_eq!(back.combat.max_hp, 600);
        assert_eq!(back.combat.state, -1);
        assert_eq!(back.combat.base_armor, -10);
    }

    #[test]
    fn reverse_bridge_roundtrip_registries() {
        let mut pd = PlayerData::default();
        pd.registries.set_reg("test_var", 42);
        pd.registries.set_reg("another", 99);
        pd.registries.set_reg_str("str_key", "hello");

        let mmo = pd.to_mmo_char_status();
        let back = PlayerData::from_mmo_char_status(&mmo);
        assert_eq!(back.registries.get_reg("test_var"), Some(42));
        assert_eq!(back.registries.get_reg("another"), Some(99));
        assert_eq!(back.registries.get_reg_str("str_key"), Some("hello"));
    }

    #[test]
    fn reverse_bridge_roundtrip_inventory() {
        let mut pd = PlayerData::default();
        pd.inventory.inventory[0].id = 100;
        pd.inventory.inventory[0].amount = 5;
        pd.inventory.equip[3].id = 200;
        pd.inventory.money = 50000;

        let mmo = pd.to_mmo_char_status();
        let back = PlayerData::from_mmo_char_status(&mmo);
        assert_eq!(back.inventory.inventory[0].id, 100);
        assert_eq!(back.inventory.inventory[0].amount, 5);
        assert_eq!(back.inventory.equip[3].id, 200);
        assert_eq!(back.inventory.money, 50000);
    }

    #[test]
    fn reverse_bridge_roundtrip_kills() {
        let mut pd = PlayerData::default();
        pd.registries.kill_reg.insert(1001, 5);
        pd.registries.kill_reg.insert(2002, 10);

        let mmo = pd.to_mmo_char_status();
        let back = PlayerData::from_mmo_char_status(&mmo);
        assert_eq!(back.registries.get_kill_count(1001), 5);
        assert_eq!(back.registries.get_kill_count(2002), 10);
    }
}
