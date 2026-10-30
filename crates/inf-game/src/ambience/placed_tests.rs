use super::*;
use crate::terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing};

/// A round lake 40 grid points across in the middle of a flat world, its
/// floor below the plane.
fn lake() -> TerrainRuntime {
    let cell = |index: usize| {
        let (x, z) = ((index / 512) as f32, (index % 512) as f32);
        let deep = (x - 256.0).hypot(z - 256.0) < 20.0;
        u16::from(if deep { -20_i8 as u8 } else { 10 })
    };
    TerrainRuntime::from_raw_cells(
        TerrainSampling {
            spacing: TerrainSpacing::Units128,
            height_scale: TerrainHeightScale::Units32,
        },
        0,
        (0..512 * 512).map(cell),
    )
}

/// The shores ring the lake, at the plane, and nowhere else.
#[test]
fn shores_ring_a_lake_at_its_plane() {
    let found = shores(&lake(), 0);
    assert!(!found.is_empty());
    for [x, y, z] in found {
        let from_middle = (x - 256.0 * 128.0).hypot(z - 256.0 * 128.0);
        assert!(from_middle < 22.0 * 128.0, "{x}, {z}");
        assert_eq!(y, 0.0);
    }
    assert!(
        shores(&lake(), -1_000).is_empty(),
        "a plane under the floor"
    );
}

/// A ridge between the camera and a sound hides it; open ground does not.
/// From the lake's floor, its rim walls the camera in; above it, nothing
/// does.
#[test]
fn ridges_hide_sounds_and_walls_close_in() {
    let terrain = lake();
    let middle = 256 * 128;
    // On the floor (-640) by the middle, a sound beyond the rim (320 high).
    let floor = [middle, -600, middle];
    assert!(occluded(&terrain, floor, [4_000.0, 600.0, 0.0]));
    // From high above the rim, nothing stands between.
    let above = [middle, 2_000, middle];
    assert!(!occluded(&terrain, above, [4_000.0, -1_600.0, 0.0]));
    assert_eq!(enclosure(&terrain, floor), 0.0, "the rim is farther out");
    let near_rim = [middle + 2_200, -600, middle];
    assert!(enclosure(&terrain, near_rim) > 0.0);
    assert_eq!(enclosure(&terrain, above), 0.0);
}

/// The ground wraps: a place just past the edge is near.
#[test]
fn offsets_wrap_across_the_edge() {
    assert_eq!(
        offset_from([65_500, 10, -30], [20, 40, 65_526]),
        [56.0, 30.0, 20.0]
    );
}

/// A barrier passing beside the camera is nearest across from it.
#[test]
fn a_line_is_heard_from_its_nearest_point() {
    let near = nearest_on_line([-100.0, 0.0, 50.0], [100.0, 0.0, 50.0]);
    assert_eq!(near, [0.0, 0.0, 50.0]);
    assert_eq!(
        nearest_on_line([10.0, 0.0, 0.0], [20.0, 0.0, 0.0]),
        [10.0, 0.0, 0.0]
    );
}

/// Panned by direction along the camera's right, toward the middle when
/// near; louder and brighter near than far.
#[test]
fn placing_pans_dims_and_dulls_with_distance() {
    let right = [1.0, 0.0, 0.0];
    assert_eq!(pan([1_000.0, 0.0, 0.0], 1_000.0, right), 1.0);
    assert_eq!(pan([-1_000.0, 0.0, 0.0], 1_000.0, right), -1.0);
    assert_eq!(pan([0.0, 0.0, 800.0], 800.0, right), 0.0);
    assert!((pan([150.0, 0.0, 0.0], 150.0, right) - 0.5).abs() < 1.0e-6);
    assert_eq!(loudness(0.0, 2_000.0), 1.0);
    assert_eq!(loudness(2_000.0, 2_000.0), 0.0);
    assert!(loudness(500.0, 2_000.0) > loudness(1_500.0, 2_000.0));
    assert!(dullness(0.0, 2_000.0) > dullness(1_900.0, 2_000.0));
    assert!(dullness(2_000.0, 2_000.0) > 0.0);
}

/// The model names the tuning gives match retail's: Artoria's three
/// generators are actors, Luna Azine's nine power transformers furniture,
/// and its shield is a force field.
#[test]
fn the_tunings_names_find_retails_machines() {
    let tuning = crate::modern_tuning::ModernTuning::defaults();
    let atmosphere = tuning.atmosphere();
    let source = |model: &str| {
        atmosphere
            .sources
            .iter()
            .position(|source| source.models.iter().any(|name| name == model))
            .unwrap()
    };
    let (residency, _, world) =
        crate::test_worlds::offline_world_retained(11, TerrainSpacing::Units256);
    let mut names = HashMap::new();
    let generators = world
        .arena
        .actor_list()
        .iter()
        .filter_map(|&id| world.arena.actor(id).ok())
        .filter(|actor| {
            named(
                &residency,
                atmosphere,
                &mut names,
                actor.rotating.visual.model_word,
            ) == Some(source("newgen"))
        })
        .count();
    assert_eq!(generators, 3);
    let (residency, _, world) =
        crate::test_worlds::offline_world_retained(8, TerrainSpacing::Units256);
    let mut names = HashMap::new();
    let placed = furniture(&world, &residency, atmosphere, &mut names);
    let transformers = placed
        .iter()
        .filter(|(index, _)| *index == source("util04"))
        .count();
    assert_eq!(transformers, 9);
    let fields = world
        .arena
        .actor_list()
        .iter()
        .filter(|&&id| world.arena.selector18(id).is_ok())
        .count();
    assert!(fields >= 1, "Luna Azine's shield");
}

/// A held crystal ring rises over its attack, follows its crystal's flare
/// down to nothing, and gives way to a newer ring over the attack again.
#[test]
fn a_held_ring_rises_follows_the_flash_and_gives_way() {
    let attack = 0.12;
    // Rising: silent at the strike, half way at half the attack, whole by it.
    assert_eq!(ring_envelope(0.0, attack, 1.0, None), 0.0);
    assert!((ring_envelope(0.06, attack, 1.0, None) - 0.5).abs() < 1.0e-6);
    assert_eq!(ring_envelope(0.12, attack, 1.0, None), 1.0);
    // Then as loud as the flash: the flare fades over 1.2 s, and so does it.
    for (age, level) in [(0.3, 0.75), (0.6, 0.5), (1.08, 0.1)] {
        assert!((ring_envelope(age, attack, level, None) - level).abs() < 1.0e-6);
    }
    assert_eq!(ring_envelope(1.2, attack, 0.0, None), 0.0);
    // A newer ring of the crystal took over 0.06 s ago: half way out.
    assert!((ring_envelope(0.5, attack, 1.0, Some(0.06)) - 0.5).abs() < 1.0e-6);
    assert_eq!(ring_envelope(0.5, attack, 1.0, Some(0.12)), 0.0);
    // Without an attack (the tuning's): whole at the strike, as loud as the
    // flash after, and gone at once when a newer ring takes over.
    assert_eq!(ring_envelope(0.0, 0.0, 1.0, None), 1.0);
    assert!((ring_envelope(0.6, 0.0, 0.5, None) - 0.5).abs() < 1.0e-6);
    assert_eq!(ring_envelope(0.6, 0.0, 0.5, Some(0.0)), 0.0);
}

/// A burst into a cluster rings no more than three crystals at once, a new
/// one cutting off the oldest, and a pair of bolts rings once; a crystal
/// struck again takes its own ring over at any time.
#[test]
fn crystal_rings_are_capped() {
    assert_eq!(admit_ring(&[], 0, false), Some(false));
    let two = [(1, 0, 900_000), (2, 100_000, 900_000)];
    assert_eq!(admit_ring(&two, 150_000, false), None);
    assert_eq!(admit_ring(&two, 150_000, true), Some(false));
    assert_eq!(admit_ring(&two, 200_000, false), Some(false));
    let three = [(1, 0, 900_000), (2, 100_000, 900_000), (3, 200_000, 900_000)];
    assert_eq!(admit_ring(&three, 300_000, false), Some(true));
}

/// The crystals' rings are Splice's energy shields, which the release pak
/// carries, held with the flash from the strike on (no attack at all,
/// 2026-10-27), heard to 4500 units.
#[test]
fn the_crystals_ring_splices_shields_with_the_flash() {
    let tuning = crate::modern_tuning::current();
    let shields = tuning.atmosphere().shields.as_ref().unwrap();
    assert_eq!(shields.attack, Some(0.0));
    assert_eq!(shields.range, 4500.0);
    assert_eq!(shields.sounds.len(), 3);
    assert!(shields
        .sounds
        .iter()
        .all(|name| name.starts_with("crystal/")));
}
