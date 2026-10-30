use super::*;

fn tick(fade: &mut WorldFadeState, delta: i32, elapsed: i32, far: u32, header: u32) -> u32 {
    fade.performance.tick(delta as u32, elapsed as u32).unwrap();
    update_manager_far(far, header, fade.performance.particle_load_factor as i32)
}

#[test]
fn manager_feedback_preserves_header_floor_and_upper_cap() {
    assert_eq!(update_manager_far(0x2800, 1280, 0), 0x2800 - 208);
    assert_eq!(update_manager_far(0x1400, 1280, 0), 0x1400);
    assert_eq!(update_manager_far(0x1800, 7000, 0), 7256);
    assert_eq!(update_manager_far(0x3000, 1280, 0x1_0000), 0x3060);
    assert_eq!(update_manager_far(0x30ff, 1280, 0x1_0000), 0x3100);
    assert_eq!(update_manager_far(0x2800, 1280, 0xd000), 0x2800);
}

#[test]
fn predraw_keeps_inclusive_deadband_and_native_truncation() {
    let mut fade = WorldFadeState::loaded(1000, 100, WorldTimingRing::default());
    assert_eq!(fade.predraw(1064, 164, 0), [225, 1000]);
    assert_eq!(fade.predraw(936, 36, 0), [225, 1000]);
    assert_eq!(fade.predraw(1065, 165, 0), [225, 1064]);
    assert_eq!(fade.smoothed_header, 164);
    // Signed difference divides toward zero; these low-level inputs isolate
    // the arithmetic, outside authored world-distance invariants.
    let mut fade = WorldFadeState::loaded(0, 1, WorldTimingRing::default());
    assert_eq!(fade.predraw(64, (-737_i32) as u32, 0), [15, 0]);
}

#[test]
fn load_tick_predraw_order_reproduces_first_frame_slew() {
    let mut fade = WorldFadeState::loaded(0x2800, 1280, WorldTimingRing::default());
    // Static 125000us delta yields feedback 0 and manager F=10032; predraw
    // advances retained F by 64 rather than copying that manager target.
    let far = tick(&mut fade, 125_000, 125_000, 0x2800, 1280);
    assert_eq!(far, 10032);
    assert_eq!(fade.predraw(far, 1280, 0), [2224, 10176]);
    let far = tick(&mut fade, 125_000, 125_000, far, 1280);
    assert_eq!(fade.predraw(far, 1280, 0), [2208, 10112]);
}

#[test]
fn zoom_adjustment_clamps_header_before_slew() {
    let mut fade = WorldFadeState::loaded(10240, 1280, WorldTimingRing::default());
    assert_eq!(fade.predraw(10240, 1280, 0x7fff), [2224, 10240]);
    assert_eq!(fade.smoothed_header, 1344);
    let mut fade = WorldFadeState::loaded(2000, 1100, WorldTimingRing::default());
    assert_eq!(fade.predraw(2000, 1100, 0x7fff), [209, 2000]);
    assert_eq!(fade.smoothed_header, 1164);
}

#[test]
fn reload_retains_feedback_but_reseeds_both_slew_values_from_new_header() {
    let mut first = WorldFadeState::loaded(10240, 1280, WorldTimingRing::default());
    let far = tick(&mut first, 30_000, 30_000, 10240, 1280);
    first.predraw(far, 1280, 0x7fff);
    let retained = WorldVisibilityReload {
        manager_far: far,
        performance: first.performance.clone(),
    };
    let mut loaded = WorldFadeState::loaded(retained.manager_far, 768, retained.performance);
    assert_eq!(
        loaded.predraw(retained.manager_far, 768, 0),
        [(far as i32 - 768) / 4, far as i32]
    );
    assert_eq!(loaded.performance.cursor, 1);
    assert_eq!(loaded.performance.samples[0], 30_000);
}
