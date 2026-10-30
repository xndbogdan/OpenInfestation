use super::*;

const SURFACE: [i32; 2] = [1365, 768];

fn status(countdown: u16) -> RaceStatus {
    RaceStatus {
        countdown,
        wrong_way_19c: 0,
        missed_198: 0,
        host: true,
    }
}

/// 0046C07F: an idle race blinks "Press F10 to start" for the host and
/// "Waiting" for a guest, in the middle.
#[test]
fn an_idle_race_blinks_who_starts_it() {
    assert_eq!(
        race_status_texts(status(0xffff), 0x10, SURFACE),
        [(0x89, [682, 384])]
    );
    let guest = RaceStatus {
        host: false,
        ..status(0xffff)
    };
    assert_eq!(
        race_status_texts(guest, 0x20, SURFACE),
        [(0x85, [682, 384])]
    );
    assert!(race_status_texts(status(0xffff), 0x4f, SURFACE).is_empty());
}

/// 0046C134..0046C29C: "Starting" in the middle and sliding in from the top
/// and the bottom, which meet it as the countdown runs out.
#[test]
fn the_countdown_slides_its_texts_together() {
    for (countdown, up, down) in [
        (0x2dc6, 0, 768),
        (0x1000, 250, 518),
        (0x186, 372, 396),
        (1, 384, 384),
    ] {
        assert_eq!(
            race_status_texts(status(countdown), 0x04, SURFACE),
            [(0x86, [682, 384]), (0x86, [682, up]), (0x86, [682, down])],
            "{countdown:#x}"
        );
    }
    assert!(race_status_texts(status(0x1000), 0x30, SURFACE).is_empty());
}

/// 0046C2A1..0046C437: while the race runs, the wrong way and a missed gate,
/// each in its two spellings by the clock.
#[test]
fn a_running_race_tells_the_wrong_way_and_a_missed_gate() {
    let both = RaceStatus {
        wrong_way_19c: 1,
        missed_198: 5,
        ..status(0)
    };
    assert_eq!(
        race_status_texts(both, 0x30, SURFACE),
        [(0x83, [682, 180]), (0x87, [682, 240])]
    );
    assert_eq!(
        race_status_texts(both, 0x0f, SURFACE),
        [(0x84, [682, 180]), (0x88, [682, 240])]
    );
    assert!(race_status_texts(status(0), 0x30, SURFACE).is_empty());
}

const BOARD: [i32; 2] = [640, 480];

type Plan = (Vec<OverlayOp>, Result<(), String>);

/// H0 as retail's for the texts the boards reach, `S<id>` otherwise; ten
/// pixels a byte; an H6 style's word names its index.
fn plan(inputs: &OverlayInputs<'_>) -> Plan {
    let resources = OverlayResources {
        string: |id: i32| {
            Ok(Some(match id {
                0x82 => b"%s team wins".to_vec(),
                0x85 => b"Waiting".to_vec(),
                0x89 => b"Press F10 to start".to_vec(),
                0x8a => b"Still choosing team".to_vec(),
                _ => format!("S{id:x}").into_bytes(),
            }))
        },
        width: |text: &[u8]| Ok(10 * text.len() as i32),
        style: |index: i32| Ok(0x0100_0000 | index as u32),
    };
    let mut ops = Vec::new();
    let result = plan_network_overlay(inputs, &resources, &mut ops);
    (ops, result)
}

fn inputs<'a>(game: &'a GameNetState, mode: u32) -> OverlayInputs<'a> {
    OverlayInputs {
        surface: BOARD,
        clock: 0,
        mode,
        host: true,
        local_slot: 1,
        scores_held: false,
        game,
        race: None,
        carriers: &[],
        goals: [0; 2],
        ball_holder: None,
    }
}

fn text(text: &str, position: [i32; 2], color_word: u32) -> OverlayOp {
    OverlayOp::Text {
        text: text.as_bytes().to_vec(),
        position,
        color_word,
    }
}

fn bar(corners: [[i32; 2]; 4], sprite_slot: u16) -> OverlayOp {
    quad(sprite_slot, corners)
}

const TOP_BAR: [[i32; 2]; 4] = [[640, 0], [0, 0], [0, 19], [640, 19]];

/// Three players in, each with a name and details, with these scores.
fn players(scores: [[u16; 3]; 3]) -> GameNetState {
    let mut game = GameNetState::default();
    for (index, (name, details)) in [("Al", "AA"), ("Bob", "BB"), ("Cy", "CC")]
        .iter()
        .enumerate()
    {
        let slot = index + 1;
        game.present[slot] = 1;
        game.scores[slot] = scores[index];
        game.names[slot][..name.len()].copy_from_slice(name.as_bytes());
        game.details[slot][..details.len()].copy_from_slice(details.as_bytes());
    }
    game
}

fn texts(ops: &[OverlayOp]) -> Vec<OverlayOp> {
    ops.iter()
        .filter(|op| matches!(op, OverlayOp::Text { .. }))
        .cloned()
        .collect()
}

/// 004424BB..00442737, 004428D9..00442A33 and 00441510: Backspace's backdrop
/// about the middle, the top bar, then the compact board of details along
/// it and the full one of names and scores, both highest score first and
/// ties in slot order.
#[test]
fn deathmatch_boards_rank_the_players_by_score() {
    let game = players([[4, 1, 5], [7, 2, 9], [1, 3, 5]]);
    let (ops, result) = plan(&OverlayInputs {
        scores_held: true,
        ..inputs(&game, 2)
    });
    result.unwrap();
    let [c1, c2, c3] = [0x0100_0027, 0x0100_0028, 0x0100_0029];
    assert_eq!(
        ops,
        [
            bar([[32, 240], [32, -48], [320, -48], [320, 240]], 0x2a6),
            bar([[320, -48], [608, -48], [608, 240], [320, 240]], 0x2a6),
            bar([[608, 240], [608, 528], [320, 528], [320, 240]], 0x2a6),
            bar([[320, 528], [32, 528], [32, 240], [320, 240]], 0x2a6),
            bar(TOP_BAR, 0x230),
            text("BB", [41, 14], c2),
            text("AA", [116, 14], c1),
            text("CC", [191, 14], c3),
            text("Bob (BB)", [304, 96], c2),
            text(" :     9", [384, 96], c2),
            text("Al (AA)", [314, 115], c1),
            text(" :     5", [384, 115], c1),
            text("Cy (CC)", [314, 134], c3),
            text(" :     5", [384, 134], c3),
        ]
    );
    let other = plan(&inputs(&game, 1)).0;
    assert_eq!(texts(&other).len(), 3, "the full board needs Backspace");
}

/// 0044180E..00441925: a compact entry that would pass "MMMM" from the
/// edge starts the next line from the board's left under a new bar.
#[test]
fn the_compact_board_wraps_under_a_new_bar() {
    let mut game = GameNetState::default();
    for slot in 1..SLOTS {
        game.present[slot] = 1;
        game.details[slot][0] = b'@' + slot as u8;
    }
    let ops = plan(&inputs(&game, 1)).0;
    assert_eq!(ops.len(), 18);
    assert_eq!(ops[8], text("H", [571, 14], 0x0100_002e));
    assert_eq!(ops[9], bar([[640, 19], [0, 19], [0, 38], [640, 38]], 0x230));
    assert_eq!(ops[10], text("I", [46, 33], 0x0100_002f));
    assert_eq!(ops[17], text("P", [571, 33], 0x0100_0036));
}

/// 0044290B..00442991 and 00441E2E..00441F7A: a race's second bar, each
/// racer's total time under the details, the full board's best lap time and
/// laps, and this machine's racer brightened on the even ticks.
#[test]
fn a_race_lists_times_and_laps() {
    let mut game = players([[0, 0, 16], [0, 0, 15], [0, 0, 0]]);
    game.present[3] = 0;
    let mut race = RaceState {
        steps_6c: 0,
        records: [crate::object_runtime::RacerRecord::default(); crate::object_runtime::RACERS],
        countdown_2d4: 0,
        grid_2d6: [[0; 3]; 16],
        laps_338: 3,
        finished_33c: 0,
        sway_340: None,
        restart_34c: 0,
        wait_354: 0,
    };
    race.records[1].best_02 = 1247;
    race.records[1].total_06 = 2500;
    race.records[1].laps_18 = 2;
    race.records[1].flags_20 = 1;
    let base = OverlayInputs {
        scores_held: true,
        local_slot: 2,
        race: Some(&race),
        ..inputs(&game, 3)
    };
    let (ops, result) = plan(&base);
    result.unwrap();
    assert_eq!(ops[5], bar([[640, 38], [0, 38], [0, 19], [640, 19]], 0x230));
    let [mine, theirs] = [0x0140_4068, 0x0100_0027];
    assert_eq!(
        texts(&ops),
        [
            text("AA", [41, 14], theirs),
            text("02:05", [26, 33], theirs),
            text("BB", [116, 14], mine),
            text("00:00", [101, 33], mine),
            text("Al (AA)", [250, 96], theirs),
            text("- 01:02:35   2/3", [320, 96], theirs),
            text("Bob (BB)", [240, 115], mine),
            text("- 00:00:00   3/3", [320, 115], mine),
        ]
    );
    let odd = plan(&OverlayInputs { clock: 5, ..base }).0;
    assert_eq!(texts(&odd)[2], text("BB", [116, 14], 0x0100_0028));
}

/// 0044195F..00441CA9: a team game lists each team under its line (won,
/// lost and score from its task), those still choosing last and white, and
/// flashes a banner's carrier in the banner's colour.
#[test]
fn team_games_head_each_team_and_flash_the_carrier() {
    use crate::network::game::TeamScore;
    let mut game = players([[0, 0, 3], [0, 0, 6], [0, 0, 1]]);
    game.team_game = 1;
    game.states[1..=3].copy_from_slice(&[2, 3, 1]);
    game.teams[2] = Some(TeamScore {
        words_0c: [1, 2],
        score: 7,
        ..TeamScore::default()
    });
    game.teams[3] = Some(TeamScore {
        words_0c: [0, 4],
        score: 12,
        ..TeamScore::default()
    });
    let carriers = [(1, 3)];
    let base = OverlayInputs {
        scores_held: true,
        carriers: &carriers,
        ..inputs(&game, 0)
    };
    let (ops, result) = plan(&base);
    result.unwrap();
    let [two, three, white] = [0x0100_0027, 0x0100_0028, 0x01ff_ffff];
    assert_eq!(
        texts(&ops),
        [
            text("S3a : 01-02 : Score     7", [195, 76], two),
            text("Al (AA)", [250, 105], three),
            text(" :     3 : 00-00", [320, 105], three),
            text("S3b : 00-04 : Score    12", [195, 133], three),
            text("Bob (BB)", [240, 162], three),
            text(" :     6 : 00-00", [320, 162], three),
            text("Still choosing team :", [215, 190], white),
            text("Cy (CC)", [250, 219], white),
            text(" :     1 : 00-00", [320, 219], white),
            text("BB", [41, 14], three),
            text("AA", [116, 14], three),
            text("CC", [191, 14], white),
        ]
    );
    let later = plan(&OverlayInputs { clock: 10, ..base }).0;
    assert_eq!(texts(&later)[1], text("Al (AA)", [250, 105], two));
    game.teams[2] = None;
    let missing = plan(&inputs(&game, 0)).1;
    assert!(missing.is_ok(), "the compact board reads no task");
    let held = plan(&OverlayInputs {
        scores_held: true,
        ..inputs(&game, 0)
    })
    .1;
    assert!(held.unwrap_err().contains("no task"));
}

/// 00442236..004424B7: the game's end blinks each 50 ticks, the winning
/// team in a team game, then who restarts it.
#[test]
fn the_game_end_blinks_the_winner_and_who_restarts() {
    let mut game = GameNetState::default();
    (game.ended, game.team_game, game.winning_team) = (1, 1, 4);
    let ops = plan(&OverlayInputs {
        clock: 50,
        ..inputs(&game, 0)
    })
    .0;
    assert_eq!(
        ops,
        [
            text("S3c team wins", [255, 160], 0x0100_0000),
            text("Press F10 to start", [230, 208], 0x0100_0000),
            bar(TOP_BAR, 0x230),
        ]
    );
    let hidden = plan(&OverlayInputs {
        clock: 100,
        ..inputs(&game, 0)
    })
    .0;
    assert_eq!(hidden, [bar(TOP_BAR, 0x230)]);
    game.team_game = 0;
    let guest = plan(&OverlayInputs {
        clock: 50,
        host: false,
        ..inputs(&game, 1)
    })
    .0;
    assert_eq!(guest[0], text("Waiting", [285, 208], 0x0100_0000));
}

/// 00441A1A: a Powerball side past 3 would count the bytes after the goal
/// bytes, so its board stops there.
#[test]
fn powerball_counts_only_its_two_sides() {
    let mut game = players([[0; 3]; 3]);
    game.team_game = 1;
    game.states[1] = 4;
    let (_, result) = plan(&OverlayInputs {
        scores_held: true,
        ..inputs(&game, 4)
    });
    assert!(result.unwrap_err().contains("past the goal bytes"));
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

/// `check_net_overlay.py`: `004421E6..00442B2E` and `00441510` native over
/// deathmatches, races (with and without their object), team games with
/// banners carried, Powerball's goals and ball holder, the game's end,
/// ties, wraps, long names, surfaces and widths; the port's ops are
/// retail's text, quad and order.
#[test]
fn the_overlay_draws_as_00442080_draws_it() {
    use crate::network::game::TeamScore;
    use crate::object_runtime::{ActorHandle, RacerRecord, RACERS};
    use serde_json::Value;
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/fixtures/net_overlay_native.json")).unwrap();
    let n = |value: &Value| value.as_i64().unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 102);
    for (index, case) in cases.iter().enumerate() {
        let i = &case["input"];
        let mut game = GameNetState::default();
        for slot in 0..SLOTS {
            game.present[slot] = n(&i["present"][slot]) as u8;
            game.states[slot] = n(&i["states"][slot]) as u32;
            game.names[slot].copy_from_slice(&hex(i["names"][slot].as_str().unwrap()));
            game.details[slot].copy_from_slice(&hex(i["details"][slot].as_str().unwrap()));
            game.scores[slot] = [0, 1, 2].map(|k| n(&i["scores"][slot][k]) as u16);
        }
        for (team, scores) in i["teams"].as_array().unwrap().iter().enumerate() {
            game.teams[team] = scores.as_array().map(|s| TeamScore {
                words_0c: [n(&s[0]) as u16, n(&s[1]) as u16],
                score: n(&s[2]) as u16,
                ..TeamScore::default()
            });
        }
        game.team_game = n(&i["team_game"]) as u8;
        game.ended = n(&i["ended"]) as u8;
        game.winning_team = n(&i["winning_team"]) as u32;
        let race = i["race"].as_object().map(|race| {
            let mut records = [RacerRecord::default(); RACERS];
            for (record, value) in records.iter_mut().zip(race["records"].as_array().unwrap()) {
                record.best_02 = n(&value[0]) as u16;
                record.total_06 = n(&value[1]) as u16;
                record.laps_18 = n(&value[2]) as u32;
                record.flags_20 = n(&value[3]) as u8;
            }
            RaceState {
                steps_6c: 0,
                records,
                countdown_2d4: 0,
                grid_2d6: [[0; 3]; 16],
                laps_338: n(&race["laps"]) as u32,
                finished_33c: 0,
                sway_340: None,
                restart_34c: 0,
                wait_354: 0,
            }
        });
        // ObjectArena::banner_carriers' walk over the receipt's actor list.
        let null = n(&i["null"]) as u32;
        let carriers: Vec<(u8, u32)> = i["banners"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|b| n(&b[0]) != 0 && n(&b[1]) != 0 && n(&b[2]) as u32 != null)
            .map(|b| {
                (
                    ActorHandle::from_raw(n(&b[2]) as u32).owner(),
                    n(&b[3]) as u32,
                )
            })
            .collect();
        let inputs = OverlayInputs {
            surface: [n(&i["surface"][0]) as i32, n(&i["surface"][1]) as i32],
            clock: n(&i["clock"]) as u32,
            mode: n(&i["mode"]) as u32,
            host: n(&i["host"]) != 0,
            local_slot: n(&i["local_slot"]) as u8,
            scores_held: n(&i["held"]) != 0,
            game: &game,
            race: race.as_ref(),
            carriers: &carriers,
            goals: i["goals"]
                .as_array()
                .map_or([0; 2], |goals| [0, 1].map(|k| n(&goals[k]) as u8)),
            ball_holder: i["ball"].as_i64().map(|holder| holder as u32),
        };
        let count = n(&i["string_count"]) as i32;
        let [mul, add] = [0, 1].map(|k| n(&i["width"][k]) as i32);
        let resources = OverlayResources {
            string: |slot: i32| {
                Ok((0..=count).contains(&slot).then(|| match slot {
                    0x82 => b"%s team wins".to_vec(),
                    0x85 => b"Waiting".to_vec(),
                    0x89 => b"Press F10 to start".to_vec(),
                    0x8a => b"Still choosing team".to_vec(),
                    _ => format!("S{slot:03X}").into_bytes(),
                }))
            },
            width: |text: &[u8]| Ok((text.len() as i32).wrapping_mul(mul).wrapping_add(add)),
            style: |index: i32| {
                Ok((index as u32)
                    .wrapping_mul(0x9e37_79b1)
                    .wrapping_add(0x7f4a_7c15))
            },
        };
        let expected: Vec<OverlayOp> = case["ops"]
            .as_array()
            .unwrap()
            .iter()
            .map(|op| match op["text"].as_str() {
                Some(text) => OverlayOp::Text {
                    text: hex(text),
                    position: [n(&op["x"]) as i32, n(&op["y"]) as i32],
                    color_word: n(&op["style"]) as u32,
                },
                None => {
                    let corner = |k: usize| &op["quad"][k];
                    OverlayOp::Quad(DirectSpriteQuad {
                        sprite_slot: n(&op["sprite"]) as u16,
                        corners: [0, 1, 2, 3]
                            .map(|k| [n(&corner(k)[0]) as i16, n(&corner(k)[1]) as i16]),
                        vertex_depths: [0, 1, 2, 3].map(|k| n(&corner(k)[2]) as u32),
                        descriptor_word: n(&op["word"]) as u32,
                        mode: DirectSpriteMode::Textured,
                    })
                }
            })
            .collect();
        let mut ops = Vec::new();
        let result = plan_network_overlay(&inputs, &resources, &mut ops);
        assert!(result.is_ok(), "case {index}: {result:?}");
        assert_eq!(ops, expected, "case {index}");
    }
}

/// `0046C43C`: nothing while play runs; the host's winner blinks with its
/// restart line, which alone shows while both sides are idle; a guest waits.
#[test]
fn powerball_status_follows_the_goal_bytes() {
    use super::{powerball_status, PowerballStatus};
    assert_eq!(powerball_status([3, 2], true, 0), None);
    assert_eq!(
        powerball_status([0x83, 2], true, 50),
        Some(PowerballStatus::Wins(0x3a))
    );
    assert_eq!(
        powerball_status([3, 0x82], true, 150),
        Some(PowerballStatus::Wins(0x3b))
    );
    assert_eq!(
        powerball_status([3, 0x82], true, 100),
        Some(PowerballStatus::Line(PRESS_F10))
    );
    assert_eq!(
        powerball_status([0x80, 0x80], true, 50),
        Some(PowerballStatus::Line(PRESS_F10))
    );
    assert_eq!(
        powerball_status([0x83, 2], false, 50),
        Some(PowerballStatus::Line(WAITING))
    );
}
