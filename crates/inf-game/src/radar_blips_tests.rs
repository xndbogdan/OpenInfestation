use super::*;

fn sine() -> [u16; 4096] {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    crate::frontend_assets::FrontendAssets::open(&root)
        .unwrap()
        .sine_table()
        .values
}

fn actor(flags: u32, x: u16, z: u16, team: i32, kind: RadarActorKind) -> RadarActor {
    RadarActor {
        flags,
        position_words: Some([x, z]),
        team,
        kind,
    }
}

fn inputs(actors: &[RadarActor]) -> RadarBlipPass {
    RadarBlipPass {
        actors: actors.to_vec(),
        eye_words: [1000, 2000],
        origin: [800, 600],
        size: [76, 76],
        // Facing along X: no turn.
        rotation: [32768, 0],
        jitter: 0,
        clock: 0,
        resource_scanner: false,
    }
}

fn no_rng(_: &mut u32) -> u32 {
    panic!("an unjammed walk draws nothing")
}

/// Only flags 0x21 without 0x10000 enter; a spawner enters whatever its
/// flags, but shows only with the Resource scanner.
#[test]
fn admission_follows_the_flags_and_the_spawners() {
    let sine = sine();
    let actors = [
        actor(0x21, 1000, 2000, 1, RadarActorKind::Other),
        actor(0x20, 1000, 2000, 1, RadarActorKind::Other),
        actor(0x10021, 1000, 2000, 1, RadarActorKind::Other),
        actor(
            0,
            1000,
            2000,
            0,
            RadarActorKind::Spawner { kind: 2, count: 8 },
        ),
    ];
    let mut rng = 0;
    let blips = radar_blips(&inputs(&actors), &sine, 0, &mut rng, no_rng).unwrap();
    // At the eye the distance is zero: the centre, origin plus half size.
    assert_eq!(
        blips,
        [RadarBlip::Dot {
            position: [876, 676],
            team: 1,
            dim: false,
            pinned: false
        }]
    );
    let scanning = RadarBlipPass {
        resource_scanner: true,
        ..inputs(&actors)
    };
    let blips = radar_blips(&scanning, &sine, 0, &mut rng, no_rng).unwrap();
    assert_eq!(
        blips[1],
        RadarBlip::Marker {
            position: [876, 676],
            half_size: 8,
            style: 0x23
        }
    );
}

/// The vehicle shows on even sevenths of the clock; dying actors dim.
#[test]
fn the_vehicle_blinks_and_the_dying_dim() {
    let sine = sine();
    let actors = [
        actor(0x21, 1000, 2000, 2, RadarActorKind::Vehicle),
        actor(0x8021, 1000, 2000, 0x12, RadarActorKind::Other),
    ];
    let mut rng = 0;
    for (clock, shown) in [(0, 2), (6, 2), (7, 1), (13, 1), (14, 2)] {
        let walk = RadarBlipPass {
            clock,
            ..inputs(&actors)
        };
        let blips = radar_blips(&walk, &sine, 0, &mut rng, no_rng).unwrap();
        assert_eq!(blips.len(), shown, "clock {clock}");
        assert!(matches!(
            blips.last(),
            Some(RadarBlip::Dot {
                team: 0x12,
                dim: true,
                ..
            })
        ));
    }
}

/// Inside the rim a distance r lands at the half size times the quarter
/// sine of 2r; Z grows upward on the radar.
#[test]
fn offsets_land_on_the_quarter_sine() {
    let sine = sine();
    // 2000 units east and 2000 north: r = isqrt(2 * 2000^2 / 4) = 1414.
    let actors = [actor(0x21, 3000, 4000, 1, RadarActorKind::Other)];
    let mut rng = 0;
    let blips = radar_blips(&inputs(&actors), &sine, 0, &mut rng, no_rng).unwrap();
    let span = 1414 * 2;
    let reach = sine_q15(span, &sine);
    let across = i32::from((reach * 2000 >> 15) as i16) * 76 / span;
    assert_eq!(
        blips,
        [RadarBlip::Dot {
            position: [800 + 76 + across, 600 - across + 76],
            team: 1,
            dim: false,
            pinned: false
        }]
    );
}

/// Beyond the rim a blip is a single pixel at the edge, shown when the
/// rotated blink word's parity differs from the clock's sevenths.
#[test]
fn pinned_blips_blink_on_the_rotated_word() {
    let sine = sine();
    let actors = [
        actor(0x21, 1000 + 20000, 2000, 1, RadarActorKind::Other),
        actor(0x21, 1000 + 20000, 2000, 1, RadarActorKind::Other),
    ];
    let mut rng = 0;
    // 0b10 rotates to 1, odd, which differs from clock 0's even sevenths:
    // shown. It then rotates to 0x8000, even: hidden.
    let blips = radar_blips(&inputs(&actors), &sine, 0b10, &mut rng, no_rng).unwrap();
    assert_eq!(blips.len(), 1);
    let RadarBlip::Dot {
        position, pinned, ..
    } = blips[0]
    else {
        panic!("a dot")
    };
    assert!(pinned);
    // Due east at the rim: the reach 0x4000 over r.
    let r = isqrt_48efc0(20000 * 20000 >> 2) as i32;
    let across = i32::from((0x4000 * 20000 >> 15) as i16) * 76 / r;
    assert_eq!(position, [800 + 76 + across, 676]);
    assert_eq!(dot_pixels(position, true), [[position[0] + 1, 676]]);
    assert_eq!(dot_pixels([10, 20], false).len(), 5);
}

/// A jitter above 7 draws twice per entering actor, each draw modulo a
/// quarter of the jitter added to its axis.
#[test]
fn the_jam_draws_twice_per_actor() {
    let sine = sine();
    let actors = [
        actor(0x21, 1000, 2000, 1, RadarActorKind::Other),
        actor(0x20, 1000, 2000, 1, RadarActorKind::Other),
    ];
    let walk = RadarBlipPass {
        jitter: 8,
        ..inputs(&actors)
    };
    let mut draws = vec![5_u32, 6];
    let mut rng = 0;
    let blips = radar_blips(&walk, &sine, 0, &mut rng, |_| draws.remove(0)).unwrap();
    assert!(draws.is_empty());
    // (5 % 2, 6 % 2) = (1, 0): one unit east.
    assert_eq!(blips.len(), 1);
}

/// A spawner of another kind keeps its predecessor's style, and the first
/// one has none.
#[test]
fn unknown_spawner_kinds_reuse_the_previous_style() {
    let sine = sine();
    let actors = [
        actor(
            0,
            1000,
            2000,
            0,
            RadarActorKind::Spawner { kind: 3, count: 4 },
        ),
        actor(
            0,
            1000,
            2000,
            0,
            RadarActorKind::Spawner { kind: 5, count: 4 },
        ),
    ];
    let walk = RadarBlipPass {
        resource_scanner: true,
        ..inputs(&actors)
    };
    let mut rng = 0;
    let blips = radar_blips(&walk, &sine, 0, &mut rng, no_rng).unwrap();
    assert!(blips
        .iter()
        .all(|blip| matches!(blip, RadarBlip::Marker { style: 0x24, .. })));
    assert_eq!(
        radar_blips(
            &RadarBlipPass {
                actors: vec![actors[1]],
                ..walk
            },
            &sine,
            0,
            &mut rng,
            no_rng
        ),
        Err(RadarBlipError::SpawnerStyle { kind: 5 })
    );
}

/// Infest03's first 60 radar walks (`radar-blips.js`), all from the terrain
/// radar with the Resource scanner: each walk's actors and inputs give the
/// dots and markers retail drew, in order, and the RNG it left. Each team
/// has one colour (the dying halved), and each spawner kind one style.
#[test]
fn walks_match_infest03() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/radar_blips_native.json");
    let walks: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(walks.len(), 60);
    let sine = sine();
    let int = |value: &serde_json::Value| value.as_i64().unwrap() as i32;
    let mut colours = std::collections::HashMap::new();
    let mut styles = std::collections::HashMap::new();
    let (mut dots, mut markers) = (0, 0);
    for walk in &walks {
        let at = walk["at"].as_str().unwrap();
        let actors: Vec<RadarActor> = walk["actors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|actor| RadarActor {
                flags: actor["flags"].as_u64().unwrap() as u32,
                position_words: Some([int(&actor["x"]) as u16, int(&actor["z"]) as u16]),
                team: int(&actor["team"]),
                kind: match actor["kind"].as_str().unwrap() {
                    "vehicle" => RadarActorKind::Vehicle,
                    "spawner" => RadarActorKind::Spawner {
                        kind: int(&actor["spawner"][0]) as i8,
                        count: int(&actor["spawner"][1]) as i8,
                    },
                    _ => RadarActorKind::Other,
                },
            })
            .collect();
        let axis = [
            walk["axis_x"][0].as_f64().unwrap() as f32,
            0.0,
            walk["axis_x"][1].as_f64().unwrap() as f32,
        ];
        assert_eq!(int(&walk["skip"]), 0);
        let pass = RadarBlipPass {
            actors: actors.clone(),
            eye_words: [int(&walk["eye"][0]) as u16, int(&walk["eye"][1]) as u16],
            origin: [int(&walk["origin"][0]), int(&walk["origin"][1])],
            size: [76, 76],
            rotation: heading_rotation(axis),
            jitter: int(&walk["jitter"]),
            clock: int(&walk["clock"]),
            resource_scanner: int(&walk["resource"]) != 0,
        };
        let mut rng = walk["rng"].as_u64().unwrap() as u32;
        let blink = crate::name_entry::name_entry_rng_next(&mut rng);
        let blips = radar_blips(
            &pass,
            &sine,
            blink,
            &mut rng,
            crate::name_entry::name_entry_rng_next,
        )
        .unwrap();
        assert_eq!(rng, walk["rng_after"].as_u64().unwrap() as u32, "{at}");
        let mut retail_dots = walk["dots"].as_array().unwrap().iter();
        let mut retail_markers = walk["markers"].as_array().unwrap().iter();
        for blip in &blips {
            match *blip {
                RadarBlip::Dot {
                    position,
                    team,
                    dim,
                    pinned,
                } => {
                    let retail = retail_dots.next().unwrap();
                    assert_eq!(position, [int(&retail[0]), int(&retail[1])], "{at}");
                    assert_eq!(pinned, int(&retail[3]) != 0, "{at}");
                    let colour = retail[2].as_u64().unwrap();
                    assert_eq!(
                        *colours.entry((team, dim)).or_insert(colour),
                        colour,
                        "{at} team {team}"
                    );
                    dots += 1;
                }
                RadarBlip::Marker {
                    position,
                    half_size,
                    style,
                } => {
                    let retail = retail_markers.next().unwrap();
                    assert_eq!(position, [int(&retail[0]), int(&retail[1])], "{at}");
                    assert_eq!(half_size, int(&retail[2]), "{at}");
                    let word = retail[3].as_u64().unwrap();
                    assert_eq!(*styles.entry(style).or_insert(word), word, "{at}");
                    markers += 1;
                }
            }
        }
        assert!(retail_dots.next().is_none(), "{at}: retail drew more dots");
        assert!(
            retail_markers.next().is_none(),
            "{at}: retail drew more markers"
        );
    }
    assert_eq!((dots, markers), (3091, 128));
    // Friends in yellow and enemies in red, in the recording's RGB565.
    assert_eq!(colours.get(&(1, false)).copied(), Some(0xffc0));
    assert_eq!(colours.get(&(0x12, false)).copied(), Some(0xf800));
}

/// The walk admits an actor before it reads the position: a carried unit
/// (flags 1 and 2 cleared by its capture) without one is passed over, and an
/// admitted actor without one stops the walk.
#[test]
fn the_walk_reads_a_position_only_for_an_actor_it_admits() {
    let sine = sine();
    let placed = actor(0x21, 1000, 2100, 2, RadarActorKind::Other);
    let carried = RadarActor {
        position_words: None,
        ..actor(0x20, 0, 0, 2, RadarActorKind::Other)
    };
    let walk = |actors: &[RadarActor]| {
        let mut rng = 0;
        radar_blips(&inputs(actors), &sine, 0, &mut rng, |_| 0)
    };
    assert_eq!(walk(&[carried, placed]), walk(&[placed]));
    assert!(walk(&[placed]).unwrap().len() == 1);
    let admitted = RadarActor {
        flags: 0x21,
        ..carried
    };
    assert_eq!(
        walk(&[placed, admitted]),
        Err(RadarBlipError::Position { index: 1 })
    );
}
