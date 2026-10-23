//! Voices of the animals (not in the original): growls, roars, hisses, bellows and calls
//! per species, in place of the soldiers' pain, agony and panic voices.
//!
//! The original's animal types (`ANM*.MD`, `NIKAKIXMAM`) have no `SOUND` lines at all, and
//! `SOUNDLIST.TXT` has no animal groups: animals fought and died silently. ACR's hit and pain
//! sounds ([`crate::hit_sounds`]) are men's voices and must never be heard from an animal.
//! An animal here growls when it strikes ([`Voice::attack`]), cries when wounded
//! ([`Voice::hurt`]) and roars or bellows when it dies ([`Voice::death`]).
//!
//! The samples are public-domain recordings (`assets/sounds/CREDITS.txt`), installed with
//! ACR's other sounds as `ACR\Sounds\<file>`; the jaguar snarls with the game's own unused
//! `Sound\new\Hepard1.wav` and `Hepard2.wav`.

/// A species' sound groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Voice {
    /// When it strikes.
    pub attack: &'static str,
    /// When it is wounded and lives.
    pub hurt: &'static str,
    /// When it dies.
    pub death: &'static str,
}

pub const BEAR_GROWL: &str = "ACR_BEAR_GROWL";
pub const BEAR_ROAR: &str = "ACR_BEAR_ROAR";
pub const BLACK_BEAR_GROWL: &str = "ACR_BLACK_BEAR_GROWL";
pub const BLACK_BEAR_ROAR: &str = "ACR_BLACK_BEAR_ROAR";
pub const GATOR_HISS: &str = "ACR_GATOR_HISS";
pub const GATOR_BELLOW: &str = "ACR_GATOR_BELLOW";
pub const BISON_GRUNT: &str = "ACR_BISON_GRUNT";
pub const BISON_BELLOW: &str = "ACR_BISON_BELLOW";
pub const ELK_BUGLE: &str = "ACR_ELK_BUGLE";
pub const HIND_CALL: &str = "ACR_HIND_CALL";
pub const JAGUAR_SNARL: &str = "ACR_JAGUAR_SNARL";

/// The sound groups and their files: below [`crate::hit_sounds::DIR`], or game paths
/// (with a backslash) as they are.
pub const GROUPS: &[(&str, &[&str])] = &[
    (BEAR_GROWL, &["bear_growl1.wav", "bear_growl2.wav", "bear_growl3.wav", "bear_growl4.wav"]),
    (BEAR_ROAR, &["bear_roar1.wav", "bear_roar2.wav"]),
    (
        BLACK_BEAR_GROWL,
        &["blackbear_growl1.wav", "blackbear_growl2.wav", "blackbear_growl3.wav", "blackbear_growl4.wav"],
    ),
    (BLACK_BEAR_ROAR, &["blackbear_roar1.wav", "blackbear_roar2.wav"]),
    (GATOR_HISS, &["gator_hiss1.wav", "gator_hiss2.wav", "gator_hiss3.wav"]),
    (GATOR_BELLOW, &["gator_bellow1.wav", "gator_bellow2.wav"]),
    (BISON_GRUNT, &["bison_grunt1.wav", "bison_grunt2.wav"]),
    (BISON_BELLOW, &["bison_bellow1.wav"]),
    (ELK_BUGLE, &["elk_bugle1.wav", "elk_bugle2.wav"]),
    (HIND_CALL, &["hind_call1.wav", "hind_call2.wav"]),
    (JAGUAR_SNARL, &[r"Sound\new\Hepard1.wav", r"Sound\new\Hepard2.wav"]),
];

const BEAR: Voice = Voice { attack: BEAR_GROWL, hurt: BEAR_GROWL, death: BEAR_ROAR };
const BLACK_BEAR: Voice = Voice { attack: BLACK_BEAR_GROWL, hurt: BLACK_BEAR_GROWL, death: BLACK_BEAR_ROAR };
const GATOR: Voice = Voice { attack: GATOR_HISS, hurt: GATOR_HISS, death: GATOR_BELLOW };
const BISON: Voice = Voice { attack: BISON_GRUNT, hurt: BISON_GRUNT, death: BISON_BELLOW };
const ELK: Voice = Voice { attack: ELK_BUGLE, hurt: ELK_BUGLE, death: ELK_BUGLE };
const HIND: Voice = Voice { attack: HIND_CALL, hurt: HIND_CALL, death: HIND_CALL };
const JAGUAR: Voice = Voice { attack: JAGUAR_SNARL, hurt: JAGUAR_SNARL, death: JAGUAR_SNARL };

/// The animal types of the original (`ANM*.MD` with `NIKAKIXMAM`) and their voices.
pub const SPECIES: &[(&str, Voice)] = &[
    ("ANMMED", BEAR),
    ("ANMMEDSM", BLACK_BEAR),
    ("ANMMED_SM", BLACK_BEAR),
    ("ANMCRO", GATOR),
    ("ANMCROSM", GATOR),
    ("ANMBIZ", BISON),
    ("ANMOLN", ELK),
    ("ANMOLNS", HIND),
    ("ANMJUG", JAGUAR),
];

/// The voice of the animal type `id` (any case, a catalog id's nation suffix such as
/// `AnmMed(NW)` ignored), `None` for other types.
pub fn voice(id: &str) -> Option<&'static Voice> {
    let id = id.split('(').next().unwrap_or(id).trim();
    SPECIES.iter().find(|(t, _)| t.eq_ignore_ascii_case(id)).map(|(_, v)| v)
}

/// Who cries out when hit: a man with the soldiers' voices, an animal with its own, or
/// nothing (an animal type without a voice).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cry {
    Human,
    Animal(&'static Voice),
    Silent,
}

impl Cry {
    /// A unit of type `id`; `animal` for `NIKAKIXMAM` types. The `ANM*` types without
    /// that flag (the eagle) are animals too.
    pub fn of(id: &str, animal: bool) -> Self {
        let animal = animal || id.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("ANM"));
        match (animal, voice(id)) {
            (false, _) => Cry::Human,
            (true, Some(v)) => Cry::Animal(v),
            (true, None) => Cry::Silent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every group the soldiers' voices use: pain, agony and panic.
    fn human_files() -> Vec<String> {
        let human =
            [crate::hit_sounds::PAIN, crate::agony::SCREAM, crate::agony::MOAN, crate::agony::GASP, crate::agony::FEAR];
        human.iter().flat_map(|g| crate::hit_sounds::files(g).unwrap()).collect()
    }

    #[test]
    fn no_animal_maps_to_a_human_voice() {
        let human = human_files();
        let human_groups =
            [crate::hit_sounds::PAIN, crate::agony::SCREAM, crate::agony::MOAN, crate::agony::GASP, crate::agony::FEAR];
        for (id, v) in SPECIES {
            assert_eq!(Cry::of(id, true), Cry::Animal(v));
            for group in [v.attack, v.hurt, v.death] {
                assert!(!human_groups.contains(&group), "{id} uses the human group {group}");
                assert!(GROUPS.iter().any(|(g, _)| *g == group), "{id}: {group} is not an animal group");
                let files = crate::hit_sounds::files(group).unwrap();
                assert!(!files.is_empty());
                for f in files {
                    assert!(!human.contains(&f), "{id}: {f} is a human voice");
                    let name = f.to_ascii_lowercase();
                    for word in ["pain", "scream", "moan", "gasp", "fear", "yell"] {
                        assert!(!name.contains(word), "{id}: {f}");
                    }
                }
            }
        }
        // The types of the original's animals, in any case; men keep their voices.
        assert_eq!(voice("AnmMed(NW)"), voice("ANMMED"));
        for id in ["AnmMed", "AnmMedSm", "AnmBiz", "AnmCro", "AnmCroSm", "AnmJug", "AnmOln", "AnmOlnS"] {
            assert!(matches!(Cry::of(id, true), Cry::Animal(_)), "{id}");
        }
        assert_eq!(Cry::of("AnmUnknown", true), Cry::Silent);
        assert_eq!(Cry::of("AnmEgl", false), Cry::Silent);
        assert_eq!(Cry::of("FraMush", false), Cry::Human);
    }

    #[test]
    fn the_black_bear_has_its_own_voice() {
        assert_ne!(voice("AnmMedSm"), voice("AnmMed"));
    }

    #[test]
    fn every_shipped_file_parses() {
        let shipped = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/sounds");
        for (_, names) in GROUPS {
            for name in names.iter().filter(|n| !n.contains('\\')) {
                let bytes = std::fs::read(shipped.join(name)).unwrap();
                acr_audio::Sound::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            }
        }
    }
}
