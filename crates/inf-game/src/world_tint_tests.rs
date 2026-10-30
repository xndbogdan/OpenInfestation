use super::*;
use crate::terrain_runtime::TerrainSpacing;

/// Retail's tint words on Artoria (`artoria-generators-dump.json`): H6 0x37
/// before, 625 ms into the fade at pass 335 and the fade's end from 340 on.
const BEFORE: u32 = 0x002e_80ea;
const FADING: u32 = 0x0010_2c54;
const AFTER: u32 = 0x000a_1932;

fn artoria() -> WorldTintSchedule {
    WorldTintSchedule {
        period: 1,
        styles: vec![0x37, 0x38],
    }
}

fn styles(slot: i16) -> Result<Option<u32>, WorldTintError> {
    Ok(match slot {
        0x37 => Some(BEFORE),
        0x38 => Some(AFTER),
        _ => None,
    })
}

/// Artoria's power going down, as retail drew it: 625 ms in, each channel
/// has moved `(to - from) * ((625 << 7) / 3) >> 15` from H6 0x37; past one
/// period it holds 0x38.
#[test]
fn artorias_power_fades_as_retail_measured() {
    let schedule = artoria();
    assert_eq!(schedule.tint(0, BEFORE, false, styles), Ok(BEFORE));
    assert_eq!(schedule.tint(625, BEFORE, false, styles), Ok(FADING));
    assert_eq!(schedule.tint(1250, FADING, false, styles), Ok(AFTER));
    assert_eq!(schedule.tint(60_000, AFTER, false, styles), Ok(AFTER));
}

/// `0048E614` shifts the period right 8 before dividing, so a 1 s period's
/// fraction reaches `(999 << 7) / 3` = 42,624, past 2^15: the blend
/// overshoots and the bytes wrap to a near white for the period's last
/// tick, as retail's do.
#[test]
fn a_short_periods_blend_overshoots_and_wraps() {
    let tint = artoria().tint(999, BEFORE, false, styles).unwrap();
    assert_eq!(tint.to_le_bytes(), [250, 250, 255, 0]);
}

/// The blend keeps `0053DED0`'s high byte; a spent list copies the last
/// style whole; infrared vision replaces only the colour.
#[test]
fn the_tints_high_byte_and_infrared() {
    let schedule = artoria();
    let tint = schedule.tint(625, 0x7700_0000, false, styles).unwrap();
    assert_eq!(tint, 0x7710_2c54);
    assert_eq!(schedule.tint(2000, 0x7700_0000, false, styles), Ok(AFTER));
    let infrared = schedule.tint(625, BEFORE, true, styles).unwrap();
    assert_eq!(infrared.to_le_bytes(), [0, 127, 0, 0]);
    assert_eq!(
        WorldTintSchedule {
            period: 0,
            styles: vec![0x37]
        }
        .tint(0, 0, false, styles),
        Err(WorldTintError::Period(0))
    );
    assert_eq!(
        WorldTintSchedule {
            period: 1,
            styles: vec![0x99]
        }
        .tint(0, 0, false, styles),
        Err(WorldTintError::Style(0x99))
    );
}

/// Artoria's H14 holds H6 0x37 and 0x38 a second apart, behind flag
/// 0x1000: its load clears mission +13, so the timer waits for a generator.
/// The resident styles are retail's tint words.
#[test]
fn artoria_loads_its_schedule_and_waits() {
    let (runtime, _, world) =
        crate::test_worlds::offline_world_retained(11, TerrainSpacing::Units256);
    assert_eq!(world.tint_schedule, artoria());
    assert_eq!(world.mission.byte_13, Some(0));
    assert_eq!(world.scheduled_tint, BEFORE);
    let resources = crate::render_resources::RenderResources::new(&runtime);
    for (slot, word) in [(0x37, BEFORE), (0x38, AFTER)] {
        let request = crate::render_resources::RenderResourceRequest {
            kind: crate::render_resources::RenderResourceKind::Style,
            argument: slot,
        };
        let crate::render_resources::RenderResourceData::Style(found) =
            resources.resolve(request).unwrap().data
        else {
            unreachable!("typed H6 style")
        };
        assert_eq!(found, word, "H6 {slot:#x}");
    }
}

/// Level 1 (world 18) swings between H6 0x37 and 0x38 every 90 s from the
/// start (no flag 0x1000), so its constructor's +13 runs the timer: a tick
/// 30 s in has moved a third of the way.
#[test]
fn level_one_swings_its_tint_from_the_start() {
    let (runtime, _, mut world) =
        crate::test_worlds::offline_world_retained(18, TerrainSpacing::Units256);
    assert_eq!(
        world.tint_schedule,
        WorldTintSchedule {
            period: 90,
            styles: vec![0x37, 0x38, 0x37, 0x38, 0x37],
        }
    );
    assert_eq!(world.mission.byte_13, Some(1));
    let start = world.scheduled_tint;
    tick_world_tint(&mut world, 30_000, false, &runtime);
    assert_eq!(world.mission.dword_2c, Some(30_000));
    assert_ne!(world.scheduled_tint, start);
    assert_eq!(world.presentation.tint_word, world.scheduled_tint);
}

/// Back on Artoria after its four generators died on an earlier visit: the
/// course target they completed (load serial 79, the one selector 39 with
/// +162) is recorded finished. The load clears mission +13 for flag 0x1000,
/// then the target's after-load (0043B720) sets G+1C3, the same byte, so the
/// schedule runs and the world goes dark again within its second, as
/// retail's arrival does (a save of 2026-10-21,
/// `artoria-revisit-tank.json`).
#[test]
fn artoria_stays_dark_when_its_generators_died_before() {
    let (runtime, _, mut world) = crate::test_worlds::offline_world_with_completion(
        11,
        [0, 0, 1 << (79 - 64), 0],
        TerrainSpacing::Units256,
    );
    assert_eq!(world.mission.byte_13, Some(1));
    tick_world_tint(&mut world, 1000, false, &runtime);
    assert_eq!(world.scheduled_tint, AFTER);
}

/// The ambience's light: Artoria lit before its generators die and at its
/// darkest after; Altair Prime's two styles are one light, so none.
#[test]
fn the_ambience_reads_the_light_off_the_schedule() {
    let (runtime, _, world) =
        crate::test_worlds::offline_world_retained(11, TerrainSpacing::Units256);
    assert_eq!(light(&world, &runtime), Some(1.0));
    let (runtime, _, mut world) = crate::test_worlds::offline_world_with_completion(
        11,
        [0, 0, 1 << (79 - 64), 0],
        TerrainSpacing::Units256,
    );
    tick_world_tint(&mut world, 1000, false, &runtime);
    assert_eq!(light(&world, &runtime), Some(0.0));
    let (runtime, _, world) =
        crate::test_worlds::offline_world_retained(5, TerrainSpacing::Units256);
    assert_eq!(light(&world, &runtime), None);
}
