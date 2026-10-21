//! 256-color palettes (`*.PAL`): 768 bytes of 8-bit RGB triples.

#[derive(Clone, PartialEq, Eq)]
pub struct Palette(pub [[u8; 3]; 256]);

impl Palette {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let mut colors = [[0u8; 3]; 256];
        let (triples, _) = bytes.get(..768)?.as_chunks::<3>();
        colors.copy_from_slice(triples);
        Some(Self(colors))
    }

    pub fn rgb(&self, index: u8) -> [u8; 3] {
        self.0[usize::from(index)]
    }
}

impl std::fmt::Debug for Palette {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Palette({:?}..)", &self.0[..4])
    }
}

/// The palettes sprites are drawn with: set 0 is the game itself, sets 1 and 2 the menus.
pub const SPRITE_PALETTES: [&str; 3] = ["0\\AGEW_1.PAL", "1\\AGEW_1.PAL", "2\\AGEW_1.PAL"];

/// Sprite packs drawn with palette set 1 (normalized paths, see [`crate::normalize_path`]).
const SET_1: &[&str] = &[
    "CURSOR_01.GP",
    "INTERF2/BUTTONS.GP",
    "INTERF2/CHECKER.GP",
    "INTERF2/COMBO.GP",
    "INTERF2/CREATEGAMEROOM.GP",
    "INTERF2/FLAGS.GP",
    "INTERF2/GAMEBUTTONS.GP",
    "INTERF2/GAMEMAP.GP",
    "INTERF2/LISTBOX.GP",
    "INTERF2/LOADGAME.GP",
    "INTERF2/MAINMENU.GP",
    "INTERF2/PLAYERLIST.GP",
    "INTERF2/SCROLL.GP",
    "INTERF2/SINGMISS.GP",
    "INTERF2/TEXTBOX1.GP",
    "INTERF2/TEXTBOX2.GP",
    "INTERF2/TEXTBOX3.GP",
    "INTERF2/VSCROLL.GP",
    "INTERF2/_LOADING.GP",
    "INTERFACE/_LOADING.GP",
    "INTERNET/PIX/GERBAC.GP",
    "INTERNET/PIX/IS_BTNS.GP",
    "INTERNET/PIX/I_BOR1.GP",
    "INTERNET/PIX/I_BOR2.GP",
    "INTERNET/PIX/LONGBOX.GP",
    "INTERNET/PIX/MAPPIX.GP",
    "INTERNET/PIX/_SLIDER.GP",
];

/// Sprite packs drawn with palette set 2.
const SET_2: &[&str] = &[
    "CURSOR_02.GP",
    "INTERF2/OPTIONS.GP",
    "INTERFACE/ANM_CANCEL_OFF.GP",
    "INTERFACE/ANM_CANCEL_ON.GP",
    "INTERFACE/ANM_ENC_OFF.GP",
    "INTERFACE/ANM_ENC_ON.GP",
    "INTERFACE/ANM_START_1_OFF.GP",
    "INTERFACE/ANM_START_1_ON.GP",
    "INTERFACE/ANM_START_OFF.GP",
    "INTERFACE/ANM_START_ON.GP",
    "INTERFACE/AN_START_DISABLE.GP",
    "INTERFACE/BOR2.GP",
    "INTERFACE/CAMPAIGNS.GP",
    "INTERFACE/COMPCOMBO.GP",
    "INTERFACE/CREATEIGAME.GP",
    "INTERFACE/GAME_SELECT.GP",
    "INTERFACE/IBUTTONS.GP",
    "INTERFACE/IGAME.GP",
    "INTERFACE/ISYMB2.GP",
    "INTERFACE/ISYMB3.GP",
    "INTERFACE/LOADING1.GP",
    "INTERFACE/LOAD_GAME.GP",
    "INTERFACE/LOGIN.GP",
    "INTERFACE/LOGWIZARD.GP",
    "INTERFACE/LONGBOX.GP",
    "INTERFACE/MAIN_MENU.GP",
    "INTERFACE/MULTI_OPT.GP",
    "INTERFACE/ONE_CAMPAIGN.GP",
    "INTERFACE/OPTIONS.GP",
    "INTERFACE/PLAYERFORM.GP",
    "INTERFACE/PLAYER_NAME.GP",
    "INTERFACE/RANDOM_MAP.GP",
    "INTERFACE/SIGNS_2.GP",
    "INTERFACE/SINGLE_MISSION.GP",
    "INTERFACE/TOP100.GP",
    "INTERFACE/TYPE_CONNECTION.GP",
    "INTERFACE/WARS.GP",
    "INTERFACE/_SLIDER.GP",
];

/// The palette a sprite pack was drawn for. The original switches palettes per screen;
/// the lists above come from ranking the three sets with `acr gp-palette` (see
/// `docs/formats/gp.md`), everything else uses the in-game set 0.
pub fn sprite_palette(gp_path: &str) -> &'static str {
    let path = crate::normalize_path(gp_path);
    if SET_1.contains(&path.as_str()) {
        SPRITE_PALETTES[1]
    } else if SET_2.contains(&path.as_str()) {
        SPRITE_PALETTES[2]
    } else {
        SPRITE_PALETTES[0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_palettes_by_path() {
        assert_eq!(sprite_palette("ENGB17C.GP"), "0\\AGEW_1.PAL");
        assert_eq!(sprite_palette("interf2\\Buttons.gp"), "1\\AGEW_1.PAL");
        assert_eq!(sprite_palette("/INTERFACE/MAIN_MENU.GP"), "2\\AGEW_1.PAL");
        assert_eq!(sprite_palette("INTERFACE\\CURSGO.GP"), "0\\AGEW_1.PAL");
    }

    #[test]
    fn palette_lists_are_normalized() {
        for path in SET_1.iter().chain(SET_2) {
            assert_eq!(crate::normalize_path(path), *path);
            assert!(!(SET_1.contains(path) && SET_2.contains(path)), "{path}");
        }
    }
}
