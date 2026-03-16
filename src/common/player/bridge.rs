use super::*;
use crate::servers::char::charstatus::MmoCharStatus;

/// Convert a C-style i8 array to a Rust String, stopping at the first null byte.
fn cstr_to_string(bytes: &[i8]) -> String {
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    bytes[..len].iter().map(|&b| b as u8 as char).collect()
}

impl PlayerData {
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
    use crate::servers::char::charstatus::MmoCharStatus;

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

    /// Allocate a zeroed MmoCharStatus on the heap (too large for stack).
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
}
