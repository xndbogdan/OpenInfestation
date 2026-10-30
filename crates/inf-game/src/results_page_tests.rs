use std::convert::Infallible;
use std::path::Path;

use inf_formats::MissionSaveState;

use super::*;
use crate::{
    frontend_assets::FrontendAssets, frontend_display::frontend_startup,
    render_resources::RenderResources, world_mission::WorldMissionInputs, Installation,
    ResidencyRequest, ResidentRuntime, ResourceSelection,
};

/// One page draw's submissions: each text as (string, position, width,
/// reveal age), the scene commands by kind, and the direct sprites.
#[derive(Default)]
struct Recorder {
    texts: Vec<(String, [i32; 2], i32, Option<i32>)>,
    /// 004A3AB0's lines: string, position and colour.
    lines: Vec<(String, [i32; 2], MenuColor)>,
    /// Each setup's centre and fade, and each model's slot and basis.
    setups: Vec<([i32; 2], [i32; 2])>,
    models: Vec<(i16, MenuBasis)>,
    commands: Vec<&'static str>,
    sprites: Vec<DirectSpriteQuad>,
}

impl FrontendSceneHost for Recorder {
    type Error = Infallible;
    fn submit(&mut self, command: SceneCommand) -> Result<(), Self::Error> {
        match &command {
            SceneCommand::Setup(setup) => self.setups.push((setup.center, setup.fade)),
            SceneCommand::Model(model) => self.models.push((model.model_slot, model.basis)),
            _ => {}
        }
        self.commands.push(match command {
            SceneCommand::BeginBatch => "begin",
            SceneCommand::FlushBatch => "flush",
            SceneCommand::Model(submission) if submission.model_slot == MODEL => "model 98",
            SceneCommand::Model(submission) if submission.model_slot == FINAL_MODEL => "model 2B3",
            SceneCommand::Model(_) => "frame",
            _ => "state",
        });
        Ok(())
    }
    fn footer_style_word(&mut self) -> Result<u32, Self::Error> {
        unreachable!("the page draws no footer")
    }
    fn surface_pixels(&mut self) -> Result<[i32; 2], Self::Error> {
        unreachable!("the page passes its surface")
    }
    fn sprite_dimensions(&mut self, _: u16) -> Result<[u16; 2], Self::Error> {
        unreachable!("the page draws no footer")
    }
}

impl ResultsPageHost for Recorder {
    fn sprite(&mut self, quad: DirectSpriteQuad) -> Result<(), Self::Error> {
        self.sprites.push(quad);
        Ok(())
    }
    fn reset_renderer(&mut self) -> Result<(), Self::Error> {
        self.commands.push("reset");
        Ok(())
    }
    fn raw_text(&mut self, request: &mut RawMenuTextRequest<'_>) -> Result<(), Self::Error> {
        self.texts.push((
            String::from_utf8_lossy(request.text).into_owned(),
            request.position_pixels,
            request.width_pixels,
            match request.reveal {
                MenuTextReveal::Timed { age_millis } => Some(age_millis),
                MenuTextReveal::Complete => None,
            },
        ));
        Ok(())
    }
    fn line_text(&mut self, request: &mut LineTextRequest<'_>) -> Result<(), Self::Error> {
        self.lines.push((
            String::from_utf8_lossy(request.text).into_owned(),
            request.position_pixels,
            request.color,
        ));
        Ok(())
    }
}

/// The 1024x768 residency of Infest02's trace, with a world's level active
/// as after a world.
fn resident() -> (FrontendAssets, ResidentRuntime) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).expect("retail Infestation data required");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 4,
                language: 0,
            },
        })
        .unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    for level in [47, 48, 28] {
        runtime.apply(ResidencyRequest::Activate { level }).unwrap();
    }
    (assets, runtime)
}

/// Level 1 completed, as Infest02's results found it: eight kills, no
/// collateral damage and 1:47 of mission time.
fn level1_mission() -> WorldMission {
    let mut mission = WorldMission::new(
        MissionSaveState::new_game(),
        WorldMissionInputs {
            current_world_a0: 18,
            dword_1c: 0x2800,
            byte_13: None,
            record_0c: Some(Box::new([0; RECORD_0C_BYTES])),
        },
    );
    mission.save.owner_3c[18] |= 3;
    mission.bytes_14_16 = Some([0; 3]);
    mission.dword_38 = Some(107_500);
    mission.record_0c.as_deref_mut().unwrap()[0xa0] = 8;
    mission
}

fn open_panel(current_stage: u8) -> Option<PanelAnimation> {
    let mut panel = PanelAnimation::with_duration(0x6000, 500_000);
    panel.current_stage = current_stage;
    panel.requested_stage = 2;
    Some(panel)
}

/// The draw at Infest02's tick 19 (4EF2BE): all three panels open, the
/// medal awarded and the game saved.
fn saved_state() -> ResultsPageState {
    ResultsPageState {
        age_millis: 0x1c9e,
        elapsed_millis: 0x8ed,
        acams_lost: 1,
        medal: Some(2),
        save_countdown_millis: 0,
        saved: true,
        save_failed: false,
        profile_status: 3,
        exit_millis: 0,
        menu_shown: false,
        panels: [open_panel(2), open_panel(2), open_panel(2)],
        final_page: false,
    }
}

fn draw(state: ResultsPageState) -> Recorder {
    draw_with(state, None)
}

fn draw_with(state: ResultsPageState, credits: Option<&FinalCredits>) -> Recorder {
    let (assets, runtime) = resident();
    let resources = RenderResources::new(&runtime);
    let startup = frontend_startup(
        &runtime,
        crate::presentation::NativeFrame::original([1024, 768]),
    )
    .unwrap();
    let mission = level1_mission();
    let page = ResultsPage {
        state,
        mission: &mission,
        rank_word: Some(2),
        tokens: 71,
        surface_pixels: [1024, 768],
        projection: SceneProjection {
            viewport: [1024, 768],
            ..startup.projection
        },
        animation: AnimationClock::default(),
        selector_one: 0,
        glyph_base_color: MenuColor::from_word(crate::menu_text::INITIAL_GLYPH_BASE_WORD),
        credits,
    };
    let mut recorder = Recorder::default();
    draw_results_page(
        &page,
        &resources,
        &MenuBasisMath::new(assets.sine_table().values),
        &mut recorder,
    )
    .unwrap();
    recorder
}

fn texts(expected: &[(&str, [i32; 2], i32, i32)]) -> Vec<(String, [i32; 2], i32, Option<i32>)> {
    expected
        .iter()
        .map(|&(text, position, width, age)| (text.to_owned(), position, width, Some(age)))
        .collect()
}

/// Infest02's draw at 4EF2BE, text for text: right-aligned at 931 in steps
/// of 38, the status line blinked off (age 7326), "Game Saved" wrapped at
/// 819 and the prompt at 512. Model 98 twice and the three frames share one
/// batch after the background.
#[test]
fn infest02_saved_page_matches_the_trace() {
    let recorder = draw(saved_state());
    assert_eq!(
        recorder.texts,
        texts(&[
            ("Kills: 8", [0x356, 0x4c], 0x400, 0x178a),
            ("Collateral Damage: 0", [0x2c4, 0x72], 0x400, 0x165e),
            ("Duration: 01:47", [0x30b, 0x98], 0x400, 0x1532),
            ("Tokens remaining: 71", [0x2d8, 0xbe], 0x400, 0x1406),
            ("Medal awarded : Silver", [0x2b5, 0x195], 0x400, 0x1276),
            ("Overall Rank : Black", [0x2c1, 0x1bb], 0x400, 0x114a),
            ("ACAMs Lost: 1", [0x304, 0x1e1], 0x400, 0x101e),
            ("Game Saved", [0x328, 0x275], 0x333, 0x10e6),
            (
                "Press <Esc> or <Space> to continue",
                [0x21a, 0x29b],
                0x200,
                0xfba
            ),
        ])
    );
    assert_eq!(recorder.sprites.len(), 5);
    let batch: Vec<_> = recorder
        .commands
        .iter()
        .copied()
        .filter(|command| *command != "state")
        .collect();
    assert_eq!(
        batch,
        ["begin", "reset", "model 98", "model 98", "frame", "frame", "frame", "flush"]
    );
}

/// The draw at 4ED669 (age 5668): the save is in flight, so its line blinks
/// "Saving game... Please Wait", and the status line shows "Mission
/// Completed" two steps below panel 2's top (aliens line pair skipped).
#[test]
fn infest02_saving_page_matches_the_trace() {
    let recorder = draw(ResultsPageState {
        age_millis: 0x1624,
        elapsed_millis: 0x6f9,
        save_countdown_millis: 0x2ef,
        saved: false,
        ..saved_state()
    });
    assert_eq!(
        recorder.texts,
        texts(&[
            ("Kills: 8", [0x356, 0x4c], 0x400, 0x1110),
            ("Collateral Damage: 0", [0x2c4, 0x72], 0x400, 0xfe4),
            ("Duration: 01:47", [0x30b, 0x98], 0x400, 0xeb8),
            ("Tokens remaining: 71", [0x2d8, 0xbe], 0x400, 0xd8c),
            ("Mission Completed", [0x2ee, 0x16f], 0x400, 0xd28),
            ("Medal awarded : Silver", [0x2b5, 0x195], 0x400, 0xbfc),
            ("Overall Rank : Black", [0x2c1, 0x1bb], 0x400, 0xad0),
            ("ACAMs Lost: 1", [0x304, 0x1e1], 0x400, 0x9a4),
            ("Saving game... Please Wait", [0x296, 0x275], 0x333, 0xa6c),
            (
                "Press <Esc> or <Space> to continue",
                [0x21a, 0x29b],
                0x200,
                0x940
            ),
        ])
    );
}

/// The draw at 4EB0FC (age 1741): only panel 1 has opened, so only its four
/// lines draw, the later ones not yet revealed.
#[test]
fn infest02_first_panel_page_matches_the_trace() {
    let recorder = draw(ResultsPageState {
        age_millis: 0x6cd,
        elapsed_millis: 0x29d,
        medal: None,
        saved: false,
        panels: [open_panel(1), None, None],
        ..saved_state()
    });
    assert_eq!(
        recorder.texts,
        texts(&[
            ("Kills: 8", [0x356, 0x4c], 0x400, 0x1b9),
            ("Collateral Damage: 0", [0x2c4, 0x72], 0x400, 0x8d),
            ("Duration: 01:47", [0x30b, 0x98], 0x400, -159),
            ("Tokens remaining: 71", [0x2d8, 0xbe], 0x400, -459),
        ])
    );
    assert_eq!(
        recorder
            .commands
            .iter()
            .filter(|command| **command == "frame")
            .count(),
        1
    );
}

/// The draw at 4F0F24, 442 ms before the briefing: the panels have closed,
/// the planet has shrunk to 614 * 442 / 1000 = 271, and only the centred
/// exit line draws, revealed for 1000 - 442 ms.
#[test]
fn infest02_exit_page_matches_the_trace() {
    let mut closed = PanelAnimation::with_duration(0x6000, 500_000);
    closed.requested_stage = 0;
    let state = ResultsPageState {
        age_millis: 0x24bb,
        elapsed_millis: 0xc15,
        exit_millis: 0x1ba,
        panels: [Some(closed); 3],
        ..saved_state()
    };
    let recorder = draw(state);
    assert_eq!(
        recorder.texts,
        texts(&[("Stand by... Warp in progress", [0x16b, 0x180], 0x400, 0x22e)])
    );
    assert!(!recorder.commands.contains(&"frame"));
    let (assets, runtime) = resident();
    let resources = RenderResources::new(&runtime);
    let startup = frontend_startup(
        &runtime,
        crate::presentation::NativeFrame::original([1024, 768]),
    )
    .unwrap();
    assert_eq!(
        recorder.sprites,
        background_quads(
            [0x300, 0x180],
            0x10f,
            0xc15,
            startup.projection.focal,
            resources.style(BACKGROUND_STYLE).unwrap(),
            &MenuBasisMath::new(assets.sine_table().values),
        )
    );
}

#[test]
fn format_answers_the_page_directives_and_rejects_others() {
    assert_eq!(
        format(FORMAT_TIME, &[Arg::Int(1), Arg::Int(47)]).unwrap(),
        b"01:47"
    );
    assert_eq!(
        format(FORMAT_TIME, &[Arg::Int(-3), Arg::Int(5)]).unwrap(),
        b"-3:05"
    );
    assert_eq!(
        format(FORMAT_COUNT, &[Arg::Text(b"Kills\0junk"), Arg::Int(-8)]).unwrap(),
        b"Kills: -8"
    );
    assert_eq!(format(b"100%% done", &[]).unwrap(), b"100% done");
    assert!(format(b"%x", &[Arg::Int(1)]).is_err());
    assert!(format(FORMAT_COUNT, &[Arg::Text(b"Kills")]).is_err());
}

/// The final world's page 11.25 s in at 1024x768: no planet and no panels,
/// model 0x2B3 alone; the roll has moved 768 * 11250 / 14400 = 600 pixels,
/// so table entries 4 to 15 are on screen, the player's line (entry 10) at
/// 768 + 10 * 38 - 600 = 548 and centred; the saved line at h/10, centred,
/// and no prompt.
#[test]
fn the_final_worlds_page_rolls_the_credits() {
    let (assets, runtime) = resident();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let credits = FinalCredits::load(executable, b"Tester".to_vec()).unwrap();
    let recorder = draw_with(
        ResultsPageState {
            age_millis: 11250,
            final_page: true,
            panels: [None; 3],
            ..saved_state()
        },
        Some(&credits),
    );
    let batch: Vec<_> = recorder
        .commands
        .iter()
        .copied()
        .filter(|command| *command != "state")
        .collect();
    assert_eq!(batch, ["begin", "model 2B3", "flush"]);
    assert!(recorder.sprites.is_empty());
    // At (w/2, h/3), its fade 0xFFFF both ways, half a turn about X-Z less
    // a quarter of the sine of 4 * 11250 = 0xAFC8: the table's entry
    // 0x2FC8 >> 2, negated in the upper half-turn.
    assert_eq!(recorder.setups, [([512, 256], [0xffff, 0xffff])]);
    let sine = i32::from(assets.sine_table().values[0xbf2]);
    let math = MenuBasisMath::new(assets.sine_table().values);
    let mut basis = MenuBasis::INITIAL;
    math.rotate_in_plane(&mut basis, 0x8000 + ((-sine) >> 2), RotationPlane::Xz);
    assert_eq!(recorder.models, [(FINAL_MODEL, basis)]);
    let ys: Vec<i32> = recorder.lines.iter().map(|line| line.1[1]).collect();
    assert_eq!(ys, [320, 358, 396, 548, 586, 624]);
    let player = &recorder.lines[3];
    assert!(player.0.contains("Tester"), "{player:?}");
    let resources = RenderResources::new(&runtime);
    let font = resources.font(FONT).unwrap();
    let width = measure_text_width(font, player.0.as_bytes()).unwrap();
    assert_eq!(player.1[0], (1024 - width) / 2);
    // Below the bottom fade's start (616), entry 12 at 624 fades.
    let full = MenuColor::from_word(resources.style(5).unwrap());
    assert_eq!(player.2, full);
    assert_ne!(recorder.lines[5].2, full);
    assert_eq!(recorder.texts.len(), 1, "{:?}", recorder.texts);
    let (text, [x, y], wrap, age) = &recorder.texts[0];
    let measured = measure_text_width(font, text.as_bytes()).unwrap();
    assert_eq!(
        ([*x, *y], *wrap, *age),
        ([(1024 - measured) / 2, 76], 0x333, Some(11250 - 3000))
    );
}
