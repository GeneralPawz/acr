//! The engine part of the city tick: gathering speeds for `GetExtraction`.

use std::sync::Arc;

use acr_sim::World;
use acr_sim::catalog::Catalog;
use acr_sim::formation::Formations;
use acr_sim::map::MapGrid;

#[test]
fn gathering_speed_is_amount_times_25_over_global_time() {
    let mut w = World::new(Arc::new(MapGrid::open(1)), Arc::new(Catalog::new()), Arc::new(Formations::default()));
    w.players[2].gathered = [100, 0, 50];
    // Nation 2 has its city tick on tmtmt % 8 == 2; GlobalTime runs 2 per tick, so the first
    // update after 1000 comes at tmtmt 506 (GlobalTime 1012).
    for _ in 0..505 {
        w.step(&[]);
    }
    assert_eq!(w.players[2].gather_speed, [0, 0, 0]);
    w.step(&[]);
    assert_eq!(w.players[2].gather_speed, [100 * 25 / 1012, 0, 50 * 25 / 1012]);
    assert_eq!(w.players[2].gathered, [0, 0, 0]);
    assert_eq!(acr_sim::economy::extraction(&w, 2, acr_sim::economy::WOOD), 2);
}
