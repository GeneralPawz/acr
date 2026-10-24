//! Buildings under construction and upgraded in 3D (#302), render only. A building under
//! construction draws its model's construction stage group (`<name>.build<k>`, the sprite's
//! `#BUILDLO_k` the simulation shows, `construction::stage_anim`) where it has one; a model
//! without stage groups rises with the progress instead: cut at a height that climbs from a
//! foundation course to the roof, the cut stepped in courses, a scaffold band under it
//! (`model.wgsl` `rising`). A building with sections (`ADDSECTION`, raised by `UPSTAGEn`
//! upgrades) draws each section's state group (`<name>.section<s>_<state>`) over its
//! finished model, or, for a single-mesh model, its `<NAME>UP` model once every section is
//! done. See docs/rendering-3d.md "Construction and upgrades".

use acr_render::model_renderer::Rise;
use acr_sim::upgrade::{section, section_state};

/// What shows of a model without stage groups from the first stage (metres): a foundation
/// course. [`Rise::height`] also carries the stepped line's depth (`rising_below` in
/// `model.wgsl` lowers the cut by up to [`STEPS`]).
const FOUNDATION: f32 = 0.3;
const STEPS: f32 = 0.6;
/// The scaffold band under the cut (metres).
const BAND: f32 = 1.4;

/// The rise of a model `top` metres high at `progress` (0 placed .. 1 finished).
pub fn rise(progress: f32, top: f32) -> Rise {
    let p = progress.clamp(0.0, 1.0);
    Rise { height: FOUNDATION + STEPS + p * (top - FOUNDATION).max(0.0), band: BAND }
}

/// The construction stage group a building in `#BUILDLO_k` shows (0..3).
pub fn stage_group(k: u8) -> usize {
    usize::from(k.min(3))
}

/// Every section of the type (`sections`, catalog `ADDSECTION`) is done in the section
/// states `stages`: the upgraded look. False for a type without sections.
pub fn upgraded(sections: &[(u8, i32)], stages: u16) -> bool {
    !sections.is_empty() && sections.iter().all(|&(s, _)| section_state(stages, s) == section::DONE)
}

/// A section state group's name (`section<s>_<state>`), parsed.
pub fn section_group(group: &str) -> Option<(u8, u16)> {
    let (s, k) = group.strip_prefix("section")?.split_once('_')?;
    Some((s.parse().ok()?, k.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use acr_sim::upgrade::with_section;

    #[test]
    fn rise_climbs_from_a_foundation_to_the_roof() {
        let low = rise(0.0, 8.0);
        let high = rise(1.0, 8.0);
        // The stepped line lowers the cut by up to STEPS: a foundation course shows at once.
        assert!((low.height - STEPS - FOUNDATION).abs() < 1e-5);
        assert!(high.height - STEPS >= 8.0 - 1e-5);
        assert!(rise(0.5, 8.0).height > low.height && rise(0.5, 8.0).height < high.height);
    }

    #[test]
    fn upgraded_needs_every_section_done() {
        let secs = [(0u8, 10000), (1u8, 10000)];
        let one = with_section(0, 0, section::DONE);
        assert!(!upgraded(&secs, one));
        assert!(upgraded(&secs, with_section(one, 1, section::DONE)));
        assert!(!upgraded(&[], 0));
        assert!(!upgraded(&secs[..1], with_section(0, 0, section::BUILDING)));
    }

    #[test]
    fn section_groups_parse() {
        assert_eq!(section_group("section0_2"), Some((0, 2)));
        assert_eq!(section_group("section3_1"), Some((3, 1)));
        assert_eq!(section_group("build2"), None);
    }
}
