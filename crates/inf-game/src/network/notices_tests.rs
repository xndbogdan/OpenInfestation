use super::*;

fn loan_state() -> (GameNetState, GameFrameState) {
    let mut game = GameNetState::default();
    game.present[1] = 1;
    game.present[2] = 1;
    game.names[1][..4].copy_from_slice(b"host");
    game.names[2][..5].copy_from_slice(b"guest");
    let frame = GameFrameState {
        kill_lines: vec![b"@1 fell to @2".to_vec()],
        die_lines: vec![b"@1 died".to_vec()],
        selfkill_lines: vec![b"@1 blew up".to_vec()],
        ..GameFrameState::default()
    };
    (game, frame)
}

/// 0045DA50: the attacker is kept for 0.75 s; a guest sends the packet
/// (27), the host moves the score itself.
#[test]
fn a_hit_remembers_its_attacker_and_moves_the_score() {
    let (mut game, mut frame) = loan_state();
    let mut loan = NetworkGameLoan {
        game: &mut game,
        frame: &mut frame,
    };
    let send = loan.damage_notice([10, 0, 2, 1]);
    assert_eq!(
        send,
        Some((27, GameSend::Dword(u32::from_le_bytes([10, 0, 2, 1]))))
    );
    assert_eq!((frame.attacker_830, frame.countdown_82c), (1, 750_000));
    game.host = true;
    game.scores[2][2] = 50;
    let mut loan = NetworkGameLoan {
        game: &mut game,
        frame: &mut frame,
    };
    assert_eq!(loan.damage_notice([10, 0, 2, 1]), None);
    // Behind the loser, the winner takes double, and the loser loses that.
    assert_eq!((game.scores[1][2], game.scores[2][2]), (20, 30));
}

/// 0045D8F0 on a guest: an unknown killer is the attacker still
/// remembered, the kill word counts the death and names the killer, the
/// kill group's line goes out (26, then 31) and to the first player.
#[test]
fn a_death_names_its_killer_and_tells_everyone() {
    let (mut game, mut frame) = loan_state();
    game.kill_word = 0x0005_0003;
    frame.countdown_82c = 1;
    frame.attacker_830 = 1;
    let mut rng = 1;
    let notice = NetworkGameLoan {
        game: &mut game,
        frame: &mut frame,
    }
    .death_notice([0, 0, 2, 0], &mut rng)
    .unwrap();
    assert_eq!(game.kill_word, 0x0001_0004);
    let line = b"guest fell to host".to_vec();
    assert_eq!(
        notice,
        DeathNotice {
            sends: vec![(26, GameSend::State), (31, GameSend::Text(line.clone()))],
            line: Some(line),
        }
    );
    assert_ne!(rng, 1);
}

/// Without the attacker it is the death group; in Powerball no line.
#[test]
fn a_lone_death_uses_the_death_group_and_powerball_says_nothing() {
    let (mut game, mut frame) = loan_state();
    let mut rng = 1;
    let notice = NetworkGameLoan {
        game: &mut game,
        frame: &mut frame,
    }
    .death_notice([0, 0, 2, 0], &mut rng)
    .unwrap();
    assert_eq!(notice.line, Some(b"guest died".to_vec()));
    game.game_mode = 4;
    let notice = NetworkGameLoan {
        game: &mut game,
        frame: &mut frame,
    }
    .death_notice([0, 0, 2, 2], &mut rng)
    .unwrap();
    assert_eq!(
        notice,
        DeathNotice {
            sends: vec![(26, GameSend::State)],
            line: None
        }
    );
}
