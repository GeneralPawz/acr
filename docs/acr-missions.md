# ACR's own missions (#417)

The original's single missions are data: an `.m3d` map, a script DLL and a text file named in
`Missions\Missions.txt` (docs/script-runtime.md). ACR must never ship them, and the
installation's lists cannot be changed. ACR's own missions are therefore code, listed in the
single missions screen (docs/ui.md "Single missions") **after** the original's rows, which
stay as they are.

## Where they live

`crates/acr-game/src/acr_missions.rs`: `AcrMission` is the list (`ALL`, in menu order), and
each mission supplies

| | |
| --- | --- |
| `id` | the identifier a savegame keeps (`GameKind::Acr`, savegame tag 3) |
| `name`, `briefing` | the list row and the text viewer (CP1252, German like the game's texts) |
| `players` | the `GameSetup` players: human first (owner 0), then the computer's |
| `map` | the `M3dMap`, made in code: terrain, `SEA2` water, the units |
| `verdict` | `Some(true)` won, `Some(false)` lost, `None` while it goes on |
| `orders` | the computer players' commands for this tick |

`MissionLink` (`mission.rs`) calls `verdict` and `orders` before every tick, in the place
where the scripts of the original's missions run; there is no script, no AI DLL and no
`Missions.txt`. Both are functions of the world and its tick only (no state of their own), so
a savegame replays them exactly. The screen shows the win or lose text of the original
(`WINNER`, `LOOSER`) and returns to the list with the mission still selected. The preview
picture is empty (the original's previews are files `Maps2\single<n>.bmp`).

## Adding one

1. A variant of `AcrMission`, listed in `ALL`, with its arms in every method.
2. A map function. Ships float where `SEA2` is above 130, land units stop above 128
   (docs/water.md); `shore()` in the Lake Erie map shows how land is made from a signed
   distance (heights, textures and depth from the same function). Unit type names are the
   catalog's (`ShpFregat(US)`); `acr_mapgen`'s sea map lists which ships each nation has
   (docs/mapgen.md "Sea battle").
3. Tests: unit tests for the map and setup (`acr_missions::tests`), and an e2e scenario that
   starts it from the menu (`tests/e2e/sea_mission.json`).

The difficulty combo of the screen does not affect them (yet).

## Seeschlacht auf dem Eriesee (Battle of Lake Erie, 10 September 1813)

The player commands the American squadron under Perry, the computer the British under
Barclay. The map is the western basin of Lake Erie, north up: the Ohio shore in the south,
Pelee Island and the Canadian shore in the north, the Fort Malden headland in the north-west
and the Bass Islands (South Bass with Put-in-Bay, Gibraltar, Middle Bass, North Bass,
Rattlesnake) in the south-east. The Americans start off Put-in-Bay facing north-west, the
British come from the north-west, as at dawn on 10 September (the NPS account: the lookout
sights six vessels north-west of Put-in-Bay beyond Rattlesnake Island).

Ships (the game has the frigate and the caravel for the European nations, no tier between;
`ShpParom` and `ShpLodka` carry no guns):

| Historical | Game type |
| --- | --- |
| USS Lawrence, USS Niagara (20-gun brigs) | `ShpFregat(US)` |
| Caledonia, Scorpion, Ariel, Somers | `ShpPinta(US)` |
| HMS Detroit, HMS Queen Charlotte | `ShpFregat(EN)` |
| Lady Prevost, General Hunter, Chippewa | `ShpPinta(EN)` |

Left out for playability: the American Porcupine, Tigress and Trippe and the British Little
Belt (9 against 6 historically; 6 against 5 here). The units are not named in the game.

- **Win**: every British ship is sunk. **Lose**: every American ship is sunk.
- **Not built**: a surrender ("struck colours", as Lawrence, Detroit and Queen Charlotte did).
  ACR has no such rule, so none here; it could come later as an optional rule, off by default.
- **The computer**: at tick 0 and every 200 ticks its idle ships attack-move (`Command::AttackMove`)
  to the middle of the American fleet; ships already moving or fighting keep their orders.
- Weather and the wind (decisive in the real battle) are not part of it.

Sources: Wikipedia, "Battle of Lake Erie" (the course, the squadrons, the first shot at
11:45, both flagships struck, Detroit and Queen Charlotte struck at about 3 pm); National
Park Service, Perry's Victory and International Peace Memorial, "The Battle of Lake Erie"
(<https://www.nps.gov/pevi/learn/historyculture/battle_erie_detail.htm>: the nine American
and six British vessels, their types and guns, the British long guns against the American
carronades). The Naval History and Heritage Command's page could not be reached when this
was written.
