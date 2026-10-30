//! Native briefing gates and authored record timeline (`0040C6A0`,
//! `0040C770`, `0040CE40`, `0040D1E0/320/3D0`).
//!
//! The zero-record intro briefing exits immediately. An authored briefing
//! advances its records, then opens the list menu; only the native skip gates
//! or the armed Start timer request gameplay. The screen's camera vector uses
//! source-backed object/terrain inputs, separately from the world's camera.

use inf_formats::{
    BriefingCamera, BriefingModel, BriefingPayload, BriefingProgram, BriefingRecord,
};

use crate::briefing_camera::{BriefingCameraInputs, BriefingCameraState, BriefingCameraTick};
use crate::name_entry::name_entry_rng_next;
use crate::scheduler::SchedulerTick;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BriefingGates {
    /// Current mission's +3C DWORD, selected using runtime +A0.
    pub mission_status: u32,
    /// Process byte 0055F14C.
    pub attract_mode: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BriefingFinishReason {
    NoRecords,
    MissionStatusBit8,
    MissionStatusBit2,
    AttractMode,
    StartTimer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BriefingMode {
    Opening,
    Detail,
    List,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BriefingPanel {
    Map,
    Terrain,
    Model,
    Text,
    Header,
}

impl BriefingPanel {
    const fn index(self) -> usize {
        match self {
            Self::Map => 0,
            Self::Terrain => 1,
            Self::Model => 2,
            Self::Text => 3,
            Self::Header => 4,
        }
    }
}

/// The briefing's five panels are `0040D6A0` panel objects.
pub use crate::panel_animation::PanelAnimation as BriefingPanelAnimation;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BriefingTickEffect {
    ResumeWorldMusic,
    PlayCue {
        cue: u32,
    },
    TickTextController {
        delta_micros: i32,
    },
    /// 00439560 after item/camera updates, before the player controller tick.
    AdvanceSharedAnimation {
        delta_micros: u32,
    },
    RevealListRows {
        rows: u8,
    },
    RecordActivated {
        index: u32,
    },
    ShowList,
    /// Player controller `00413950`, before native list construction or close.
    ControllerCommand {
        command: u32,
        argument: u32,
    },
    /// Player controller `00413870`, only after the header reaches stage 2.
    TickController {
        delta_micros: i32,
    },
    /// Per-feature boundary: a native increment needs the prior text ID.
    UnownedTextIncrement {
        index: u32,
    },
    /// Unknown records retain their authored timing and skip only their effect.
    UnownedRecord {
        index: u32,
        raw_kind: u32,
    },
    /// A camera record/movement needs the loaded world's native inputs.
    UnownedCameraInput {
        field: &'static str,
    },
    Complete(BriefingFinishReason),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BriefingTickOutcome {
    pub effects: Vec<BriefingTickEffect>,
    pub finish_reason: Option<BriefingFinishReason>,
}

/// Policy state corresponding to native screen fields; resource IDs and
/// original record payloads retain their own namespaces.
#[derive(Clone, Debug)]
pub struct BriefingRuntime {
    program: BriefingProgram,
    mode: BriefingMode,
    record_index: i32,
    record_age_millis: u32,
    screen_age_millis: i32,
    start_remaining_millis: u32,
    music_resumed: bool,
    draw_enabled: bool,
    finished: Option<BriefingFinishReason>,
    panels: [BriefingPanelAnimation; 5],
    menu_text_id: Option<u16>,
    model: Option<BriefingModel>,
    model_age_millis: i32,
    camera: Option<BriefingCamera>,
    camera_heading: i32,
    camera_heading_target: i32,
    camera_state: BriefingCameraState,
    terrain_camera_disabled: bool,
    map_scale_word: u16,
    map_scale_target_word: u16,
    pulse_remaining_micros: u32,
    pulse_pause_micros: u32,
    list_rows: u8,
    list_closed: bool,
    /// How long the list shows before its cue and Next: retail's 5000 ms,
    /// two seconds past its fourth row.
    list_close_millis: u32,
}

/// Retail's list closes this long after it opens (`0040C770`).
pub const LIST_CLOSE_MILLIS: u32 = 5000;

impl BriefingRuntime {
    pub fn new(program: BriefingProgram) -> Self {
        Self {
            program,
            mode: BriefingMode::Opening,
            record_index: -1,
            record_age_millis: 0,
            screen_age_millis: 0,
            start_remaining_millis: 0,
            music_resumed: false,
            draw_enabled: false,
            finished: None,
            panels: [
                BriefingPanelAnimation::new(0),
                BriefingPanelAnimation::new(0),
                BriefingPanelAnimation::new(0),
                BriefingPanelAnimation::new(0x2800),
                BriefingPanelAnimation::new(0x3000),
            ],
            // Constructor has no writer for +2A0. The first explicit text
            // record supplies it; increment-only without that is unowned.
            menu_text_id: None,
            model: None,
            model_age_millis: 0,
            camera: None,
            camera_heading: 0,
            camera_heading_target: 0,
            camera_state: BriefingCameraState::default(),
            terrain_camera_disabled: false,
            map_scale_word: 0,
            map_scale_target_word: 0,
            pulse_remaining_micros: 0,
            pulse_pause_micros: 0,
            list_rows: 0,
            list_closed: false,
            list_close_millis: LIST_CLOSE_MILLIS,
        }
    }

    /// A port briefing whose list holds fewer rows than retail's four
    /// closes it as long after its last row as retail's does after its
    /// fourth (Pelagia's), rather than leaving empty rows on screen.
    pub fn with_list_close(mut self, millis: u32) -> Self {
        self.list_close_millis = millis;
        self
    }

    pub fn program(&self) -> &BriefingProgram {
        &self.program
    }
    pub fn music_resume_pending(&self) -> bool {
        !self.music_resumed
    }
    /// The screen performs 004872A0 before base input, then latches +F5.
    pub fn acknowledge_music_resume(&mut self) {
        self.music_resumed = true;
    }
    pub fn mode(&self) -> BriefingMode {
        self.mode
    }
    pub fn current_record(&self) -> Option<&BriefingRecord> {
        self.program
            .records
            .get(usize::try_from(self.record_index).ok()?)
    }
    pub fn record_age_millis(&self) -> u32 {
        self.record_age_millis
    }
    pub fn screen_age_millis(&self) -> i32 {
        self.screen_age_millis
    }
    pub fn start_remaining_millis(&self) -> u32 {
        self.start_remaining_millis
    }
    /// Native screen +2A0 predecessor, last written by a kind-0 record.
    /// The display scratch 0052F018 is separately cleared on Detail replay.
    pub fn menu_text_id(&self) -> Option<u16> {
        self.menu_text_id
    }
    pub fn model(&self) -> Option<&BriefingModel> {
        self.model.as_ref()
    }
    pub fn model_age_millis(&self) -> i32 {
        self.model_age_millis
    }
    pub fn camera(&self) -> Option<&BriefingCamera> {
        self.camera.as_ref()
    }
    pub fn camera_heading(&self) -> i32 {
        self.camera_heading
    }
    pub fn camera_heading_target(&self) -> i32 {
        self.camera_heading_target
    }
    pub fn camera_position_words(&self) -> [u16; 3] {
        self.camera_state.current_words()
    }
    pub fn camera_target_xz_words(&self) -> [u16; 2] {
        self.camera_state.target_xz_words()
    }
    pub fn terrain_camera_disabled(&self) -> bool {
        self.terrain_camera_disabled
    }
    pub fn map_scale_word(&self) -> u16 {
        self.map_scale_word
    }
    pub fn map_scale_target_word(&self) -> u16 {
        self.map_scale_target_word
    }
    pub fn pulse_remaining_micros(&self) -> u32 {
        self.pulse_remaining_micros
    }
    pub fn pulse_pause_micros(&self) -> u32 {
        self.pulse_pause_micros
    }
    /// 0040C257 writes +1CC after drawing the genuine camera-disabled pane.
    /// Its following item tick decrements this same timer; drawing an enabled
    /// or closed pane must not refresh it.
    pub fn camera_disabled_drawn(&mut self) {
        self.pulse_remaining_micros = 250_000;
    }
    pub fn list_rows(&self) -> u8 {
        self.list_rows
    }
    pub fn list_closed(&self) -> bool {
        self.list_closed
    }
    pub fn visibility(&self, panel: BriefingPanel) -> BriefingPanelAnimation {
        self.panels[panel.index()]
    }
    pub fn finished(&self) -> Option<BriefingFinishReason> {
        self.finished
    }
    /// Screen +F1 starts at one and only C770 clears it. Empty/skip gate
    /// completions never admit the briefing's visible draw body.
    pub fn draw_enabled(&self) -> bool {
        self.draw_enabled
    }

    /// Constructor tail checks the mission bits, without the tick-only
    /// zero-record or attract checks. The host applies completion effects.
    pub fn constructor_finish_reason(gates: BriefingGates) -> Option<BriefingFinishReason> {
        if gates.mission_status as u8 & 2 != 0 {
            Some(BriefingFinishReason::MissionStatusBit2)
        } else if gates.mission_status as u8 & 8 != 0 {
            Some(BriefingFinishReason::MissionStatusBit8)
        } else {
            None
        }
    }

    pub fn tick_finish_reason(&self, gates: BriefingGates) -> Option<BriefingFinishReason> {
        if self.program.records.is_empty() {
            Some(BriefingFinishReason::NoRecords)
        } else if gates.mission_status as u8 & 8 != 0 {
            Some(BriefingFinishReason::MissionStatusBit8)
        } else if gates.mission_status as u8 & 2 != 0 {
            Some(BriefingFinishReason::MissionStatusBit2)
        } else if gates.attract_mode {
            Some(BriefingFinishReason::AttractMode)
        } else {
            None
        }
    }

    /// `0040D320`: keyboard shortcut and Start menu row share one one-shot
    /// two-second timer. It does not restart on repeated activation.
    pub fn arm_start(&mut self) -> bool {
        if self.start_remaining_millis != 0 || self.finished.is_some() {
            return false;
        }
        self.start_remaining_millis = 2000;
        for panel in &mut self.panels {
            panel.request(0);
        }
        true
    }

    /// Detail row `0040D1E0`; the menu host owns its push operation.
    pub fn show_detail(&mut self) {
        self.record_index = -1;
        // 0040D1E0 clears displayed 0052F018 through the menu owner, but
        // retains screen +2A0 for a later increment-only kind-0 record.
        for panel in [
            BriefingPanel::Map,
            BriefingPanel::Text,
            BriefingPanel::Terrain,
            BriefingPanel::Model,
        ] {
            self.panels[panel.index()].request(0);
        }
        self.record_age_millis = 0;
        self.pulse_remaining_micros = 0;
        self.pulse_pause_micros = 0;
        self.terrain_camera_disabled = false;
        self.model = None;
        self.model_age_millis = 0;
        self.camera = None;
        self.map_scale_word = 0;
        self.map_scale_target_word = 0;
        self.camera_heading_target = 0;
        self.camera_heading = 0;
        self.mode = BriefingMode::Detail;
        self.list_closed = false;
        self.list_rows = 0;
    }

    /// List row `0040D3D0`; it retains camera/model inputs just as native.
    pub fn show_list(&mut self) {
        self.mode = BriefingMode::List;
        self.record_age_millis = 0;
        self.list_rows = 0;
        self.panels[BriefingPanel::Terrain.index()].request(0);
        self.panels[BriefingPanel::Model.index()].request(0);
        self.map_scale_target_word = 0;
    }

    pub fn tick(
        &mut self,
        tick: SchedulerTick,
        gates: BriefingGates,
        rng: &mut u32,
        camera_inputs: Option<BriefingCameraInputs<'_>>,
    ) -> BriefingTickOutcome {
        let mut outcome = BriefingTickOutcome::default();
        if self.finished.is_some() {
            return outcome;
        }
        let finish = self.tick_finish_reason(gates);
        if let Some(reason) = finish {
            self.complete(reason, &mut outcome);
            return outcome;
        }
        if !self.music_resumed {
            self.music_resumed = true;
            outcome.effects.push(BriefingTickEffect::ResumeWorldMusic);
        }
        // Base `004392B0` advances +C0 from the unclamped second argument.
        self.screen_age_millis = self
            .screen_age_millis
            .wrapping_add(tick.elapsed_micros / 1000);
        // Constructor +F0=1: `0040C6A0` supplies trunc(delta * 7 / 8).
        let delta = tick.delta_micros.wrapping_mul(7) / 8;
        self.tick_item(delta, rng, camera_inputs, &mut outcome);
        outcome
    }

    fn tick_item(
        &mut self,
        delta: i32,
        rng: &mut u32,
        camera_inputs: Option<BriefingCameraInputs<'_>>,
        outcome: &mut BriefingTickOutcome,
    ) {
        self.draw_enabled = true;
        for panel in &mut self.panels {
            panel.tick(delta);
        }
        outcome
            .effects
            .push(BriefingTickEffect::TickTextController {
                delta_micros: delta,
            });
        let retreat = if let Some(camera) = self.camera {
            if let Some(inputs) = camera_inputs {
                let motion = self.camera_state.tick(
                    BriefingCameraTick {
                        record: camera,
                        heading: self.camera_heading,
                        delta_micros: delta,
                        map_scale_word: self.map_scale_word,
                        terrain_camera_disabled: self.terrain_camera_disabled,
                    },
                    inputs,
                );
                self.terrain_camera_disabled = motion.terrain_camera_disabled;
                if motion.restart_pulse {
                    self.pulse_remaining_micros = 500_000;
                }
                motion.retreat_map_scale
            } else {
                outcome
                    .effects
                    .push(BriefingTickEffect::UnownedCameraInput {
                        field: "movement object/terrain inputs",
                    });
                false
            }
        } else {
            false
        };
        if retreat {
            self.map_scale_word = i32::from(self.map_scale_word)
                .wrapping_sub(delta >> 5)
                .max(0x10) as u16;
        } else {
            self.tick_map_scale(delta);
        }
        self.tick_pulse(delta as u32, rng);
        let millis = delta / 1000;
        self.model_age_millis = self.model_age_millis.wrapping_add(millis);

        if self.start_remaining_millis != 0 {
            if self.start_remaining_millis > (delta as u32) / 1000 {
                self.start_remaining_millis =
                    self.start_remaining_millis.wrapping_sub(millis as u32);
            } else {
                // Native leaves +F8 at 1 after requesting phase 4.
                self.start_remaining_millis = 1;
                self.complete(BriefingFinishReason::StartTimer, outcome);
            }
        } else if self.mode == BriefingMode::Opening {
            if self.panels[BriefingPanel::Header.index()].current_stage == 0 {
                self.panels[BriefingPanel::Header.index()].request(2);
            }
            if self.screen_age_millis > 1000 {
                self.mode = BriefingMode::Detail;
            }
        }

        if self.mode == BriefingMode::Detail && self.start_remaining_millis == 0 {
            self.record_age_millis = self.record_age_millis.wrapping_add(millis as u32);
            let advance = self.current_record().map_or(true, |record| {
                self.record_age_millis >= record.duration_100ms.wrapping_mul(100)
            });
            // Native advances once per item tick; zero-duration records are
            // never all drained by one call and surplus time is discarded.
            if advance {
                self.advance_record(camera_inputs, outcome);
            }
        } else if self.mode == BriefingMode::List {
            self.record_age_millis = self.record_age_millis.wrapping_add(millis as u32);
            self.list_rows = (self.record_age_millis / 1000 + 1).min(4) as u8;
            outcome.effects.push(BriefingTickEffect::RevealListRows {
                rows: self.list_rows,
            });
            if self.record_age_millis > self.list_close_millis && !self.list_closed {
                outcome.effects.push(BriefingTickEffect::PlayCue { cue: 2 });
                outcome.effects.push(BriefingTickEffect::ControllerCommand {
                    command: 1,
                    argument: 0,
                });
                self.list_closed = true;
            }
        }
        self.tick_heading(delta);
        outcome
            .effects
            .push(BriefingTickEffect::AdvanceSharedAnimation {
                delta_micros: delta as u32,
            });
        let header = self.panels[BriefingPanel::Header.index()];
        if header.current_stage == 2 && header.requested_stage == 2 {
            outcome.effects.push(BriefingTickEffect::TickController {
                delta_micros: delta,
            });
        }
    }

    fn complete(&mut self, reason: BriefingFinishReason, outcome: &mut BriefingTickOutcome) {
        self.finished = Some(reason);
        outcome.finish_reason = Some(reason);
        outcome.effects.push(BriefingTickEffect::Complete(reason));
    }

    fn advance_record(
        &mut self,
        camera_inputs: Option<BriefingCameraInputs<'_>>,
        outcome: &mut BriefingTickOutcome,
    ) {
        self.record_index = self.record_index.wrapping_add(1);
        let Some(record) = self.current_record().cloned() else {
            outcome.effects.push(BriefingTickEffect::PlayCue { cue: 2 });
            outcome.effects.push(BriefingTickEffect::ControllerCommand {
                command: 1,
                argument: 0,
            });
            self.show_list();
            outcome.effects.push(BriefingTickEffect::ShowList);
            return;
        };
        self.record_age_millis = 0;
        outcome.effects.push(BriefingTickEffect::RecordActivated {
            index: record.index,
        });
        match record.payload {
            BriefingPayload::MenuText {
                string_id,
                increment_bytes,
            } => {
                if increment_bytes == [0, 0] {
                    self.menu_text_id = Some(string_id);
                } else if let Some(previous) = self.menu_text_id {
                    self.menu_text_id = Some(previous.wrapping_add(1));
                } else {
                    outcome
                        .effects
                        .push(BriefingTickEffect::UnownedTextIncrement {
                            index: record.index,
                        });
                }
            }
            BriefingPayload::Visibility { flags } => {
                let panel = match record.raw_kind {
                    1 => BriefingPanel::Terrain,
                    3 => BriefingPanel::Map,
                    4 => BriefingPanel::Model,
                    5 => BriefingPanel::Text,
                    _ => {
                        outcome.effects.push(BriefingTickEffect::UnownedRecord {
                            index: record.index,
                            raw_kind: record.raw_kind,
                        });
                        return;
                    }
                };
                self.panels[panel.index()].request(visibility_stage(flags));
                if panel == BriefingPanel::Model {
                    self.model_age_millis = 0;
                }
            }
            BriefingPayload::Model(model) => {
                self.model = Some(model);
                self.model_age_millis = 0;
            }
            BriefingPayload::Terrain { camera_disabled } => {
                self.terrain_camera_disabled = camera_disabled != 0
            }
            BriefingPayload::Camera(camera) => {
                if camera.heading_degrees != 0 {
                    let degrees = i32::from(camera.heading_degrees);
                    self.camera_heading_target =
                        degrees.wrapping_mul(0x1_0000) / 360 + degrees / 0x8001;
                }
                if camera.actor_id != 0 {
                    // A stand-off (Pelagia's program) looks along the
                    // heading this record turns to.
                    let heading = if camera.heading_degrees != 0 {
                        self.camera_heading_target
                    } else {
                        self.camera_heading
                    };
                    match camera_inputs {
                        Some(inputs) => {
                            if self
                                .camera_state
                                .activate_standing_off(
                                    camera.actor_id,
                                    heading,
                                    inputs.math,
                                    inputs.world.arena,
                                )
                                .is_err()
                            {
                                outcome
                                    .effects
                                    .push(BriefingTickEffect::UnownedCameraInput {
                                        field: "target object position",
                                    });
                            }
                        }
                        None => outcome
                            .effects
                            .push(BriefingTickEffect::UnownedCameraInput {
                                field: "target object list",
                            }),
                    }
                }
                self.camera = Some(camera);
            }
            BriefingPayload::Pulse => self.pulse_remaining_micros = 500_000,
            BriefingPayload::MapScale { word } => self.map_scale_target_word = word.wrapping_shl(9),
            BriefingPayload::Unknown => {
                outcome.effects.push(BriefingTickEffect::UnownedRecord {
                    index: record.index,
                    raw_kind: record.raw_kind,
                });
            }
        }
    }

    fn tick_map_scale(&mut self, delta: i32) {
        let amount = delta >> 5;
        let current = i32::from(self.map_scale_word);
        let target = i32::from(self.map_scale_target_word);
        self.map_scale_word = if current < target {
            current.wrapping_add(amount).min(target)
        } else if current > target {
            current.wrapping_sub(amount).max(target)
        } else {
            current
        } as u16;
    }

    fn tick_pulse(&mut self, delta: u32, rng: &mut u32) {
        if self.pulse_remaining_micros == 0 {
            if self.pulse_pause_micros <= delta {
                self.pulse_remaining_micros = 150_000 + (name_entry_rng_next(rng) & 0xffff) * 2;
            } else {
                self.pulse_pause_micros -= delta;
            }
        } else if self.pulse_remaining_micros > delta {
            self.pulse_remaining_micros -= delta;
        } else {
            self.pulse_remaining_micros = 0;
            self.pulse_pause_micros = ((name_entry_rng_next(rng) & 0xffff) + 62_500) * 32;
        }
    }

    fn tick_heading(&mut self, delta: i32) {
        let Some(camera) = self.camera else {
            return;
        };
        if camera.rotation_word == 0 {
            return;
        }
        // IMUL then SHL/RCL: signed product >>31, with wrapping operands.
        let change = ((i64::from(delta.wrapping_shl(13))
            * i64::from(i32::from(camera.rotation_word).wrapping_shl(5)))
            >> 31) as i32;
        let remaining =
            i32::from((self.camera_heading_target as u16).wrapping_sub(self.camera_heading as u16));
        if self.camera_heading_target == 0 || change.wrapping_abs() < remaining {
            self.camera_heading = self.camera_heading.wrapping_add(change);
        } else {
            self.camera_heading = self.camera_heading_target;
        }
    }
}

/// `0040CE40` tests bit 2 before bit 4 before bit 8.
pub const fn visibility_stage(flags: u32) -> u8 {
    if flags & 2 != 0 {
        0
    } else if flags & 4 != 0 {
        1
    } else {
        ((flags & 8) >> 2) as u8
    }
}

#[cfg(test)]
#[path = "briefing_runtime_tests.rs"]
mod tests;
