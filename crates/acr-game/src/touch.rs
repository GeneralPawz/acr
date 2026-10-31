//! Touch gestures (#331): fingers on a phone or tablet turned into the mouse and a few game
//! commands. Pure: the window feeds finger events and a clock, the game applies what comes out.
//!
//! | Gesture | Result |
//! |---|---|
//! | tap | left click |
//! | drag | left button held (a selection rectangle, a dragged slider) |
//! | long press | right click where the finger is (on a production icon in the game: the Ctrl+click, endless production, #438) |
//! | long press, then swipe | right button dragged: the order's direction arrow (formations face it) |
//! | two fingers | the middle button at their middle: moving them together pans the map |
//! | pinch (while the two fingers pan) | zoom about the fingers' middle (the wheel's zoom) |
//! | a triangle drawn like ▶ while paused | unpause |

use std::time::Duration;

/// What the game does for a gesture. Positions are window pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gesture {
    Move([f64; 2]),
    LeftDown,
    LeftUp,
    RightDown,
    RightUp,
    /// The middle button, which pans the map (modern controls).
    MiddleDown,
    MiddleUp,
    Unpause,
    /// Wheel notches, positive in.
    Zoom(f32),
}

/// A finger's phase, as winit's `TouchPhase`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Started,
    Moved,
    Ended,
}

/// How long a finger rests before it holds the right button.
const LONG_PRESS: Duration = Duration::from_millis(500);
/// Density-independent pixels a finger may wander and still tap or long-press.
const SLOP_DP: f64 = 10.0;
/// A change of the fingers' distance (as a log ratio) from which two fingers also zoom, so
/// the jitter of a pan does not.
const PINCH_LOG: f64 = 0.12;
/// The smallest triangle drawn, both sides.
const TRIANGLE_DP: f64 = 50.0;
/// Pinch: the wheel's zoom step (`controls::ZOOM_STEP`, 2^(1/4)) per notch.
const NOTCH_LOG: f64 = std::f64::consts::LN_2 / 4.0;

#[derive(Clone, Debug, PartialEq)]
enum State {
    Idle,
    /// One finger down, not yet moved: a tap, a long press or the start of a drag.
    Pending {
        since: Duration,
    },
    /// One finger dragging with the left button held.
    Dragging,
    /// One finger drawing while the game is paused: buffered, maybe the ▶.
    Tracing,
    /// One finger holding the right button after a long press; moving it drags the order's
    /// direction arrow, lifting it gives the order.
    RightHeld,
    /// Two fingers, holding the middle button at their middle (a pan), and zooming once their
    /// distance has changed enough.
    Two {
        start_dist: f64,
        last_dist: f64,
        pinching: bool,
    },
    /// The gesture is over; wait until every finger is up.
    Done,
}

#[derive(Clone, Debug)]
struct Finger {
    id: u64,
    pos: [f64; 2],
}

/// The fingers on the screen and the gesture they make.
#[derive(Clone, Debug)]
pub struct Touches {
    state: State,
    fingers: Vec<Finger>,
    /// The first finger's path, from where it touched.
    path: Vec<[f64; 2]>,
    /// Window pixels per density-independent pixel.
    scale: f64,
}

impl Default for Touches {
    fn default() -> Self {
        Self { state: State::Idle, fingers: Vec::new(), path: Vec::new(), scale: 1.0 }
    }
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn mid(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
}

impl Touches {
    /// Sets the window's scale factor (pixels per density-independent pixel).
    pub fn set_scale(&mut self, scale: f64) {
        self.scale = scale.max(0.5);
    }

    /// A finger event at `now`; `paused` says whether the game is paused (only then is a
    /// one-finger drawing held back, since it may be the ▶).
    pub fn event(&mut self, id: u64, phase: Phase, pos: [f64; 2], now: Duration, paused: bool) -> Vec<Gesture> {
        let mut out = Vec::new();
        match phase {
            Phase::Started => {
                if self.fingers.iter().any(|f| f.id == id) {
                    return out;
                }
                self.fingers.push(Finger { id, pos });
                match (&self.state, self.fingers.len()) {
                    (State::Idle, 1) => {
                        self.path = vec![pos];
                        self.state = State::Pending { since: now };
                    }
                    (State::Pending { .. } | State::Tracing | State::Dragging, 2) => {
                        if self.state == State::Dragging {
                            out.push(Gesture::LeftUp);
                        }
                        let (a, b) = (self.fingers[0].pos, self.fingers[1].pos);
                        let d = dist(a, b).max(1.0);
                        out.extend([Gesture::Move(mid(a, b)), Gesture::MiddleDown]);
                        self.state = State::Two { start_dist: d, last_dist: d, pinching: false };
                    }
                    _ => {}
                }
            }
            Phase::Moved => {
                let Some(i) = self.fingers.iter().position(|f| f.id == id) else { return out };
                self.fingers[i].pos = pos;
                match &mut self.state {
                    State::Pending { .. } if i == 0 => {
                        self.path.push(pos);
                        if dist(pos, self.path[0]) > SLOP_DP * self.scale {
                            if paused {
                                self.state = State::Tracing;
                            } else {
                                out.extend([Gesture::Move(self.path[0]), Gesture::LeftDown, Gesture::Move(pos)]);
                                self.state = State::Dragging;
                            }
                        }
                    }
                    State::Dragging | State::RightHeld if i == 0 => out.push(Gesture::Move(pos)),
                    State::Tracing if i == 0 => self.path.push(pos),
                    State::Two { start_dist, last_dist, pinching } if i < 2 => {
                        let (a, b) = (self.fingers[0].pos, self.fingers[1].pos);
                        let (d, m) = (dist(a, b).max(1.0), mid(a, b));
                        out.push(Gesture::Move(m));
                        *pinching |= (d / *start_dist).ln().abs() > PINCH_LOG;
                        if *pinching {
                            out.push(Gesture::Zoom(((d / *last_dist).ln() / NOTCH_LOG) as f32));
                            *last_dist = d;
                        }
                    }
                    _ => {}
                }
            }
            Phase::Ended => {
                let Some(i) = self.fingers.iter().position(|f| f.id == id) else { return out };
                self.fingers[i].pos = pos;
                if i == 0 {
                    match self.state {
                        State::Pending { .. } => {
                            out.extend([Gesture::Move(self.path[0]), Gesture::LeftDown, Gesture::LeftUp]);
                        }
                        State::Dragging => out.extend([Gesture::Move(pos), Gesture::LeftUp]),
                        State::RightHeld => out.extend([Gesture::Move(pos), Gesture::RightUp]),
                        State::Tracing => {
                            self.path.push(pos);
                            if is_play_triangle(&self.path, TRIANGLE_DP * self.scale) {
                                out.push(Gesture::Unpause);
                            } else {
                                out.extend([
                                    Gesture::Move(self.path[0]),
                                    Gesture::LeftDown,
                                    Gesture::Move(pos),
                                    Gesture::LeftUp,
                                ]);
                            }
                        }
                        _ => {}
                    }
                }
                if matches!(self.state, State::Two { .. }) {
                    out.push(Gesture::MiddleUp);
                }
                self.fingers.remove(i);
                self.state = if self.fingers.is_empty() { State::Idle } else { State::Done };
            }
        }
        out
    }

    /// The clock at `now`: a finger resting long enough presses the right button.
    pub fn tick(&mut self, now: Duration) -> Vec<Gesture> {
        match self.state {
            State::Pending { since } if now.saturating_sub(since) >= LONG_PRESS => {
                self.state = State::RightHeld;
                vec![Gesture::Move(self.path[0]), Gesture::RightDown]
            }
            _ => Vec::new(),
        }
    }
}

/// Whether a closed stroke is a triangle pointing right (▶): its rightmost point about
/// halfway down, its left side spanning most of its height, and its area about half its
/// bounding box (a circle fills 0.79, a square 1).
fn is_play_triangle(path: &[[f64; 2]], min_size: f64) -> bool {
    if path.len() < 8 {
        return false;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in path {
        (x0, y0, x1, y1) = (x0.min(p[0]), y0.min(p[1]), x1.max(p[0]), y1.max(p[1]));
    }
    let (w, h) = (x1 - x0, y1 - y0);
    if w < min_size || h < min_size || !(0.5..=2.5).contains(&(h / w)) {
        return false;
    }
    if dist(path[0], path[path.len() - 1]) > 0.35 * w.max(h) {
        return false;
    }
    let right = path.iter().fold(path[0], |r, p| if p[0] > r[0] { *p } else { r });
    if (right[1] - (y0 + y1) / 2.0).abs() > 0.25 * h {
        return false;
    }
    let left: Vec<f64> = path.iter().filter(|p| p[0] < x0 + 0.25 * w).map(|p| p[1]).collect();
    let span = left.iter().cloned().fold(f64::MIN, f64::max) - left.iter().cloned().fold(f64::MAX, f64::min);
    if span < 0.6 * h {
        return false;
    }
    let n = path.len();
    let area = (0..n).map(|i| {
        let (a, b) = (path[i], path[(i + 1) % n]);
        a[0] * b[1] - b[0] * a[1]
    });
    let fill = area.sum::<f64>().abs() / 2.0 / (w * h);
    (0.3..=0.65).contains(&fill)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// Points along the corners' polygon, closed, `per` points per side.
    fn stroke(corners: &[[f64; 2]], per: usize) -> Vec<[f64; 2]> {
        let mut out = Vec::new();
        for i in 0..corners.len() {
            let (a, b) = (corners[i], corners[(i + 1) % corners.len()]);
            for k in 0..per {
                let t = k as f64 / per as f64;
                out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
            }
        }
        out.push(corners[0]);
        out
    }

    #[test]
    fn a_tap_clicks_and_a_drag_holds_the_left_button() {
        let mut t = Touches::default();
        assert!(t.event(1, Phase::Started, [100.0, 100.0], ms(0), false).is_empty());
        let tap = t.event(1, Phase::Ended, [102.0, 101.0], ms(80), false);
        assert_eq!(tap, [Gesture::Move([100.0, 100.0]), Gesture::LeftDown, Gesture::LeftUp]);
        t.event(1, Phase::Started, [100.0, 100.0], ms(200), false);
        let start = t.event(1, Phase::Moved, [150.0, 100.0], ms(220), false);
        assert_eq!(start, [Gesture::Move([100.0, 100.0]), Gesture::LeftDown, Gesture::Move([150.0, 100.0])]);
        assert_eq!(
            t.event(1, Phase::Ended, [160.0, 100.0], ms(300), false),
            [Gesture::Move([160.0, 100.0]), Gesture::LeftUp]
        );
        assert!(t.tick(ms(2000)).is_empty());
    }

    #[test]
    fn a_long_press_is_a_right_click_and_a_swipe_after_it_drags_the_arrow() {
        let mut t = Touches::default();
        t.event(1, Phase::Started, [50.0, 60.0], ms(0), false);
        assert!(t.tick(ms(400)).is_empty());
        assert_eq!(t.tick(ms(500)), [Gesture::Move([50.0, 60.0]), Gesture::RightDown]);
        assert!(t.tick(ms(900)).is_empty());
        assert_eq!(
            t.event(1, Phase::Ended, [51.0, 60.0], ms(1000), false),
            [Gesture::Move([51.0, 60.0]), Gesture::RightUp]
        );

        t.event(1, Phase::Started, [50.0, 60.0], ms(2000), false);
        t.tick(ms(2600));
        assert_eq!(t.event(1, Phase::Moved, [150.0, 20.0], ms(2700), false), [Gesture::Move([150.0, 20.0])]);
        assert_eq!(
            t.event(1, Phase::Ended, [180.0, 0.0], ms(2800), false),
            [Gesture::Move([180.0, 0.0]), Gesture::RightUp]
        );
    }

    fn zoom_of(gestures: &[Gesture]) -> f32 {
        gestures.iter().map(|g| if let Gesture::Zoom(l) = g { *l } else { 0.0 }).sum()
    }

    #[test]
    fn two_fingers_moving_together_pan_the_map() {
        let mut t = Touches::default();
        t.event(1, Phase::Started, [100.0, 100.0], ms(0), false);
        let start = t.event(2, Phase::Started, [160.0, 100.0], ms(10), false);
        assert_eq!(start, [Gesture::Move([130.0, 100.0]), Gesture::MiddleDown]);
        let mut all = Vec::new();
        for k in 1..=10 {
            let (dx, dy) = (7.0 * f64::from(k), 10.0 * f64::from(k));
            all.extend(t.event(1, Phase::Moved, [100.0 + dx, 100.0 + dy], ms(20 + k as u64), false));
            all.extend(t.event(2, Phase::Moved, [160.0 + dx, 100.0 + dy], ms(20 + k as u64), false));
        }
        assert!(all.iter().all(|g| matches!(g, Gesture::Move(_))), "a pan has no zoom: {all:?}");
        assert_eq!(all.last(), Some(&Gesture::Move([200.0, 200.0])));
        assert_eq!(t.event(1, Phase::Ended, [170.0, 200.0], ms(100), false), [Gesture::MiddleUp]);
        assert!(t.event(2, Phase::Ended, [230.0, 200.0], ms(110), false).is_empty(), "no click after the pan");
        assert!(t.tick(ms(2000)).is_empty());
    }

    #[test]
    fn two_fingers_sliding_down_no_longer_pause() {
        let mut t = Touches::default();
        t.event(1, Phase::Started, [100.0, 100.0], ms(0), false);
        t.event(2, Phase::Started, [160.0, 100.0], ms(10), false);
        let mut all = Vec::new();
        for k in 1..=20 {
            let y = 100.0 + 10.0 * f64::from(k);
            all.extend(t.event(1, Phase::Moved, [100.0, y], ms(20 + k as u64), false));
            all.extend(t.event(2, Phase::Moved, [160.0, y], ms(20 + k as u64), false));
        }
        assert!(all.iter().all(|g| matches!(g, Gesture::Move(_))), "{all:?}");
    }

    #[test]
    fn a_pinch_zooms_while_it_pans() {
        let mut t = Touches::default();
        t.event(1, Phase::Started, [100.0, 100.0], ms(0), false);
        t.event(2, Phase::Started, [200.0, 100.0], ms(10), false);
        let mut all = Vec::new();
        for k in 1..=10 {
            all.extend(t.event(2, Phase::Moved, [200.0 + 10.0 * f64::from(k), 100.0], ms(220), false));
        }
        // The fingers spread from 100 to 200 px: twice as far, four notches in.
        assert!((zoom_of(&all) - 4.0).abs() < 0.6, "{all:?}");
        // The middle moved with the spreading finger, so the map panned too.
        assert!(all.contains(&Gesture::Move([200.0, 100.0])));
        // Moving both fingers on while spread: pan and zoom in the same event stream.
        let more = t.event(1, Phase::Moved, [120.0, 130.0], ms(300), false);
        assert!(more.iter().any(|g| matches!(g, Gesture::Move(_))) && zoom_of(&more) < 0.0, "{more:?}");
    }

    #[test]
    fn a_play_triangle_drawn_while_paused_unpauses() {
        let tri = stroke(&[[100.0, 100.0], [220.0, 170.0], [100.0, 240.0]], 12);
        let mut t = Touches::default();
        let mut out = t.event(1, Phase::Started, tri[0], ms(0), true);
        for p in &tri[1..] {
            out.extend(t.event(1, Phase::Moved, *p, ms(10), true));
        }
        out.extend(t.event(1, Phase::Ended, *tri.last().unwrap(), ms(400), true));
        assert_eq!(out, [Gesture::Unpause]);

        // Not paused, the same stroke is a drag.
        let mut t = Touches::default();
        t.event(1, Phase::Started, tri[0], ms(0), false);
        let moved = t.event(1, Phase::Moved, tri[3], ms(10), false);
        assert_eq!(moved[1], Gesture::LeftDown);
    }

    #[test]
    fn other_shapes_are_no_triangle() {
        let min = 50.0;
        assert!(is_play_triangle(&stroke(&[[0.0, 0.0], [120.0, 70.0], [0.0, 140.0]], 10), min));
        assert!(!is_play_triangle(&stroke(&[[120.0, 0.0], [0.0, 70.0], [120.0, 140.0]], 10), min), "◀");
        assert!(
            !is_play_triangle(&stroke(&[[0.0, 0.0], [120.0, 0.0], [120.0, 120.0], [0.0, 120.0]], 10), min),
            "square"
        );
        let circle: Vec<[f64; 2]> = (0..=40)
            .map(|k| f64::from(k) * std::f64::consts::TAU / 40.0)
            .map(|a| [60.0 + 60.0 * a.cos(), 60.0 + 60.0 * a.sin()])
            .collect();
        assert!(!is_play_triangle(&circle, min), "circle");
        assert!(!is_play_triangle(&stroke(&[[0.0, 0.0], [30.0, 15.0], [0.0, 30.0]], 10), min), "too small");
        assert!(!is_play_triangle(
            &[
                [0.0, 0.0],
                [100.0, 0.0],
                [200.0, 0.0],
                [300.0, 10.0],
                [400.0, 0.0],
                [500.0, 0.0],
                [600.0, 0.0],
                [700.0, 0.0]
            ],
            min
        ));
    }
}
