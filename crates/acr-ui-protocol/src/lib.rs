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
        /// The player's `ui_skin` setting; the UI resolves `auto` itself from the viewport
        /// ([`FromUi::Skin`] tells the game what it chose).
        #[serde(default)]
        skin: SkinSetting,
    },
    View(Box<ViewPatch>),
    /// The game refused the [`FromUi::Intent`] numbered `seq`; nothing changed. `reason` is
    /// for the log and the developer, not for the player.
    Rejected {
        seq: u32,
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub enum Host {
    Native,
    Web,
}

/// The `ui_skin` setting (`settings.txt`): `auto` is the classic skin with a fine pointer on a wide
/// viewport and the touch skin otherwise (docs/webui.md "Touch skin").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub enum SkinSetting {
    #[default]
    Auto,
    Classic,
    Touch,
}

/// The skin the UI draws with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub enum Skin {
    #[default]
    Classic,
    Touch,
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
    /// The touch skin's view of the interface (only while the UI named [`Skin::Touch`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub touch: Option<Box<TouchHud>>,
}

/// The in-game interface as the touch skin lays it out: semantic pieces, each with the draws of
/// its original art in its own pixel space, instead of the classic skin's canvas groups. Taps go
/// back as intents on the ids (`hud/<slot>`, `army/<slot>`, `bar/menu`, `bar/pause`, `minimap`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub struct TouchHud {
    /// The six resources, in the bar's order.
    pub resources: Vec<ResourceView>,
    /// The command grid's icons (the selected types and formations, the commands, production).
    pub icons: Vec<IconView>,
    /// The army list's icons.
    pub army: Vec<IconView>,
    /// The game is paused.
    pub paused: bool,
    /// The game has a minimap (`minimap` takes taps on it).
    pub minimap: bool,
    /// The game speed as the options' slider shows it, `100 - FPSTime` (`0..=100`); `bar/speed`
    /// sets it (`set_value`).
    pub speed: i32,
    /// The original's words for the slow and the fast end (`SLOWMOD`, `FASTMOD`).
    pub speed_text: [String; 2],
}

/// A resource of the top bar: its icon and number.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub struct ResourceView {
    /// `wood`, `gold`, `stone`, `food`, `iron` or `coal`.
    pub key: String,
    pub value: u32,
    /// `[w, h]` of the draws' box.
    pub size: [i32; 2],
    pub parts: Vec<Part>,
}

/// What an icon asks for when it is tapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub enum IconRole {
    /// Keeps a selected type or formation (the selection summary).
    Select,
    /// A command, a production or an upgrade.
    Command,
    /// A formation of the army list.
    Army,
}

/// One icon of the command grid or the army list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub struct IconView {
    /// `hud/<slot>` or `army/<slot>`.
    pub id: String,
    pub role: IconRole,
    /// `[w, h]` of the draws' box.
    pub size: [i32; 2],
    pub parts: Vec<Part>,
    /// The command's glyph of the touch skin's own icon set (`ui/src/lib/icons/`), named by what
    /// it does; `parts` then hold only what goes over it (outline, life bar, numbers). `None`:
    /// `parts` are the original's picture, enlarged by the pixel-art scaler.
    pub glyph: Option<String>,
    /// The hint text (empty: none), for a long press.
    pub hint: String,
    pub disabled: bool,
    pub selected: bool,
    /// A tap does something (the icon is not disabled and has a command).
    pub clickable: bool,
    /// A unit production icon: a click with Ctrl held is an endless order (`GetProduceCount` 1200).
    pub endless: bool,
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
    /// The id of the widget that goes back (the one its Escape key or its back or cancel button
    /// triggers), if the screen shows one: the touch skin's header arrow sends [`FromUi::Back`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub back: Option<String>,
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
    /// A name that stays with the widget when the dialog is rebuilt (`Dialog::set_key`), for
    /// a skin that lays widgets out by role. Not every widget has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub key: Option<String>,
    pub kind: WidgetKind,
    /// `[x, y, w, h]` of the area that reacts to the mouse, in canvas pixels.
    pub rect: [i32; 4],
    pub enabled: bool,
    /// The widget is visible and inside its clip window: the player can see and press it. A skin
    /// that lays widgets out by role (the touch skin) leaves the others out.
    pub shown: bool,
    pub hover: bool,
    /// The widget's text, for a skin that lays widgets out by role (a button's or check's label).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub label: Option<String>,
    /// The widget's state: a list's rows, a combo box's options, a slider's position, a text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub value: Option<WidgetValue>,
    /// What the widget draws, in order.
    pub parts: Vec<Part>,
}

/// The state a skin that does not draw the original's pixels needs of a widget.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub enum WidgetValue {
    List {
        rows: Vec<String>,
        selected: Option<u32>,
    },
    Combo {
        options: Vec<String>,
        selected: u32,
        open: bool,
    },
    /// A slider: `0..=max`.
    Range {
        value: i32,
        max: i32,
    },
    Input {
        text: String,
        capacity: u32,
    },
    Viewer {
        text: String,
    },
    /// A check or radio button of a strip.
    Check {
        on: bool,
    },
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
    /// The skin the UI draws with now (it resolves `ui_skin=auto` from the viewport and the
    /// pointer). The touch skin takes the HUD's pixels away from the game and gets
    /// [`ViewPatch::touch`]; sent after [`ToUi::Hello`] and whenever the choice changes.
    Skin { skin: Skin },
    /// A finger on the map (physical pixels), for the game's touch gestures (`touch.rs`).
    Touch {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        id: u64,
        phase: TouchPhase,
        x: f32,
        y: f32,
    },
    /// The touch skin asks for a widget's effect without a pixel: Rust checks it and answers
    /// [`ToUi::Rejected`] with the same `seq` if it cannot be applied. `id` is a widget's id
    /// (`<Screen>/<index>`), a HUD icon's (`hud/<slot>`, `army/<slot>`) or `minimap`.
    Intent { seq: u32, id: String, action: Action },
    /// The touch skin's back (its header arrow): the top screen's own back or cancel widget
    /// ([`ScreenPatch::back`]) is pressed, as its Escape key would. Answered
    /// [`ToUi::Rejected`] with the same `seq` if the screen shows none.
    Back { seq: u32 },
}

/// What an [`FromUi::Intent`] does to its widget; the effect is that of the original input
/// on the widget (`Dialog::activate` and the other setters, `InGame::hud_event`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = export_dir!()))]
pub enum Action {
    /// A click on a button, check, drop-down box, input box or HUD icon (`right`: its
    /// right-click command), with the modifier keys held.
    Click {
        button: Button,
        #[serde(default)]
        mods: Mods,
    },
    /// The row `row` (index into all rows) of a list.
    SelectRow { row: u32 },
    /// The entry `option` of a drop-down box.
    SelectOption { option: u32 },
    /// A slider's position.
    SetValue { value: i32 },
    /// The whole text of an input box.
    SetText { text: String },
    /// A point of the minimap, `uv` in `0..=1` from its top-left corner (`id` `minimap`):
    /// left centres the camera, right orders the selection there.
    MinimapAt { uv: [f32; 2], button: Button },
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
pub enum TouchPhase {
    Started,
    Moved,
    Ended,
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
        let json = serde_json::to_string(&ToUi::View(Box::new(patch.clone()))).unwrap();
        assert_eq!(json, r#"{"t":"view","rev":3,"loading":null,"cursor":null}"#);
        assert_eq!(serde_json::from_str::<ToUi>(&json).unwrap(), ToUi::View(Box::new(patch)));
        let json = serde_json::to_string(&ToUi::View(Box::new(ViewPatch { rev: 4, ..Default::default() }))).unwrap();
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

    #[test]
    fn a_widgets_key_is_sent_only_when_it_has_one() {
        let widget = |key: Option<&str>| WidgetView {
            id: "S/1".into(),
            key: key.map(str::to_owned),
            kind: WidgetKind::Button,
            rect: [0; 4],
            enabled: true,
            shown: true,
            hover: false,
            label: None,
            value: None,
            parts: Vec::new(),
        };
        let without = serde_json::to_string(&widget(None)).unwrap();
        assert!(!without.contains("key"), "{without}");
        let with = serde_json::to_string(&widget(Some("ok"))).unwrap();
        assert!(with.contains(r#""key":"ok""#), "{with}");
        assert_eq!(serde_json::from_str::<WidgetView>(&without).unwrap(), widget(None));
        assert_eq!(serde_json::from_str::<WidgetView>(&with).unwrap(), widget(Some("ok")));
    }

    #[test]
    fn intents_and_their_refusal_round_trip() {
        let json = r#"{"t":"intent","seq":7,"id":"Options/3","action":{"t":"set_value","value":40}}"#;
        let FromUi::Intent { seq, id, action } = serde_json::from_str(json).unwrap() else { panic!() };
        assert_eq!((seq, id.as_str(), action), (7, "Options/3", Action::SetValue { value: 40 }));
        let rejected = ToUi::Rejected { seq: 7, reason: "no widget".into() };
        assert_eq!(serde_json::to_string(&rejected).unwrap(), r#"{"t":"rejected","seq":7,"reason":"no widget"}"#);
        assert!(serde_json::from_str::<FromUi>(r#"{"t":"intent","seq":1,"id":"a","action":{"t":"nope"}}"#).is_err());
    }

    #[test]
    fn back_is_a_message_of_its_own() {
        let FromUi::Back { seq } = serde_json::from_str(r#"{"t":"back","seq":3}"#).unwrap() else { panic!() };
        assert_eq!(seq, 3);
    }

    #[test]
    fn the_skin_travels_both_ways_and_a_touch_is_a_finger() {
        let hello =
            ToUi::Hello { protocol: PROTOCOL, host: Host::Web, asset_base: String::new(), skin: SkinSetting::Touch };
        let json = serde_json::to_string(&hello).unwrap();
        assert!(json.contains(r#""skin":"touch""#), "{json}");
        // An older game says nothing: `auto`.
        let old = r#"{"t":"hello","protocol":1,"host":"web","asset_base":""}"#;
        assert_eq!(
            serde_json::from_str::<ToUi>(old).unwrap(),
            ToUi::Hello { protocol: 1, host: Host::Web, asset_base: String::new(), skin: SkinSetting::Auto }
        );
        let skin: FromUi = serde_json::from_str(r#"{"t":"skin","skin":"touch"}"#).unwrap();
        assert_eq!(skin, FromUi::Skin { skin: Skin::Touch });
        let touch: FromUi = serde_json::from_str(r#"{"t":"touch","id":2,"phase":"started","x":1.5,"y":2}"#).unwrap();
        assert_eq!(touch, FromUi::Touch { id: 2, phase: TouchPhase::Started, x: 1.5, y: 2.0 });
        assert!(serde_json::from_str::<FromUi>(r#"{"t":"skin","skin":"auto"}"#).is_err());
    }

    #[test]
    fn a_widget_carries_its_label_and_value_only_when_it_has_them() {
        let widget = WidgetView {
            id: "S/2".into(),
            key: None,
            kind: WidgetKind::List,
            rect: [0; 4],
            enabled: true,
            shown: true,
            hover: false,
            label: None,
            value: Some(WidgetValue::List { rows: vec!["a".into()], selected: Some(0) }),
            parts: Vec::new(),
        };
        let json = serde_json::to_string(&widget).unwrap();
        assert!(
            !json.contains("label") && json.contains(r#""value":{"t":"list","rows":["a"],"selected":0}"#),
            "{json}"
        );
        assert_eq!(serde_json::from_str::<WidgetView>(&json).unwrap(), widget);
    }
}
