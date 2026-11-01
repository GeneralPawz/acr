//! The contract between the game and its Svelte UI (tracking issue #316, section 7), version 1.
//!
//! Rust keeps all UI state and logic and publishes a view model ([`ToUi`]); the UI draws it
//! and forwards raw input ([`FromUi`]). Message enums are tagged with `t` in snake case; in a
//! [`ViewPatch`] a `None` field means "unchanged" and is left out of the JSON.
//!
//! With the `ts` feature every type derives `ts_rs::TS`; the export test writes the types to
//! `ui/src/lib/protocol/`, which is committed and checked for no diff.

use serde::{Deserialize, Serialize};

/// Version of this contract. A UI only claims screens when its [`FromUi::Ready`] names the
/// same number.
pub const PROTOCOL: u32 = 1;

/// Where the TypeScript types go, relative to ts-rs's `bindings` folder of this crate.
#[cfg(feature = "ts")]
macro_rules! export_dir {
    () => {
        "../../../ui/src/lib/protocol/"
    };
}

/// `Option<Option<T>>` fields of a patch: absent = unchanged, `null` = cleared.
mod nullable {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<T: Serialize, S: Serializer>(value: &Option<Option<T>>, s: S) -> Result<S::Ok, S::Error> {
        match value {
            Some(inner) => inner.serialize(s),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
        Option::<T>::deserialize(d).map(Some)
    }
}

/// Rust to UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub enum ToUi {
    /// The answer to [`FromUi::Ready`]; `asset_base` is the prefix of asset paths for the
    /// host (empty in the browser, where the bridge fetches assets itself).
    Hello {
        protocol: u32,
        host: Host,
        asset_base: String,
    },
    View(ViewPatch),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub enum Host {
    Native,
    Web,
}

/// What the app shows (`Game.mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub enum Mode {
    Intro,
    Menus,
    Loading,
    Game,
}

/// Changed topics of the view model since the last patch. `rev` grows by one with every
/// patch; a gap means a lost patch and the UI answers with [`FromUi::Resync`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub struct ViewPatch {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub rev: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<Mode>,
    /// Replay progress of a loading game, 0..=1; `null` = none.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "nullable")]
    #[cfg_attr(feature = "ts", ts(type = "number | null", optional))]
    pub loading: Option<Option<f32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<Box<Stage>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<ScreenPatch>,
    /// The engine's cursor as an image; `null` = none (the system's cursor).
    #[serde(default, skip_serializing_if = "Option::is_none", with = "nullable")]
    #[cfg_attr(feature = "ts", ts(type = "import(\"./CursorView\").CursorView | null", optional))]
    pub cursor: Option<Option<CursorView>>,
    /// The parts of the in-game interface the UI draws (mode `game`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hud: Option<Box<HudPatch>>,
}

/// Where the two canvases lie in the window.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub struct Stage {
    /// The 1024x768 menu canvas (`Canvas::fit`).
    pub menu: CanvasView,
    /// The in-game screen (`Canvas::screen`).
    pub game: CanvasView,
}

/// A canvas in window (physical) pixels: canvas pixel `p` lies at `origin + p * scale`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub struct CanvasView {
    pub scale: f32,
    pub origin: [f32; 2],
    /// The canvas size in its own pixels.
    pub size: [i32; 2],
}

/// The top screen. While `engine_drawn` the engine draws it and `widgets` is empty.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub struct ScreenPatch {
    pub name: String,
    /// Names of all open screens, bottom first.
    pub stack: Vec<String>,
    pub in_game: bool,
    pub busy: bool,
    pub engine_drawn: bool,
    pub hint: Option<String>,
    pub widgets: WidgetsPatch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub enum WidgetsPatch {
    /// Replaces all widgets, in draw order.
    Full { widgets: Vec<WidgetView> },
    /// Replaces or adds the widgets in `upsert` (by id); `order` lists all ids in draw order
    /// when it changed.
    Changed { upsert: Vec<WidgetView>, order: Option<Vec<String>> },
}

/// One widget of a dialog. `id` is `<Screen>/<index>`, as in the test driver's widget tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub struct WidgetView {
    pub id: String,
    pub kind: WidgetKind,
    /// `[x, y, w, h]` of the area that reacts to the mouse, in canvas pixels.
    pub rect: [i32; 4],
    pub enabled: bool,
    pub hover: bool,
    /// What the widget draws, in order.
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub enum WidgetKind {
    Area,
    Outline,
    Picture,
    Text,
    Button,
    Strip,
    List,
    Combo,
    Input,
    Viewer,
    Slider,
    Scrollbar,
}

/// A drawn piece of a widget, resolved by the engine. Positions are canvas pixels; `url` is
/// an asset path for the bridge (`bmp/..`, `frame/..`, `text/..`); `clip` is `[x, y, w, h]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub enum Part {
    Sprite {
        url: String,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        alpha: f32,
        clip: Option<[i32; 4]>,
    },
    /// A string of an original font, laid out by the UI from the font's glyph atlas. `x`, `y`
    /// is the position `ShowString` takes.
    Text {
        font: String,
        set: u8,
        text: String,
        x: i32,
        y: i32,
        fade: u8,
        clip: Option<[i32; 4]>,
    },
    Fill {
        rgba: [u8; 4],
        rect: [i32; 4],
        clip: Option<[i32; 4]>,
    },
}

/// The changed groups of the in-game interface. A group is a layer of draw parts on the game
/// canvas (`Stage::game`), replaced as a whole; resources and the top bar's frames are groups
/// of their own, so a changed number does not resend the frames.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub struct HudPatch {
    /// Groups that are new or changed.
    pub groups: Vec<HudGroup>,
    /// Groups that only moved: the same parts, shifted to a new `offset` (a hint follows the mouse).
    pub moved: Vec<HudMove>,
    /// Names of groups that are gone.
    pub removed: Vec<String>,
}

/// A group that moved without changing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub struct HudMove {
    pub name: String,
    pub offset: [i32; 2],
}

/// One layer of the in-game interface, in game canvas pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub struct HudGroup {
    pub name: String,
    /// Stacking order, low first.
    pub layer: u8,
    /// `hud_scale`: the group is drawn `scale` times larger about `anchor` (1: as it is).
    pub anchor: [f32; 2],
    pub scale: f32,
    /// The parts' shift in game canvas pixels (a [`HudMove`] changes it); the scale is about the
    /// shifted anchor.
    #[serde(default)]
    pub offset: [i32; 2],
    pub parts: Vec<Part>,
}

/// The engine's cursor image; `hotspot` is the pixel of the image that points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub struct CursorView {
    pub url: String,
    pub hotspot: [i32; 2],
}

/// UI to Rust.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub enum FromUi {
    /// The UI is mounted. `screens` are the screens it draws itself and `hud` the parts of the
    /// in-game interface; the engine stops drawing exactly those, and only if `protocol` is
    /// [`PROTOCOL`].
    Ready {
        protocol: u32,
        screens: Vec<String>,
        #[serde(default)]
        hud: Vec<String>,
        viewport: [f32; 2],
        dpr: f32,
    },
    /// The page size in CSS pixels changed.
    Viewport { viewport: [f32; 2], dpr: f32 },
    /// Send the whole view model again.
    Resync,
    /// A mouse (or pen or touch) pointer; `x`, `y` in window (physical) pixels.
    Pointer { phase: Phase, x: f32, y: f32, button: Option<Button>, mods: Mods },
    /// Wheel notches, positive away from the user (up).
    Wheel { x: f32, y: f32, lines: f32, mods: Mods },
    /// A DOM keyboard event (`KeyboardEvent.key` and `.code`).
    Key { key: String, code: String, down: bool, repeat: bool, mods: Mods },
    /// The page lost the focus: held keys and buttons are released.
    Blur,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub enum Phase {
    Move,
    Down,
    Up,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub enum Button {
    Left,
    Middle,
    Right,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patches_leave_unchanged_topics_out_and_keep_nulls() {
        let patch = ViewPatch { rev: 3, loading: Some(None), cursor: Some(None), ..Default::default() };
        let json = serde_json::to_string(&ToUi::View(patch.clone())).unwrap();
        assert_eq!(json, r#"{"t":"view","rev":3,"loading":null,"cursor":null}"#);
        assert_eq!(serde_json::from_str::<ToUi>(&json).unwrap(), ToUi::View(patch));
        let json = serde_json::to_string(&ToUi::View(ViewPatch { rev: 4, ..Default::default() })).unwrap();
        assert_eq!(json, r#"{"t":"view","rev":4}"#);
    }

    #[test]
    fn messages_are_tagged_in_snake_case() {
        let ready = r#"{"t":"ready","protocol":1,"screens":["MainMenu"],"viewport":[800,600],"dpr":1.5}"#;
        let FromUi::Ready { protocol, screens, .. } = serde_json::from_str(ready).unwrap() else { panic!() };
        assert_eq!((protocol, screens), (PROTOCOL, vec!["MainMenu".to_owned()]));
        let part = Part::Fill { rgba: [1, 2, 3, 4], rect: [0, 0, 5, 6], clip: None };
        assert_eq!(
            serde_json::to_string(&part).unwrap(),
            r#"{"t":"fill","rgba":[1,2,3,4],"rect":[0,0,5,6],"clip":null}"#
        );
        assert!(serde_json::from_str::<FromUi>(r#"{"t":"nope"}"#).is_err());
    }
}
