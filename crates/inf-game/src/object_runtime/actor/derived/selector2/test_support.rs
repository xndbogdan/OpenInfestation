//! Shared helpers for the selector-2 receipt tests: the Level-1 world at a
//! terrain detail, the receipt word lookups and the control-block codec.

use serde_json::Value;

use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        actor::damage_reaction::ActorDamageReactionHost, ActorDamageCallback,
        ActorDamageReactionEffect, ActorHandle, ActorTimedDamageHost, ObjectArena, ObjectId,
        PlayerControlBlock, VehicleDamageHost,
    },
    process_particles::factories::ParticleFactoryProcessState,
    process_sound::{
        ProcessSoundRecordError, ProcessSoundRef, ProcessSoundVoiceRef, ProcessSoundVoiceTeardown,
    },
    world_backend::WorldSession,
    ResidentRuntime,
};

pub(super) fn loaded(
    selection: u32,
    spacing: crate::terrain_runtime::TerrainSpacing,
) -> (ResidentRuntime, FrontendAssets, WorldSession) {
    crate::test_worlds::offline_world(selection, spacing)
}

pub(super) struct NoVoices;
impl ProcessSoundVoiceTeardown for NoVoices {
    fn delete_voice(
        &mut self,
        _: ProcessSoundRef,
        _: ProcessSoundVoiceRef,
    ) -> Result<(), ProcessSoundRecordError> {
        Ok(())
    }
}

pub(super) struct Receipt<'a> {
    pub(super) offsets: &'a [Value],
    pub(super) values: &'a [Value],
}

impl Receipt<'_> {
    pub(super) fn dword(&self, offset: u32) -> u32 {
        let index = self
            .offsets
            .iter()
            .position(|value| value.as_u64() == Some(u64::from(offset)))
            .unwrap_or_else(|| panic!("offset {offset:x} not captured"));
        self.values[index].as_u64().unwrap() as u32
    }
    pub(super) fn low(&self, offset: u32) -> u16 {
        self.dword(offset) as u16
    }
    pub(super) fn high(&self, offset: u32) -> u16 {
        (self.dword(offset) >> 16) as u16
    }
}

pub(super) fn words(value: &Value) -> Vec<u32> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_u64().unwrap() as u32)
        .collect()
}

pub(super) fn controls_from(words: &[u32]) -> PlayerControlBlock {
    let mut bytes_38 = [0u8; 16];
    for index in 0..4 {
        bytes_38[index * 4..index * 4 + 4].copy_from_slice(&words[13 + index].to_le_bytes());
    }
    let [analog_04, b5, b6, b7] = words[0].to_le_bytes();
    PlayerControlBlock {
        analog_04,
        bytes_05_07: [b5, b6, b7],
        axes_08: [words[1] as i32, words[2] as i32, words[3] as i32],
        turn_14: words[4] as i32,
        pitch_18: words[5] as i32,
        thrust_1c: words[6] as i32,
        fire_20: words[7] as i32,
        grenade_24: words[8] as i32,
        dword_28: words[9],
        dword_2c: words[10],
        dword_30: words[11],
        dword_34: words[12],
        bytes_38,
    }
}

pub(super) fn controls_words(controls: &PlayerControlBlock) -> Vec<u32> {
    let mut words = vec![
        u32::from_le_bytes([
            controls.analog_04,
            controls.bytes_05_07[0],
            controls.bytes_05_07[1],
            controls.bytes_05_07[2],
        ]),
        controls.axes_08[0] as u32,
        controls.axes_08[1] as u32,
        controls.axes_08[2] as u32,
        controls.turn_14 as u32,
        controls.pitch_18 as u32,
        controls.thrust_1c as u32,
        controls.fire_20 as u32,
        controls.grenade_24 as u32,
        controls.dword_28,
        controls.dword_2c,
        controls.dword_30,
        controls.dword_34,
    ];
    for index in 0..4 {
        words.push(u32::from_le_bytes(
            controls.bytes_38[index * 4..index * 4 + 4]
                .try_into()
                .unwrap(),
        ));
    }
    words
}

/// The vehicle's damage services for tests: Infest01's difficulty (256),
/// the process NULL, and recorded sounds, flashes and reaction effects.
pub(super) struct DamageServices {
    pub(super) process: ParticleFactoryProcessState,
    pub(super) network: bool,
    /// The animation clock; None answers an error.
    pub(super) clock: Option<u32>,
    pub(super) selector70: bool,
    pub(super) events: Vec<DamageEvent>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum DamageEvent {
    Sound { position: [u16; 3], volume: u32 },
    Flash { position: [u16; 3], style: u32 },
    Effect(String),
}

impl DamageServices {
    pub(super) fn new(null: ActorHandle) -> Self {
        let mut process = ParticleFactoryProcessState::native_startup();
        process.process_null_handle = Some(null);
        Self {
            process,
            network: false,
            clock: None,
            selector70: false,
            events: Vec::new(),
        }
    }
}

impl ActorDamageReactionHost for DamageServices {
    type Error = &'static str;
    type RecentSource = ObjectId;
    fn process_null_handle(&self) -> Option<ActorHandle> {
        self.process.process_null_handle
    }
    fn network_active(&mut self) -> Result<bool, Self::Error> {
        Ok(self.network)
    }
    /// Infest01's difficulty: its deep-water 403 took 403 health.
    fn difficulty_multiplier(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<i16, Self::Error> {
        Ok(256)
    }
    fn source_bonuses(
        &mut self,
        _: &mut ObjectArena,
        _: ActorHandle,
    ) -> Result<Option<[i8; 6]>, Self::Error> {
        Err("source bonuses")
    }
    fn animation_tick(&mut self) -> Result<u32, Self::Error> {
        self.clock.ok_or("animation tick")
    }
    fn network_role(&mut self) -> Result<i32, Self::Error> {
        Err("network role")
    }
    fn local_player_byte(&mut self) -> Result<u8, Self::Error> {
        Err("local player byte")
    }
    fn excludes_status(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<bool, Self::Error> {
        Ok(false)
    }
    fn recent_damage_source(
        &mut self,
        _: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<Option<ObjectId>, Self::Error> {
        Ok(Some(actor))
    }
    fn recent_source_tick(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<u32, Self::Error> {
        Err("recent source tick")
    }
    fn recent_source_owner_word(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<u16, Self::Error> {
        Err("recent source owner")
    }
    fn effect(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        effect: ActorDamageReactionEffect,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.events.push(DamageEvent::Effect(format!("{effect:?}")));
        Ok(())
    }
}

impl VehicleDamageHost for DamageServices {
    fn player_process(&self) -> &ParticleFactoryProcessState {
        &self.process
    }
    fn collision_sound(&mut self, position: [u16; 3], volume: u32) -> Result<(), Self::Error> {
        self.events.push(DamageEvent::Sound { position, volume });
        Ok(())
    }
    fn selector70_alive(&mut self, _: &ObjectArena) -> Result<bool, Self::Error> {
        Ok(self.selector70)
    }
    fn hit_flash(
        &mut self,
        _: &mut ObjectArena,
        position: [u16; 3],
        style: u32,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.events.push(DamageEvent::Flash { position, style });
        Ok(())
    }
}

/// `0047C7C0` reaching the vehicle's `004212F0`.
impl ActorTimedDamageHost for DamageServices {
    type Error = String;
    fn virtual_b4(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        callback: ActorDamageCallback,
    ) -> Result<(), String> {
        arena
            .react_vehicle_damage(actor, callback, &mut 0, self)
            .map_err(|error| error.to_string())
    }
}
