//! Stub implementation of the game API (`dmcr.exe` and `DipServer.dll` exports) that
//! records every call.
//!
//! All of these functions are `cdecl` (caller cleans the stack, confirmed by the
//! `add esp, n` after every call site and by the `YA` calling convention code in the
//! mangled C++ names). [`ARG_COUNTS`] lists the number of 32-bit arguments of each
//! function as inferred from the stack cleanup at its call sites across all 73 DLLs; the
//! real signatures still have to be reverse-engineered for the API implementation.

use crate::runtime::{Host, HostCall, Result};

/// `(function, 32-bit argument count)` inferred from call sites, sorted by name.
/// Entries marked "ambiguous" had inconsistent cleanups (variadic functions or cleanups
/// merged by the optimiser); the most frequent value is listed.
pub const ARG_COUNTS: &[(&str, u8)] = &[
    ("?AddCannon@@YAXPAUGAMEOBJ@@E@Z", 2),
    ("?AddFirers@@YAXPAUGAMEOBJ@@E@Z", 2),
    ("?AddPsKillers@@YAXPAUGAMEOBJ@@E_N@Z", 3),
    ("?AddStorm@@YAXPAUGAMEOBJ@@EH@Z", 3),
    ("?AddTomahawks@@YAXPAUGAMEOBJ@@EGHH@Z", 5),
    ("?AssignHint1@@YAXPADH@Z", 2),
    ("?AssignHint@@YAXPADH@Z", 5), // ambiguous: add20:3, add8:1
    ("?GetTextByID@@YAPADPAD@Z", 1),
    ("?SetAIForDiplo@@YA_NEHH@Z", 3),
    ("?_ExFree@@YAXPAX@Z", 1),
    ("?_ExMalloc@@YAPAXH@Z", 1),
    ("?_ExRealloc@@YAPAXPAXH@Z", 2),
    ("AI_Torg", 3),
    ("AddOneUnitToGroup", 2),
    ("AddResource", 3),
    ("AddUnitToSel", 1),
    ("AllowAttack", 2),
    ("AskMultilineQuestion", 4), // ambiguous: add16:2, add60:1
    ("AskQuestion", 1),
    ("AskQuestionParam", 2),
    ("AssignDesantDestGroup", 2),
    ("AssignDesantStartGroup", 2),
    ("Attack3", 4), // ambiguous: add16:18, add24:4, add36:2
    ("Attack6", 4),
    ("AttackBuildingsInZone", 3),
    ("AttackByTomahawks", 4),
    ("AttackEnemyInZone", 9), // ambiguous: add36:1, none:1
    ("AttackEnemyInZone2", 3),
    ("AttackGrpByGrp", 2),
    ("CancelAllPurposes", 1),
    ("ChangeFriends", 2),
    ("CheckBuildingsComplete", 1),
    ("CheckDef", 4),
    ("CheckLeaveAbility", 1), // ambiguous: add24:1, add4:1
    ("CheckProduction", 1),
    ("CleanGroup", 1),
    ("ClearLightSpot", 1),
    ("ClearSelection", 1),
    ("CollectUnits", 4),
    ("CopyUnits", 6),
    ("CreateDangerMap", 5), // ambiguous: add20:29, none:8
    ("CreateDangerMapForDef", 5),
    ("CreateDangerMapForTom", 5),
    ("CreateFarmList", 2),
    ("CreateFields", 1),
    ("CreateFormation", 7),
    ("CreateFriendBuildingsTopList", 2),
    ("CreateMeatList", 3),
    ("CreateObject0", 6),
    ("CreateTopListEnArmy", 3),
    ("DeleteHiddenUnits", 1),
    ("DeleteUnitFromGroup", 2),
    ("DieUnit", 1),
    ("DisableMission", 1),
    ("DisableUpgrade", 2),
    ("EnableMission", 1),
    ("EnableUnit", 3),
    ("EnumEnemyArmies", 0), // ambiguous: none:2
    ("FilterUnitsByCategory", 3),
    ("FindNextZoneOnTheSafeWay", 5),
    ("FindNextZoneOnTheSafeWayToObject", 6),
    ("FreeTimer", 1),
    ("GetAINation", 0),
    ("GetAmountOfWarriors", 1),
    ("GetBuildingEntryPoint", 4),
    ("GetCurrentUnits", 1),
    ("GetDiff", 1),
    ("GetDifficulty", 0),
    ("GetEnemyForce", 2),
    ("GetExtraction", 1),
    ("GetFogSpotVisibility", 2),
    ("GetGlobalTime", 0),
    ("GetGroupForce", 1),
    ("GetGrpCenter", 3),
    ("GetInsideAmount", 1),
    ("GetInsideList", 2),
    ("GetLandType", 0),
    ("GetMaxDamage", 1),
    ("GetMaxInsideAmount", 1),
    ("GetMaxUnits", 1),
    ("GetMinDamage", 1),
    ("GetMonUsage", 1),
    ("GetMoney", 1),
    ("GetMyNation", 0),
    ("GetNInside", 1), // ambiguous: add24:1, add4:2
    ("GetNUnits", 1),
    ("GetNZones", 0),
    ("GetOBJ", 1),
    ("GetOrderType", 1),
    ("GetPeaceTimeLeft", 0),
    ("GetRND", 1),
    ("GetReadyUnits", 1),
    ("GetResource", 2),
    ("GetSearchRadius", 1),
    ("GetTargetObject", 3),
    ("GetTopDist", 4),
    ("GetTopDst", 2),
    ("GetTopZRealCoor", 3),
    ("GetTopZone", 2),
    ("GetTopZoneCoor", 3), // ambiguous: add12:12, none:13
    ("GetTorgResult", 3),
    ("GetTotalAmount0", 1),
    ("GetTotalAmount1", 2),
    ("GetUnitCost", 3),
    ("GetUnitExCaps", 3),
    ("GetUnitGlobalInfo", 2),
    ("GetUnitInfo", 3),
    ("GetUnitInfo2", 2), // ambiguous: add8:3, none:15
    ("GetUnitNMASK", 1),
    ("GetUnitStorm", 1),
    ("GetUnits", 1),
    ("GetUnitsAmount0", 2),
    ("GetUnitsAmount1", 2),
    ("GetUnitsAmount2", 3),
    ("GetUnitsByNation", 2),
    ("GetUnitsTypeInGroup", 2),
    ("GetUpgradeCost", 3),
    ("GetZoneCoor", 3),
    ("GetZonesDist", 2),
    ("GrpAttackObject", 3),
    ("InitialUpgrade", 2),
    ("InitialUpgradeNew", 3),
    ("InsertUnitToGroup", 3),
    ("InviteUnitIntoBuilding", 3), // ambiguous: add12:52, add24:11, add32:2
    ("IsResourceEnabled", 2),
    ("LeaveBuilding", 4), // ambiguous: add16:4, add8:1
    ("LooseGame", 0),
    ("MakeEnemyAgrTopGrp", 3),
    ("MakeOneShotToBuild", 3),
    ("MissErrorMessage", 2),
    ("NationIsErased", 1),
    ("OBJ_AttackObj", 4),
    ("OBJ_LeaveMine", 2),
    ("OBJ_SendTo", 5),
    ("Patrol", 3),
    ("ProduceUnit", 3),
    ("ProduceUnitFast", 4),
    ("PushAllUnitsAway", 1),
    ("RandNew", 2),
    ("RegisterDynGroup", 1),
    ("RegisterDynGroupEx", 3),
    ("RegisterFormation", 2),
    ("RegisterUnitType", 2),
    ("RegisterUnits", 2),
    ("RegisterUpgrade", 2),
    ("RegisterVar", 2),
    ("RegisterVisibleZone", 2),
    ("RegisterZone", 2),
    ("RemoveGroup", 2),
    ("RemoveNUnits", 4),
    ("RemoveNearestNotBusyUnit", 5),
    ("RemoveNearestUnit", 5),
    ("RemovePanicUnits", 2), // ambiguous: add8:8, none:3
    ("RemoveUnitFromGroup", 2),
    ("RemoveUpperGroundUnits", 2),
    ("RunTimer", 2),
    ("SGP_AttackByArchers", 4),
    ("SGP_ComeIntoBuilding", 3),
    ("SGP_MakeOneStepTo", 6),
    ("SGP_MoveToPoint", 8), // ambiguous: add32:16, add44:1, none:6
    ("SGP_RepairBuilding", 3),
    ("SGP_TakeResources", 4),
    ("SaveSelectedUnits", 3),
    ("SearchCapturedTopToDeffend", 2),
    ("SelAttackGroup", 2),
    ("SelCenter", 0), // ambiguous: none:2
    ("SelChangeNation", 2),
    ("SelDie", 1),
    ("SelErase", 1),
    ("SelSendAndKill", 4),
    ("SelSendTo", 4),
    ("SelectTypeOfUnitsInZone", 4),
    ("SelectUnits", 2),
    ("SelectUnitsInZone", 3), // ambiguous: add12:22, none:6
    ("SelectUnitsType", 3),
    ("SendUnitsToTransport", 1),
    ("SetBuildingsCollector", 2),
    ("SetDestPoint", 2),
    ("SetEnemyBuildingsCollector", 2), // ambiguous: add36:1, add56:1, add8:2, none:1
    ("SetLightSpot", 3),
    ("SetMaxInTransport", 2),
    ("SetNMASK", 2), // ambiguous: add8:1, none:1
    ("SetPlayerName", 2),
    ("SetResource", 3),
    ("SetStandGround", 2),
    ("SetStartPoint", 1),
    ("SetToInternalResourcesZero", 1),
    ("SetTrigg", 2),
    ("SetUnitEnableState", 3),
    ("SetUnitInfo", 1),
    ("SetUnitsCollector", 2),
    ("SetUnitsState", 5), // ambiguous: add20:1, add32:1
    ("SetUpgradeEnableStatus", 3),
    ("ShowCentralText", 2),
    ("ShowPage", 1),
    ("ShowPageParam", 2), // ambiguous: add12:28, add16:5, add8:40, none:1
    ("ShowVictory", 0),
    ("StartAI", 6),
    ("TakeFood", 1),
    ("TakeWood", 1),
    ("TimerDone", 1),
    ("TimerDoneFirst", 1),
    ("Trigg", 1),
    ("TryUnit", 4),
    ("TryUpgrade", 3),
    ("TryUpgradeEx", 5),
    ("UnitsCenter", 3),
    ("UpgIsDone", 1),
    ("UpgIsRun", 1),
];

/// Argument count of a game API function, if known.
pub fn arg_count(name: &str) -> Option<u8> {
    ARG_COUNTS.binary_search_by(|(n, _)| n.cmp(&name)).ok().map(|i| ARG_COUNTS[i].1)
}

/// One recorded call into the game API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiCall {
    pub name: String,
    pub args: Vec<u32>,
    /// Arguments that point to a printable NUL-terminated string, by index.
    pub strings: Vec<(usize, String)>,
    pub ret: u32,
}

impl std::fmt::Display for ApiCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}(", self.name)?;
        for (i, a) in self.args.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            match self.strings.iter().find(|(j, _)| *j == i) {
                Some((_, s)) => write!(f, "{s:?}")?,
                None if (*a as i32).unsigned_abs() < 0x10000 => write!(f, "{}", *a as i32)?,
                None => write!(f, "{a:#x}")?,
            }
        }
        write!(f, ") -> {}", self.ret as i32)
    }
}

/// A [`Host`] that answers every game API call with 0 and records it.
///
/// Memory functions (`_ExMalloc`, `_ExRealloc`, `_ExFree`) are implemented on the guest
/// heap, and `GetTextByID` returns its argument, so code that uses their results keeps
/// working. Functions in `results` return the given value instead of 0.
#[derive(Debug, Default)]
pub struct RecordingHost {
    pub calls: Vec<ApiCall>,
    pub results: Vec<(String, u32)>,
}

/// A printable string of 1..=200 bytes at `addr`, if there is one.
fn guess_string(call: &HostCall, addr: u32) -> Option<String> {
    if addr < 0x1_0000 {
        return None;
    }
    let bytes = call.mem.read_cstr(addr, 201).ok()?;
    let printable = |b: &u8| b.is_ascii_graphic() || *b == b' ' || *b >= 0xA0;
    (!bytes.is_empty() && bytes.len() <= 200 && bytes.iter().all(printable))
        .then(|| bytes.iter().map(|&b| char::from(b)).collect())
}

impl Host for RecordingHost {
    fn call(&mut self, call: &mut HostCall<'_>) -> Result<u32> {
        let n = arg_count(call.name).unwrap_or(4);
        let args = (0..u32::from(n)).map(|i| call.arg(i)).collect::<Result<Vec<_>>>()?;
        let strings = args.iter().enumerate().filter_map(|(i, &a)| guess_string(call, a).map(|s| (i, s))).collect();
        let ret = match call.name {
            "?_ExMalloc@@YAPAXH@Z" => call.heap.alloc(call.mem, args[0], true).unwrap_or(0),
            "?_ExRealloc@@YAPAXPAXH@Z" if args[0] == 0 => call.heap.alloc(call.mem, args[1], true).unwrap_or(0),
            "?_ExRealloc@@YAPAXPAXH@Z" => call.heap.realloc(call.mem, args[0], args[1], true).unwrap_or(0),
            "?_ExFree@@YAXPAX@Z" => {
                call.heap.free(args[0]);
                0
            }
            "?GetTextByID@@YAPADPAD@Z" => args[0],
            name => self.results.iter().find(|(n, _)| n == name).map_or(0, |&(_, v)| v),
        };
        self.calls.push(ApiCall { name: call.name.to_owned(), args, strings, ret });
        Ok(ret)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_and_unique() {
        assert!(ARG_COUNTS.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(arg_count("CreateObject0"), Some(6));
        assert_eq!(arg_count("GetGlobalTime"), Some(0));
        assert_eq!(arg_count("nonexistent"), None);
    }
}
