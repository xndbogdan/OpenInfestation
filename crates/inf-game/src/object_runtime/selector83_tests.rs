use super::*;
use crate::terrain_runtime::TerrainSpacing;

struct Host<'a> {
    terrain: &'a mut TerrainRuntime,
    camera: Option<[u16; 3]>,
}

impl Selector83Host for Host<'_> {
    type Error = String;
    fn terrain(&mut self) -> Option<&mut TerrainRuntime> {
        Some(self.terrain)
    }
    fn camera_position(&mut self, _: &mut ObjectArena) -> Result<Option<[u16; 3]>, String> {
        Ok(self.camera)
    }
}

/// Every patch the world's lists hold.
fn patches(arena: &ObjectArena) -> Vec<ObjectId> {
    let mut found: Vec<ObjectId> = [
        crate::object_runtime::ObjectList::Flag0Primary,
        crate::object_runtime::ObjectList::Flag0Secondary,
        crate::object_runtime::ObjectList::Flag1Primary,
        crate::object_runtime::ObjectList::Flag1Secondary,
        crate::object_runtime::ObjectList::Parking,
    ]
    .iter()
    .flat_map(|list| arena.list(*list).iter().map(|node| node.object))
    .filter(|object| arena.selector83(*object).is_ok())
    .collect();
    found.sort_by_key(|object| format!("{object:?}"));
    found.dedup();
    found
}

/// World 0 (the Alien Brain's map) places two patches, 1536 by 2560 (12
/// by 20 cells), each sinking 200 below the water plane over 2500 steps,
/// sunk for 2000 and risen for 5000.
#[test]
fn the_alien_brains_map_places_two_patches() {
    let (_, _, world) = crate::test_worlds::offline_world_retained(0, TerrainSpacing::Units256);
    let found: Vec<_> = patches(&world.arena)
        .into_iter()
        .map(|object| {
            let state = world.arena.selector83(object).unwrap();
            (
                state.table_78,
                state.state_74,
                world.arena.object(object).unwrap().base.position_words,
            )
        })
        .collect();
    let table = Some(Selector83Table {
        extent: [1536, 2560],
        depth: 200,
        travel: 2500,
        risen: 5000,
        sunk: 2000,
    });
    assert_eq!(
        found,
        [
            (table, 0, Some([44925, 65140, 52530])),
            (table, 0, Some([37648, 64603, 43840]))
        ]
    );
}

/// 0043C3D0 through a whole cycle: the first update saves the 240 cells and
/// starts sinking; between the ends only a patch within 8000 of the camera
/// moves; the end of the travel leaves every cell at the water plane less
/// 200; the sunk wait, the rise and the risen wait follow, and the rise's
/// end restores every saved byte.
#[test]
fn a_patch_sinks_waits_rises_and_waits() {
    let (_, _, world) = crate::test_worlds::offline_world_retained(0, TerrainSpacing::Units256);
    let mut arena = world.arena;
    let mut terrain = world.terrain;
    let patch = patches(&arena)[0];
    let at = arena.object(patch).unwrap().base.position_words.unwrap();
    let corner = [at[0].wrapping_sub(6 * 128), at[2].wrapping_sub(10 * 128)];
    let byte = |terrain: &TerrainRuntime, [i, j]: [u16; 2]| {
        let [x, z] = [
            corner[0].wrapping_add(i * 128),
            corner[1].wrapping_add(j * 128),
        ];
        terrain.cell_at_grid([x >> 7, z >> 7]).raw() as u8
    };
    let before: Vec<u8> = (0..12)
        .flat_map(|i| (0..20).map(move |j| [i, j]))
        .map(|cell| byte(&terrain, cell))
        .collect();
    let far = Some([at[0].wrapping_add(8001), at[1], at[2]]);
    let near = Some(at);
    let tick = |arena: &mut ObjectArena, terrain: &mut TerrainRuntime, camera, delta| {
        arena
            .tick_selector83(patch, delta, &mut Host { terrain, camera })
            .unwrap();
        let state = arena.selector83(patch).unwrap();
        (state.state_74, state.timer_6c)
    };
    // Saving: the heights are kept and sinking starts on a fresh timer.
    assert_eq!(tick(&mut arena, &mut terrain, far, 125_000), (3, 0));
    assert_eq!(
        arena.selector83(patch).unwrap().heights_70.as_deref(),
        Some(&before[..])
    );
    // Halfway with the camera 8001 away: nothing moves.
    assert_eq!(
        tick(&mut arena, &mut terrain, far, 1250 * 1024),
        (3, 1250 * 1024)
    );
    let now: Vec<u8> = (0..12)
        .flat_map(|i| (0..20).map(move |j| [i, j]))
        .map(|cell| byte(&terrain, cell))
        .collect();
    assert_eq!(now, before);
    // The travel's end: every cell at the plane less 200, whatever the camera.
    assert_eq!(tick(&mut arena, &mut terrain, far, 1250 * 1024), (4, 0));
    let plane = i32::from((terrain.plane_base() >> terrain.plane_shift()) as i16);
    let sunk = ((plane - 200) >> terrain.height_shift()) as u8;
    assert!((0..12)
        .flat_map(|i| (0..20).map(move |j| [i, j]))
        .all(|cell| byte(&terrain, cell) == sunk));
    // Sunk for 2000 steps, then rising: with the camera at the patch the
    // cells move a quarter of the way back.
    assert_eq!(
        tick(&mut arena, &mut terrain, near, 1999 * 1024),
        (4, 1999 * 1024)
    );
    assert_eq!(tick(&mut arena, &mut terrain, near, 1024), (1, 0));
    assert_eq!(
        tick(&mut arena, &mut terrain, near, 625 * 1024),
        (1, 625 * 1024)
    );
    let shift = terrain.height_shift();
    let target = plane - 200;
    for (index, cell) in (0..12)
        .flat_map(|i| (0..20).map(move |j| [i, j]))
        .enumerate()
    {
        let saved = i32::from(before[index] as i8) << shift;
        let fraction = 0x1_0000 - (625 << 16) / 2500;
        let moved = ((target - saved) * fraction >> 16) + saved;
        assert_eq!(byte(&terrain, cell), (moved >> shift) as u8);
    }
    // The rise's end restores every saved byte; risen for 5000 steps.
    assert_eq!(tick(&mut arena, &mut terrain, far, 1875 * 1024), (2, 0));
    let now: Vec<u8> = (0..12)
        .flat_map(|i| (0..20).map(move |j| [i, j]))
        .map(|cell| byte(&terrain, cell))
        .collect();
    assert_eq!(now, before);
    assert_eq!(tick(&mut arena, &mut terrain, far, 5000 * 1024), (3, 0));
}

/// `patch-world19.json` in retail, 125 ms a tick with the camera beyond
/// 8000: the first patch saves on tick 1, is sunk on tick 22 (every cell of
/// its first row at byte AB), starts rising on 39, is back on 60 with every
/// byte restored, and sinks again from tick 101, 122 and so on.
#[test]
fn a_patch_keeps_retails_ticks() {
    let (_, _, world) = crate::test_worlds::offline_world_retained(0, TerrainSpacing::Units256);
    let mut arena = world.arena;
    let mut terrain = world.terrain;
    let patch = patches(&arena)[0];
    let at = arena.object(patch).unwrap().base.position_words.unwrap();
    let row = |terrain: &TerrainRuntime| -> Vec<u8> {
        let [x, z] = [at[0].wrapping_sub(6 * 128), at[2].wrapping_sub(10 * 128)];
        (0..20)
            .map(|j: u16| {
                terrain
                    .cell_at_grid([x >> 7, z.wrapping_add(j * 128) >> 7])
                    .raw() as u8
            })
            .collect()
    };
    let before = row(&terrain);
    assert_eq!(before[..4], [0x02, 0xf6, 0xeb, 0xe6]);
    let far = Some([at[0].wrapping_add(9000), at[1], at[2]]);
    let mut changes = Vec::new();
    for tick in 1..=201 {
        let state = arena.selector83(patch).unwrap().state_74;
        arena
            .tick_selector83(
                patch,
                125_000,
                &mut Host {
                    terrain: &mut terrain,
                    camera: far,
                },
            )
            .unwrap();
        let next = arena.selector83(patch).unwrap().state_74;
        if next != state {
            changes.push((tick, next));
        }
        if tick == 22 {
            assert_eq!(row(&terrain), [0xab; 20]);
        }
        if tick == 60 {
            assert_eq!(row(&terrain), before);
        }
    }
    assert_eq!(
        changes,
        [
            (1, 3),
            (22, 4),
            (39, 1),
            (60, 2),
            (101, 3),
            (122, 4),
            (139, 1),
            (160, 2),
            (201, 3)
        ]
    );
}
