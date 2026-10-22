//! Static data of the in-game interface: icon names (`ICONLIST.TXT`) and the interface
//! sections of `NRES.DAT` (weapon icons, command icon slots, formation icons).

use std::collections::HashMap;

/// An icon of the command panel: its frame in `ICONS.GP`, its slot in the 12-column grid
/// (`x + 12·y`, `AddIcon` 0x495758) and the hint text id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotIcon {
    pub frame: usize,
    pub slot: usize,
    pub hint: String,
}

/// `NRES.DAT [ORDERICONS]`: the icon of a formation group (`LINE 8 0 Formation_Line
/// Assume_Rank_Formation`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderIcon {
    pub group: String,
    pub icon: SlotIcon,
}

/// `NRES.DAT [CONST]` switches of the first selected object (`STOPBUILD Production_OFF
/// Production_ON 11 0`, `KARTECH FireKartech_Cannon 7 1`; parsers 0x4ec38a..): one or two
/// icon frames (off, on) and the slot `x + 12·y`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchIcon {
    pub frames: Vec<usize>,
    pub slot: usize,
}

#[derive(Debug, Clone, Default)]
pub struct HudData {
    /// `ICONLIST.TXT`: icon name → frame of `ICONS.GP` (line index, the lookup 0x4f0516).
    icons: HashMap<String, usize>,
    /// `[WEAPONS]` in file order: (name, attack icon, protection icon) frames of `SMICONS.GP`.
    pub weapons: Vec<(String, usize, usize)>,
    /// Per weapon kind: flag `I` (0x40 of 0x23bbb84), whose attacks the unit numbers skip
    /// (0x6080fc).
    pub weapon_hidden: Vec<bool>,
    /// `#GENSHIELD`: the `SMICONS.GP` frame of the shield value.
    pub shield_icon: usize,
    /// `[CONST]` lines that place an icon (`GO&ATTACK 3 2 Go_With_Attack`).
    pub consts: HashMap<String, SlotIcon>,
    /// `STOPBUILD`, `KARTECH`, `PREKARTECH`, `FISHING`, `ZASEV` (see [`SwitchIcon`]).
    pub switches: HashMap<String, SwitchIcon>,
    pub orders: Vec<OrderIcon>,
    /// `UN?GROUP slot ungroup dogroup` (parser 0x4ec123): the slot of the grouping icon
    /// (`GroupIconPos` 0x66ae28, 12 without the line) and its two frames.
    pub group_slot: usize,
    pub ungroup_icon: Option<usize>,
    pub dogroup_icon: Option<usize>,
}

impl HudData {
    pub const ICON_LIST: &'static str = "ICONLIST.TXT";
    pub const NRES: &'static str = "NRES.DAT";

    pub fn parse(icon_list: &[u8], nres: &[u8]) -> Self {
        let icons = acr_formats::defs::name_list(icon_list)
            .into_iter()
            .enumerate()
            .map(|(i, n)| (n.to_ascii_uppercase(), i))
            .rev()
            .collect();
        let mut data = Self { icons, shield_icon: 13, group_slot: 12, ..Default::default() };
        // The engine's defaults (0x66ae84/0x66ae88): only `ZASEV` has an icon without a line.
        data.switches.insert("ZASEV".into(), SwitchIcon { frames: vec![0x98], slot: 1 });
        let text = String::from_utf8_lossy(nres);
        let mut section = "";
        for line in text.lines() {
            let line = line.split("//").next().unwrap_or("");
            let words: Vec<&str> = line.split_whitespace().collect();
            let Some(&first) = words.first() else { continue };
            if first.starts_with('[') {
                section = first;
                continue;
            }
            let int = |i: usize| words.get(i).and_then(|w| w.parse::<usize>().ok());
            match section {
                "[WEAPONS]" if first == "#GENSHIELD" => data.shield_icon = int(1).unwrap_or(13),
                "[WEAPONS]" => {
                    if let (Some(a), Some(p)) = (int(1), int(2)) {
                        data.weapons.push((first.to_owned(), a, p));
                        data.weapon_hidden.push(words.get(3).is_some_and(|f| f.contains('I')));
                    }
                }
                "[CONST]" if matches!(first, "STOPBUILD" | "KARTECH" | "PREKARTECH" | "FISHING" | "ZASEV") => {
                    let two = matches!(first, "STOPBUILD" | "FISHING");
                    let n = if two { 2 } else { 1 };
                    let frames: Option<Vec<usize>> = (1..=n).map(|i| words.get(i).and_then(|w| data.icon(w))).collect();
                    if let (Some(frames), Some(x), Some(y)) = (frames, int(n + 1), int(n + 2)) {
                        data.switches.insert(first.to_owned(), SwitchIcon { frames, slot: x + 12 * y });
                    }
                }
                "[CONST]" if first == "UN?GROUP" => {
                    if let Some(slot) = int(1) {
                        data.group_slot = slot;
                        data.ungroup_icon = words.get(2).and_then(|w| data.icon(w));
                        data.dogroup_icon = words.get(3).and_then(|w| data.icon(w));
                    }
                }
                "[CONST]" => {
                    // `NAME x y icon [hint]`: a placed icon.
                    if let (Some(x), Some(y), Some(icon)) = (int(1), int(2), words.get(3)) {
                        let frame = data.icon(icon);
                        let hint = words.get(4).unwrap_or(icon).to_string();
                        if let Some(frame) = frame {
                            data.consts.insert(first.to_owned(), SlotIcon { frame, slot: x + 12 * y, hint });
                        }
                    }
                }
                "[ORDERICONS]" => {
                    if let (Some(x), Some(y), Some(icon)) = (int(1), int(2), words.get(3))
                        && let Some(frame) = data.icon(icon)
                    {
                        let hint = words.get(4).unwrap_or(icon).to_string();
                        data.orders.push(OrderIcon {
                            group: first.to_owned(),
                            icon: SlotIcon { frame, slot: x + 12 * y, hint },
                        });
                    }
                }
                _ => {}
            }
        }
        data
    }

    /// The `ICONS.GP` frame of an icon name.
    pub fn icon(&self, name: &str) -> Option<usize> {
        self.icons.get(&name.to_ascii_uppercase()).copied()
    }

    /// The weapon kind index of a `[WEAPONS]` name.
    pub fn weapon(&self, name: &str) -> Option<usize> {
        self.weapons.iter().position(|(n, ..)| n.eq_ignore_ascii_case(name))
    }

    /// The icon of a formation group (`LINE`, `KARE`, ...).
    pub fn order(&self, group: &str) -> Option<&SlotIcon> {
        self.orders.iter().find(|o| o.group.eq_ignore_ascii_case(group)).map(|o| &o.icon)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_interface_sections() {
        let icons = b"Page1\r\nCancel\r\nFormation_Line\r\nGo_With_Attack\r\n";
        let nres = b"[WEAPONS]\r\n#GENSHIELD  13\r\nMECH\t\t21 19\tO\r\nSTRELA 1 14 HRB\r\n[CONST]\r\n\
GO&ATTACK 3 2 Go_With_Attack\r\nSTOP_ICON Cancel\r\n[ORDERICONS]\r\nLINE   8 0 Formation_Line Assume_Rank_Formation\r\n[END]\r\n";
        let d = HudData::parse(icons, nres);
        assert_eq!(d.icon("cancel"), Some(1));
        assert_eq!(d.weapons, [("MECH".to_owned(), 21, 19), ("STRELA".to_owned(), 1, 14)]);
        assert_eq!(d.weapon("strela"), Some(1));
        assert_eq!(d.consts["GO&ATTACK"], SlotIcon { frame: 3, slot: 27, hint: "Go_With_Attack".into() });
        assert!(!d.consts.contains_key("STOP_ICON"));
        assert_eq!(d.order("line").unwrap().slot, 8);
        assert_eq!(d.order("LINE").unwrap().hint, "Assume_Rank_Formation");
    }

    #[test]
    fn parses_switches() {
        let icons = b"Production_OFF\r\nProduction_ON\r\nFireKartech_Cannon\r\n";
        let nres =
            b"[CONST]\r\nSTOPBUILD  Production_OFF Production_ON 11 0\r\nKARTECH FireKartech_Cannon 7 1\r\n[END]\r\n";
        let d = HudData::parse(icons, nres);
        assert_eq!(d.switches["STOPBUILD"], SwitchIcon { frames: vec![0, 1], slot: 11 });
        assert_eq!(d.switches["KARTECH"], SwitchIcon { frames: vec![2], slot: 19 });
        assert_eq!(d.switches["ZASEV"], SwitchIcon { frames: vec![152], slot: 1 }, "the engine's default");
        assert!(!d.switches.contains_key("FISHING"));
        assert_eq!((d.group_slot, d.ungroup_icon, d.dogroup_icon), (12, None, None), "the engine's default");
    }

    #[test]
    fn parses_grouping_icons() {
        let icons = b"Make_Group_Formation_OFF\r\nMake_Group_Formation_ON\r\n";
        let nres = b"[CONST]\r\nUN?GROUP 14 Make_Group_Formation_OFF Make_Group_Formation_ON\r\n[END]\r\n";
        let d = HudData::parse(icons, nres);
        assert_eq!((d.group_slot, d.ungroup_icon, d.dogroup_icon), (14, Some(0), Some(1)));
    }
}
