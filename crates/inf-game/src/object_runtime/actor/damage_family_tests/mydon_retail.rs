//! Terra Nova's two mydons (resource 96, selector 10, team 1) where the
//! recording `Infest05.run` held them when its player's cannon hit one:
//! `00428040` admitted the vehicle and called the broadcast `004281B0` at
//! `1D313C:63E`. The broadcast reaches same-team, same-resource units within
//! a quarter of the shot one's +164 (2500, so 625). The other mydon stood
//! 701 away (667 across, 1 up, 215 along); it heard nothing and never
//! attacked. At 600 it would have taken the vehicle as a target too, as both
//! mydons do in the port when they graze together.

use super::*;

#[test]
fn a_shot_mydon_tells_only_mydons_within_625_of_it() {
    let (_, _, world) = crate::test_worlds::offline_world_retained(
        2,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let mut arena = world.arena;
    let mydons: Vec<_> = arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| {
            arena
                .object(actor)
                .is_ok_and(|object| object.base.resource_word == 96)
        })
        .collect();
    assert_eq!(mydons.len(), 2);
    for &mydon in &mydons {
        assert_eq!(arena.actor(mydon).unwrap().word_b4, 1);
        assert_eq!(arena.derived_actor(mydon).unwrap().word_164, 2500);
        assert_eq!(arena.object(mydon).unwrap().base.flags & 0xa0, 0x20);
        assert_eq!(arena.family428650_actor(mydon).unwrap().count_1a0, 0);
    }
    let [shot, other] = [mydons[0], mydons[1]];
    let place = |arena: &mut ObjectArena, actor: ObjectId, position: [u16; 3]| {
        arena.owned_mut(actor).unwrap().state.base.position_words = Some(position);
    };
    // The recording's vehicle handle and the broadcast's own arguments.
    let attacker = request(0x0cd5_0001, 0x03c0_0000, 0);
    place(&mut arena, shot, [31134, 0xfa4e, 23143]);
    place(&mut arena, other, [31801, 0xfa4f, 22928]);
    arena
        .broadcast_family_damage_target(shot, attacker)
        .unwrap();
    assert_eq!(arena.family428650_actor(other).unwrap().count_1a0, 0);

    place(&mut arena, other, [31734, 0xfa4f, 23143]);
    arena
        .broadcast_family_damage_target(shot, attacker)
        .unwrap();
    let state = arena.family428650_actor(other).unwrap();
    assert_eq!(state.count_1a0, 1);
    assert_eq!(state.entries_1a4[0].handle, attacker.source);
    assert_eq!(arena.family428650_actor(shot).unwrap().count_1a0, 0);
}
