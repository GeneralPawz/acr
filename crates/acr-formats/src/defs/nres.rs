//! `NRES.DAT`: resource types, weapon kinds and other engine-wide name lists.
//!
//! ```text
//! [RESTYPES]
//! Wood  0 500000 WOOD          name, index, limit, id
//! [WEAPONS]
//! #GENSHIELD  13
//! MECH      21 19   O          kind name, two icons/values, flag letters (`-` = none)
//! VISTREL   12 18   HRB
//! [UNITKINDS]
//! ...
//! ```
//!
//! The weapon kind's line index is what `.MD` `WEAPONKIND` and `PROTECTION` refer to; the
//! flag letters are read by 0x4eb2f0 into a bit set.

/// Weapon kind flags (`WeaponFlags`, u8 table 0x23bbb84).
pub mod weapon_flags {
    /// `H`: high trajectory; height differences extend the range.
    pub const HIGH: u8 = 1;
    /// `R`: 4 % chance of an instant kill.
    pub const RANDOM_KILL: u8 = 2;
    /// `O`: the formation attack bonus applies.
    pub const FORMATION: u8 = 4;
    /// `F`: sets the target on fire after several hits.
    pub const FIRE_SLOW: u8 = 8;
    /// `X`: sets the target on fire at once.
    pub const FIRE: u8 = 0x10;
    /// `B`: against buildings it only hurts the garrison.
    pub const GARRISON: u8 = 0x20;
    /// `I`.
    pub const I: u8 = 0x40;
}

/// One `[WEAPONS]` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeaponKind {
    pub name: String,
    pub flags: u8,
}

/// The `[WEAPONS]` section in file order (index = weapon kind).
pub fn weapon_kinds(data: &[u8]) -> Vec<WeaponKind> {
    let text = String::from_utf8_lossy(data);
    let mut in_section = false;
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_section = line.eq_ignore_ascii_case("[WEAPONS]");
            continue;
        }
        if !in_section || line.is_empty() || line.starts_with('#') || line.starts_with('/') {
            continue;
        }
        let words: Vec<&str> = line.split_whitespace().collect();
        let flags = words.get(3).map_or(0, |f| {
            f.bytes().fold(0, |acc, c| {
                acc | match c.to_ascii_uppercase() {
                    b'H' => weapon_flags::HIGH,
                    b'R' => weapon_flags::RANDOM_KILL,
                    b'O' => weapon_flags::FORMATION,
                    b'F' => weapon_flags::FIRE_SLOW,
                    b'X' => weapon_flags::FIRE,
                    b'B' => weapon_flags::GARRISON,
                    b'I' => weapon_flags::I,
                    _ => 0,
                }
            })
        });
        out.push(WeaponKind { name: words[0].to_ascii_uppercase(), flags });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_weapon_kinds() {
        let k = weapon_kinds(b"[RESTYPES]\r\nWood 0 1 WOOD\r\n[WEAPONS]\r\n#GENSHIELD 13\r\nMECH\t\t21 19\tO\r\nVISTREL\t12 18\tHRB\r\nIADROMOR 0 7 \t-\r\n[UNITKINDS]\r\nUNIT\r\n");
        assert_eq!(k.len(), 3);
        assert_eq!(k[0], WeaponKind { name: "MECH".into(), flags: weapon_flags::FORMATION });
        assert_eq!(k[1].flags, weapon_flags::HIGH | weapon_flags::RANDOM_KILL | weapon_flags::GARRISON);
        assert_eq!(k[2].flags, 0);
    }
}
