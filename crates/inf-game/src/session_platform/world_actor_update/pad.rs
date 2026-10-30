//! The save pad's update `00475420`: its chimes through the process sounds
//! and the spawn save `0046AF80` at the animation clock.

use inf_formats::native_basis::NativeBasisMath;

use crate::{
    object_runtime::{ObjectArena, PlayerId, Selector58Host, PAD_SOUND},
    process_particles::audio::ParticleSoundRequest,
    process_sound::ProcessSoundRuntime,
    ResidentRuntime,
};

pub(crate) struct WorldPadHost<'a> {
    pub residency: &'a ResidentRuntime,
    pub sounds: &'a mut ProcessSoundRuntime,
    /// The animation clock `005471C0`.
    pub clock: u32,
    pub math: &'a NativeBasisMath,
    /// `0053FF00`, which `00491E80`'s third angle reads.
    pub inverse_sine: &'a [i16; 1024],
}

impl Selector58Host for WorldPadHost<'_> {
    type Error = String;

    fn chime(&mut self, position: [u16; 3], pitch: u32) -> Result<(), String> {
        self.sounds
            .enqueue(
                self.residency,
                ParticleSoundRequest {
                    sound: PAD_SOUND,
                    position,
                    volume: 0x10000,
                    pitch,
                    flags: 0,
                },
            )
            .map_err(|error| format!("00487EC0 pad chime: {error:?}"))
    }

    fn save_spawn(&mut self, arena: &mut ObjectArena, player: PlayerId) -> Result<(), String> {
        arena
            .save_player_spawn(player, self.clock, self.math, self.inverse_sine)
            .map_err(|error| format!("0046AF80 spawn save: {error}"))
    }
}
