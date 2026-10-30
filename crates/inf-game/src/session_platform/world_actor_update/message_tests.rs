//! A rig the vehicle drops calls Helniazine's mines (selector 53) through
//! the world: the idle mine it lands on arms its timer and answers its kind,
//! and one beyond the reach does not hear it.

use super::*;
use crate::terrain_runtime::TerrainSpacing;

#[test]
fn a_dropped_rig_wakes_the_mine_it_lands_on() {
    let (_runtime, _assets, mut world) =
        crate::test_worlds::offline_world_retained(6, TerrainSpacing::Units256);
    let arena = &mut world.arena;
    let mines: Vec<ObjectId> = arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| arena.selector53(actor).is_ok())
        .collect();
    let rig = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| arena.selector52(actor).is_ok())
        .expect("Helniazine places rigs");
    let mine = mines[0];
    assert_eq!(arena.selector53(mine).unwrap().timer_13c, 0, "an idle mine");
    let kind = u32::from(arena.selector53(mine).unwrap().kind_138);
    // Every other mine far away, the rig beside the first.
    let [x, y, z] = arena.object(mine).unwrap().base.position_words.unwrap();
    for &other in &mines[1..] {
        arena.place_for_tests(other, [x.wrapping_add(0x4000), y, z]);
    }
    arena.place_for_tests(rig, [x.wrapping_add(0x100), y, z]);
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    assert_eq!(call_world_mines(arena, rig, 0x400, &process), Ok(kind));
    assert_ne!(arena.selector53(mine).unwrap().timer_13c, 0, "armed");
    // Out of reach, nothing answers.
    arena.place_for_tests(rig, [x.wrapping_add(0x2000), y, z]);
    assert_eq!(call_world_mines(arena, rig, 0x400, &process), Ok(0));
}
