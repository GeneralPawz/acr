//! Grouped formations (`CM_DOGRP` / `CM_UNGRP`): a player ties formations together, so that
//! selecting one of them selects all (`CorrectImSelectionInGroups` 0x43dbff, in the
//! interface). The groups live in the player's city (`GroupsSet` City+0xf63, `NGroupsInSet`
//! +0xf67, `NGroups` +0xf6b) and change only through the network commands 0x58
//! (`GroupSelectedFormations` 0x43d91d) and 0x59 (`UnGroupSelectedUnits` 0x43d717).
//!
//! The original keeps the brigade ids of a group and their count in two arrays.
//! `DeleteFromGroups` (0x43d592) shifts the id arrays when a group becomes empty but not the
//! counts, so every later group gets the count of the group before it. That is reproduced:
//! reads are cut at the end of a group's ids (the engine reads the heap behind them).

use crate::command::PlayerId;
use crate::world::{EntityId, World};

/// The formation groups of one player.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct FormationGroups {
    /// `GroupsSet`: the brigade ids of each group, as allocated.
    pub sets: Vec<Vec<u16>>,
    /// `NGroupsInSet`: the count of each group (see the module comment).
    pub counts: Vec<u16>,
}

impl FormationGroups {
    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }

    /// The ids of group `i` as the engine reads them: its first `NGroupsInSet[i]` ids.
    pub fn group(&self, i: usize) -> &[u16] {
        let set = &self.sets[i];
        &set[..usize::from(self.counts[i]).min(set.len())]
    }

    /// `DeleteFromGroups` (0x43d592): removes brigade `id` from the groups. A group that
    /// becomes empty is freed and the following groups move down; the counts do not, and
    /// the search stops there.
    pub fn delete(&mut self, id: u16) {
        let mut i = 0;
        while i < self.sets.len() {
            let n = usize::from(self.counts[i]).min(self.sets[i].len());
            if let Some(j) = self.sets[i][..n].iter().position(|&b| b == id) {
                self.sets[i].remove(j);
                self.counts[i] -= 1;
                if self.counts[i] == 0 {
                    self.sets.remove(i);
                    self.counts.truncate(self.sets.len());
                    return;
                }
            }
            i += 1;
        }
    }

    /// `CheckUnGroupPossibility` (0x43e0b7): some group consists of exactly these brigades.
    pub fn is_group(&self, ids: &[u16]) -> bool {
        (0..self.sets.len())
            .any(|i| usize::from(self.counts[i]) == ids.len() && ids.iter().all(|id| self.group(i).contains(id)))
    }

    /// The groups' members of the brigades `ids`, added after them (`CorrectImSelectionInGroups`
    /// 0x43dbff): for each id, every group that holds it contributes its ids.
    pub fn expand(&self, ids: &[u16]) -> Vec<u16> {
        let mut out = ids.to_vec();
        for &id in ids {
            for i in 0..self.sets.len() {
                let g = self.group(i);
                if g.contains(&id) {
                    for &b in g {
                        if !out.contains(&b) && out.len() < 256 {
                            out.push(b);
                        }
                    }
                }
            }
        }
        out
    }
}

/// The distinct brigades of the units (`FormsIDS`, at most 256), in the order the units
/// list them: units that are alive and not hidden (`!Sdoxlo`) and in a formation. The
/// original goes through the player's synchronised selection (`Selm`); here the command
/// carries it.
pub fn brigades_of(world: &World, player: PlayerId, units: &[EntityId]) -> Vec<u16> {
    let mut ids: Vec<u16> = Vec::new();
    for id in units {
        let Some(u) = world.units.get(id.0 as usize) else { continue };
        if u.owner != player || !u.active() {
            continue;
        }
        if let Some(b) = u.brigade
            && !ids.contains(&b)
            && ids.len() < 256
        {
            ids.push(b);
        }
    }
    ids
}

/// `GroupSelectedFormations` (0x43d91d): the formations of the units leave their groups and
/// form a new one.
pub fn group(world: &mut World, player: PlayerId, units: &[EntityId]) {
    let ids = brigades_of(world, player, units);
    let groups = &mut world.formation_groups[usize::from(player)];
    for &id in &ids {
        groups.delete(id);
    }
    if !ids.is_empty() {
        groups.counts.truncate(groups.sets.len());
        groups.counts.push(ids.len() as u16);
        groups.sets.push(ids);
    }
}

/// `UnGroupSelectedUnits` (0x43d717): the formations of the units leave their groups.
pub fn ungroup(world: &mut World, player: PlayerId, units: &[EntityId]) {
    let ids = brigades_of(world, player, units);
    let groups = &mut world.formation_groups[usize::from(player)];
    for &id in &ids {
        groups.delete(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn groups(sets: &[&[u16]]) -> FormationGroups {
        FormationGroups {
            sets: sets.iter().map(|s| s.to_vec()).collect(),
            counts: sets.iter().map(|s| s.len() as u16).collect(),
        }
    }

    #[test]
    fn delete_keeps_the_counts_in_place() {
        let mut g = groups(&[&[1, 2], &[3], &[4, 5, 6], &[7]]);
        g.delete(2);
        assert_eq!(g.group(0), &[1]);
        // Group [3] empties: the later groups move down, their counts do not.
        g.delete(3);
        assert_eq!(g.sets, vec![vec![1], vec![4, 5, 6], vec![7]]);
        assert_eq!(g.counts, vec![1, 0, 3]);
        assert_eq!(g.group(1), &[] as &[u16]);
        assert_eq!(g.group(2), &[7]);
        // Brigade 4 cannot be found any more (its group reads as empty).
        g.delete(4);
        assert_eq!(g.sets[1], vec![4, 5, 6]);
    }

    #[test]
    fn group_check_and_expansion() {
        let g = groups(&[&[1, 2], &[3, 4, 5]]);
        assert!(g.is_group(&[2, 1]));
        assert!(!g.is_group(&[1]));
        assert!(!g.is_group(&[1, 3]));
        assert_eq!(g.expand(&[4]), vec![4, 3, 5]);
        assert_eq!(g.expand(&[1, 9]), vec![1, 9, 2]);
    }
}
