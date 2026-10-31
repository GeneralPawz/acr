//! The editor's "Kampf" rule (docs/controls.md, "Editor"): who fights whom. `AUS` allies every
//! player with every other, `AN` is every player for himself, `Teams` allies the owners of the
//! same team number (1..=4, picked per colour with a right click on its swatch). The animals'
//! neutral player ([`NEUTRAL`]) is hostile to everyone unless everything is allied.
//! Alliances are set as the mission scripts set them (`sync_alliances`): the nations' masks and
//! every object's `nmask`.

use acr_sim::World;

use super::NEUTRAL;

/// The teams a swatch cycles through.
pub const TEAMS: u8 = 4;

/// How the players are allied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Everybody is everybody's ally, nobody attacks (the start).
    Off,
    /// Everybody against everybody.
    Free,
    /// Owners of the same team are allied.
    Teams,
}

impl Mode {
    /// The mode's number in a savegame.
    pub fn code(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::Free => 1,
            Self::Teams => 2,
        }
    }

    pub fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            0 => Self::Off,
            1 => Self::Free,
            2 => Self::Teams,
            _ => return None,
        })
    }

    /// The next mode of the "Kampf" line: AUS, AN, Teams, AUS ...
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Free,
            Self::Free => Self::Teams,
            Self::Teams => Self::Off,
        }
    }
}

/// The team numbers (1..=[`TEAMS`]) of the eight owners; the neutral one has none.
pub fn default_teams() -> [u8; 8] {
    std::array::from_fn(|p| p as u8 % TEAMS + 1)
}

/// The next team number of a swatch, 1 after [`TEAMS`].
pub fn next_team(team: u8) -> u8 {
    team % TEAMS + 1
}

/// The alliance mask of each owner: bit `q` set when `q` is his ally (himself included).
pub fn masks(mode: Mode, team: &[u8; 8]) -> [u8; 8] {
    std::array::from_fn(|p| match mode {
        Mode::Off => 0xFF,
        Mode::Free => 1 << p,
        Mode::Teams => (0..8)
            .filter(|&q| q == p || (p != NEUTRAL as usize && q != NEUTRAL as usize && team[q] == team[p]))
            .fold(0, |m, q| m | 1 << q),
    })
}

/// Sets the nations' masks and every object's.
pub fn apply(world: &mut World, mode: Mode, team: &[u8; 8]) {
    world.nation_mask = masks(mode, team);
    let masks = world.nation_mask;
    for u in &mut world.units {
        u.nmask = masks[usize::from(u.owner) & 7];
    }
}

/// The next owner after `local` (wrapping) that has a living unit, or `local` when no other has.
pub fn next_controlled(world: &World, local: u8) -> u8 {
    let has = |p: u8| world.units.iter().any(|u| u.alive && u.owner == p);
    (1..8).map(|d| (local + d) & 7).find(|&p| has(p)).unwrap_or(local)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_allies_everybody() {
        assert_eq!(masks(Mode::Off, &default_teams()), [0xFF; 8]);
    }

    #[test]
    fn free_for_all_allies_only_oneself() {
        let m = masks(Mode::Free, &default_teams());
        assert_eq!(m, [1, 2, 4, 8, 16, 32, 64, 128]);
    }

    #[test]
    fn teams_of_two_and_a_lone_player() {
        // Players 1+2 (team 1), 3+4 (team 2), 5 alone (team 3); the neutral one alone.
        let team = [1, 1, 2, 2, 3, 4, 4, 1];
        let m = masks(Mode::Teams, &team);
        assert_eq!(m[0], 0b11);
        assert_eq!(m[1], 0b11);
        assert_eq!(m[2], 0b1100);
        assert_eq!(m[3], 0b1100);
        assert_eq!(m[4], 0b1_0000);
        assert_eq!(m[5], 0b110_0000);
        // The neutral player shares team 1 on paper, yet stays alone and nobody allies it.
        assert_eq!(m[7], 0x80);
        assert!(m[..7].iter().all(|&x| x & 0x80 == 0));
    }

    #[test]
    fn alliances_are_symmetric() {
        let m = masks(Mode::Teams, &default_teams());
        for p in 0..8 {
            for q in 0..8 {
                assert_eq!(m[p] >> q & 1, m[q] >> p & 1);
            }
        }
    }

    #[test]
    fn the_teams_and_modes_cycle() {
        assert_eq!([1, 2, 3, 4].map(next_team), [2, 3, 4, 1]);
        assert_eq!(Mode::Off.next().next().next(), Mode::Off);
    }
}
