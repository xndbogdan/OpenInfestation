//! 00436D20 chooses the actual particle service, then returns AL1. Captured
//! burst XYZ and live factory XYZ deliberately retain their distinct custody.

use super::FurnitureCallbackContext;
use crate::{process_particles::births::ParticleBurst, process_sound::ProcessSoundRuntime};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FurnitureParticleEffect {
    Burst(ParticleBurst),
    Central { selector: u32 },
    Emitter { selector: u32 },
    TerrainFlash { selector: u32 },
}

pub struct FurnitureParticleCallbackInputs<'a> {
    pub context: &'a mut FurnitureCallbackContext,
    pub sounds: &'a mut ProcessSoundRuntime,
    pub rng: &'a mut u32,
}

pub trait FurnitureParticleCallbackHost {
    type Error;
    fn emit(
        &mut self,
        effect: FurnitureParticleEffect,
        inputs: FurnitureParticleCallbackInputs<'_>,
    ) -> Result<(), Self::Error>;
}

pub fn run_furniture_particle_callback<H: FurnitureParticleCallbackHost + ?Sized>(
    selector: u32,
    host: &mut H,
    inputs: FurnitureParticleCallbackInputs<'_>,
) -> Result<bool, H::Error> {
    let effect = match selector {
        0 => FurnitureParticleEffect::Burst(ParticleBurst {
            debris_count: 16,
            position: inputs.context.position_words,
            placement_radius: 20,
            central_type: 1,
            debris_types: [1, 1],
            speed: 8,
            flags: 0,
        }),
        1 => FurnitureParticleEffect::Emitter { selector: 10 },
        2 => FurnitureParticleEffect::Emitter { selector: 13 },
        selector => FurnitureParticleEffect::Central { selector },
    };
    host.emit(effect, inputs)?;
    Ok(true)
}

#[cfg(test)]
mod tests;
