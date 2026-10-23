//! The game's wiring of the rural settlements: model slot fits, and the
//! abandoned wagons laid on the shipped world.

use super::*;
use pampa_core::StructureKind;

#[test]
fn the_shipped_slot_fits_reach_the_renderer() {
    let (slots, warning) = model_slots(&asset_path(SLOTS_FILE));
    assert!(warning.is_none(), "{warning:?}");
    let pump = &slots.0["windpump"];
    let rotor = pump.rotor.as_ref().expect("the windpump's wheel spins");
    assert_eq!(rotor.node, "rotor");
    assert!((pump.scale - 0.875).abs() < 1e-6);
    assert!(slots.0.contains_key("wrecked_wagon"));
    for slot in slots.0.keys() {
        assert!(StructureKind::SLOTS.contains(&slot.as_str()), "{slot}");
    }
    // A broken file fits nothing and says so; a missing one fits nothing.
    let dir = std::env::temp_dir().join(format!("pampa-seca-slots-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let broken = dir.join("slots.ron");
    std::fs::write(&broken, r#"(slots: { "carreta": () })"#).unwrap();
    let (slots, warning) = model_slots(&broken);
    assert!(slots.0.is_empty() && warning.is_some());
    let (slots, warning) = model_slots(&dir.join("missing.ron"));
    assert!(slots.0.is_empty() && warning.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_shipped_world_has_a_few_wrecks_off_its_roads_and_tracks() {
    let loaded = towns::load_world(&asset_path(TOWNS_FILE));
    let laid = lay_out(&loaded);
    let layout = &laid.settlements;
    let roads = laid.world.terrain.roads.as_deref().expect("roads laid");
    let yards = layout
        .settlements
        .iter()
        .flat_map(|s| s.of(StructureKind::WreckedWagon))
        .count();
    println!(
        "wrecks: {yards} in yards, {} on verges",
        layout.roadside.len()
    );
    assert!(layout.roadside.len() <= pampa_core::SettlementLayout::ROADSIDE_WRECKS);
    assert!(yards + layout.roadside.len() >= 1, "no wreck anywhere");
    for wreck in &layout.roadside {
        for c in wreck.corners() {
            assert!(
                roads.distance(c) > pampa_core::RoadNetwork::HALF_WIDTH,
                "{wreck:?}"
            );
        }
        for o in wreck.obstacles() {
            assert!(laid.world.obstacles.contains(&o));
        }
    }
    // No wild tree grows on a wreck.
    for wreck in &layout.roadside {
        let chunk = laid.world.config.chunk_of(wreck.at);
        for t in laid.flora.trees(chunk) {
            assert!(
                !wreck.footprint().expanded(0.5).contains_point(t.at),
                "{t:?}"
            );
        }
    }
}
