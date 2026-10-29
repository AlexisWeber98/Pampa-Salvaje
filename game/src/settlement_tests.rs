//! The game's wiring of the rural settlements: model slot fits.

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
    let dir = std::env::temp_dir().join(format!("pampa-salvaje-slots-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let broken = dir.join("slots.ron");
    std::fs::write(&broken, r#"(slots: { "carreta": () })"#).unwrap();
    let (slots, warning) = model_slots(&broken);
    assert!(slots.0.is_empty() && warning.is_some());
    let (slots, warning) = model_slots(&dir.join("missing.ron"));
    assert!(slots.0.is_empty() && warning.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
