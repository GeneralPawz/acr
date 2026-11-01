//! DOM keyboard events as winit keys, so forwarded keys take the path of window keys.

use acr_ui_protocol::Mods;
use winit::keyboard::{Key, ModifiersState, NamedKey, SmolStr};

/// The logical key of a DOM `KeyboardEvent.key` (`code` is unused for now: the game reacts to
/// logical keys). `None` for modifiers, dead keys and the like, which the game ignores.
pub fn dom_key(key: &str, _code: &str) -> Option<Key> {
    let named = match key {
        "Enter" => NamedKey::Enter,
        "Escape" => NamedKey::Escape,
        "ArrowUp" => NamedKey::ArrowUp,
        "ArrowDown" => NamedKey::ArrowDown,
        "ArrowLeft" => NamedKey::ArrowLeft,
        "ArrowRight" => NamedKey::ArrowRight,
        "PageUp" => NamedKey::PageUp,
        "PageDown" => NamedKey::PageDown,
        "Home" => NamedKey::Home,
        "End" => NamedKey::End,
        "Backspace" => NamedKey::Backspace,
        "Delete" => NamedKey::Delete,
        "Insert" => NamedKey::Insert,
        "Tab" => NamedKey::Tab,
        " " => NamedKey::Space,
        "F1" => NamedKey::F1,
        "F2" => NamedKey::F2,
        "F3" => NamedKey::F3,
        "F4" => NamedKey::F4,
        "F5" => NamedKey::F5,
        "F6" => NamedKey::F6,
        "F7" => NamedKey::F7,
        "F8" => NamedKey::F8,
        "F9" => NamedKey::F9,
        "F10" => NamedKey::F10,
        "F11" => NamedKey::F11,
        "F12" => NamedKey::F12,
        _ => {
            // One character (a grapheme may be several code points, but never a name).
            let mut chars = key.chars();
            return match (chars.next(), chars.next()) {
                (Some(c), None) if !c.is_control() => Some(Key::Character(SmolStr::new(key))),
                _ => None,
            };
        }
    };
    Some(Key::Named(named))
}

/// The text a key press types: the character of a character key.
pub fn typed(key: &Key) -> Option<&str> {
    match key {
        Key::Character(text) => Some(text.as_str()),
        _ => None,
    }
}

pub fn modifiers(mods: Mods) -> ModifiersState {
    let mut state = ModifiersState::empty();
    state.set(ModifiersState::SHIFT, mods.shift);
    state.set(ModifiersState::CONTROL, mods.ctrl);
    state.set(ModifiersState::ALT, mods.alt);
    state.set(ModifiersState::SUPER, mods.meta);
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_characters() {
        assert_eq!(dom_key("Escape", "Escape"), Some(Key::Named(NamedKey::Escape)));
        assert_eq!(dom_key(" ", "Space"), Some(Key::Named(NamedKey::Space)));
        assert_eq!(dom_key("F12", "F12"), Some(Key::Named(NamedKey::F12)));
        assert_eq!(dom_key("ä", "Quote"), Some(Key::Character("ä".into())));
        assert_eq!(typed(&dom_key("A", "KeyA").unwrap()), Some("A"));
        for ignored in ["Shift", "Control", "Dead", "Process", "Unidentified", ""] {
            assert_eq!(dom_key(ignored, ""), None, "{ignored}");
        }
    }

    #[test]
    fn typed_umlauts_and_the_euro_sign_arrive_as_cp1252() {
        let bytes: Vec<u8> = ["Ä", "Ö", "ü", "€"]
            .iter()
            .map(|c| crate::cp1252(typed(&dom_key(c, "").unwrap()).unwrap()).unwrap())
            .collect();
        assert_eq!(bytes, [0xc4, 0xd6, 0xfc, 0x80]);
    }

    #[test]
    fn modifiers_map_to_winit() {
        let state = modifiers(Mods { shift: true, alt: true, ..Default::default() });
        assert_eq!(state, ModifiersState::SHIFT | ModifiersState::ALT);
    }
}
