//! 004791F0, run by 00478F20 after its update-list swap. Each G+48 actor
//! without flag 8 may take terrain contact (flags & 04000100 == 100), then
//! furniture contact (flags & 0400000C == 4). Both gates, and the flags that
//! 0047EBB0 merges back, come from the single read before either phase.

use inf_formats::native_basis::isqrt_48efc0;
use thiserror::Error;

mod furniture;
mod pair;
pub use furniture::{FurnitureContactResponse, FurnitureResponseError, FurnitureResponseHost};
pub use pair::{ActorPairError, ActorPairHost, WorldPairHost};

use super::{
    classify_actor_contact, mark_flag_change, native_trunc_low32 as ftol, ActorDamagePacket,
    ActorDamageRequest, ActorHandle, ActorStateChangeRequest, ActorTerrainError,
    ActorTerrainInputs, BoulderParticle, CollisionContact, FamilyUpdateClass, ObjectArena,
    ObjectError, ObjectId, Selector59ContactError, Selector59ContactHost,
};
use crate::process_particles::births::ParticleBurst;

/// Reached services of one actor's pass. Each keeps its own reached writes.
pub trait WorldContactHost {
    type Error;
    /// 0044EC50 over the actor's H8 word +42, virtual +48 basis and position
    /// WORDs. `None` is a zero contact flag.
    fn terrain_contact(
        &mut self,
        arena: &ObjectArena,
        actor: ObjectId,
    ) -> Result<Option<CollisionContact>, Self::Error>;
    /// Current H12 plane, 005483BC, 005471C0, H14/0 +4 and the sine table.
    fn water_inputs(&self) -> ActorTerrainInputs<'_>;
    /// 0047D3EB..FE, read only for a closing speed above 0x96.
    fn network_impact_gate(&self) -> NetworkImpactGate;
    /// 0047C6E0 with the self-sourced packet 0047D2F0 builds at 0047D408.
    fn impact_damage(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: ActorDamageRequest,
    ) -> Result<(), Self::Error>;
    /// 0047C1D0, reached when (flags & 0400000C) == 4.
    fn furniture_contact(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<(), Self::Error>;
    /// Process 0054E894, read by the crash arms at 00434F75 and 0043F470.
    fn process_null_handle(&self) -> Option<ActorHandle>;
    /// One service of a Selector34/35 +94 crash arm, in native order.
    fn destruction_effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: TerrainDestructionEffect,
    ) -> Result<(), Self::Error>;
    /// One 0048F060 draw, for the Boulders' dust (00405A72, 00405C82..B4).
    fn random(&mut self) -> Result<u32, Self::Error>;
    /// The Boulders' splash (00489600) or dust (004894A0, no shadow), type
    /// 0 and flags 0 (00405C47..00405CF8).
    fn boulder_particle(
        &mut self,
        kind: BoulderParticle,
        position: [u16; 3],
        velocity: [u16; 3],
    ) -> Result<(), Self::Error>;
}

/// The pass's services as the Boulders' contact reaches them.
struct BoulderContact<'h, H> {
    host: &'h mut H,
}

impl<H: WorldContactHost> Selector59ContactHost for BoulderContact<'_, H> {
    type Error = H::Error;
    #[track_caller]
    fn random(&mut self) -> Result<u32, H::Error> {
        self.host.random()
    }
    fn surface(&self) -> Option<(&crate::terrain_runtime::TerrainRuntime, u32, &[u16; 4096])> {
        let inputs = self.host.water_inputs();
        inputs
            .terrain
            .map(|surface| (surface.terrain, inputs.clock, inputs.sine_quarter))
    }
    fn particle(
        &mut self,
        kind: BoulderParticle,
        position: [u16; 3],
        velocity: [u16; 3],
    ) -> Result<(), H::Error> {
        self.host.boulder_particle(kind, position, velocity)
    }
}

/// Services the Selector34 (00434ED0, +C4 not 3) and Selector35 (0043F3E0,
/// +C4 1) crash arms reach before 0047D2F0. The radius effect and the pickup
/// read the actor's live +32 position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainDestructionEffect {
    /// 0048A8B0 over the stack block built at 00434EE3..F34.
    Burst(ParticleBurst),
    /// World 004887D0(block, &+32, this handle, 1), unless flags bit 31.
    Radius {
        block_address: u32,
        source: ActorHandle,
    },
    /// 0046D870(&+32, kind, value).
    Pickup { kind: u32, value: u16 },
    /// 0047D670(0, process NULL, 0).
    ChangeState(ActorStateChangeRequest),
    /// 0047CE90: states 2 and 3 go to state 1 with the process NULL.
    Retire,
}

/// 0055F448 and, when it is set, DWORD +34 of 00463AB0's static session
/// 0054E8A8. A zero session DWORD skips the impact damage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkImpactGate {
    Offline,
    Session(u32),
    /// An active session whose +34 has no port owner.
    Unowned,
}

/// The burst block both crash arms build: sixteen type-1 debris around a
/// type-2 centre within 20 units, speed 8, no flags.
fn crash_burst(position: [u16; 3]) -> TerrainDestructionEffect {
    TerrainDestructionEffect::Burst(ParticleBurst {
        debris_count: 16,
        position,
        placement_radius: 20,
        central_type: 2,
        debris_types: [1, 1],
        speed: 8,
        flags: 0,
    })
}

/// Phases the caller still admits for one actor. A stopped phase is skipped;
/// the other one keeps running in its native order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldContactPhases {
    pub terrain: bool,
    pub furniture: bool,
}

#[derive(Debug, Error)]
pub enum WorldContactError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("terrain contact service failed: {0}")]
    TerrainHost(E),
    #[error("actor {actor:?} has no owned virtual +94 terrain response")]
    ResponseClass { actor: ObjectId },
    #[error("actor {actor:?} +94 destruction requires process NULL 0054E894")]
    MissingProcessNull { actor: ObjectId },
    #[error("actor {actor:?} +94 destruction service at {address:08X} failed: {error}")]
    Destruction {
        actor: ObjectId,
        address: u32,
        error: E,
    },
    #[error("terrain contact for {actor:?} left DWORD +{offset:02X} unwritten at 0047D2F0")]
    RequiredContact { actor: ObjectId, offset: u8 },
    #[error("0047D2F0 network gate 00463AB0 for {actor:?} is not owned")]
    RequiredNetworkGate { actor: ObjectId },
    #[error("0047D2F0 impact damage for {actor:?} failed: {error}")]
    ImpactDamage { actor: ObjectId, error: E },
    #[error("the Boulders' +94 00405A00: {0}")]
    Boulder(Selector59ContactError<E>),
    #[error(transparent)]
    Water(#[from] ActorTerrainError),
    #[error("furniture contact service failed: {0}")]
    FurnitureHost(E),
}

impl<E> WorldContactError<E> {
    /// A furniture stop leaves the terrain phase admitted on later frames.
    pub fn is_furniture(&self) -> bool {
        matches!(self, Self::FurnitureHost(_))
    }
}

/// `004690B8`: the velocity change past which the vehicle kills a mutant
/// scientist it hits.
const SCIENTIST_RAM_SPEED: u32 = 500;

impl ObjectArena {
    /// One actor of 004791F0, visited in G+48 order by the caller.
    pub fn run_world_contact<H: WorldContactHost>(
        &mut self,
        actor: ObjectId,
        phases: WorldContactPhases,
        host: &mut H,
    ) -> Result<(), WorldContactError<H::Error>> {
        let original = self.object(actor)?.base.flags;
        if original & 8 != 0 {
            return Ok(());
        }
        if phases.terrain && original & 0x0400_0100 == 0x100 {
            self.run_terrain_contact(actor, original, host)?;
        }
        if phases.furniture && original & 0x0400_000c == 4 {
            host.furniture_contact(self, actor)
                .map_err(WorldContactError::FurnitureHost)?;
        }
        Ok(())
    }

    fn run_terrain_contact<H: WorldContactHost>(
        &mut self,
        actor: ObjectId,
        original: u32,
        host: &mut H,
    ) -> Result<(), WorldContactError<H::Error>> {
        match host
            .terrain_contact(self, actor)
            .map_err(WorldContactError::TerrainHost)?
        {
            None => {
                let base = &mut self.owned_mut(actor)?.state.base;
                mark_flag_change(base, base.flags & !0x0008_0000);
            }
            Some(contact) => {
                self.respond_world_terrain(actor, contact, host)?;
                let base = &mut self.owned_mut(actor)?.state.base;
                mark_flag_change(base, base.flags | 0x0008_0000);
                // 00479346..0047936D then refresh only the dead request copy.
            }
        }
        let position = self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        let radius = self.actor(actor)?.rotating.visual.radius_word;
        let classification = classify_actor_contact(position, radius, host.water_inputs())?;
        let base = &mut self.owned_mut(actor)?.state.base;
        mark_flag_change(base, base.flags & 0x9fff_ffff);
        // 0047EBB0 ORs the pre-pass flags back in, so a cleared 0x80000, or a
        // bit dropped by the response, returns here.
        mark_flag_change(
            base,
            base.flags | (original & 0x9fff_ffff) | classification.flags_to_or(),
        );
        Ok(())
    }

    /// Actor virtual +94. Final 10/12/36 tables, the vehicle's 00520860, the
    /// Powerball's 00526464, the ECM Unit's 005262CC and the Remote Beamer's
    /// and its receiver's 005274C4/00526530 hold 0047D2F0 itself; Selector35 0043F3E0
    /// and Selector34 00434ED0 reach it after the crash arm their +C4
    /// selects, and the Boulders' 00405A00 throws up its dust after it. The
    /// rock's 00458C30 retires it (0047CE90) without the shared response.
    fn respond_world_terrain<H: WorldContactHost>(
        &mut self,
        actor: ObjectId,
        contact: CollisionContact,
        host: &mut H,
    ) -> Result<(), WorldContactError<H::Error>> {
        let state = self.actor(actor)?.word_c4;
        match self.family_update_class(actor) {
            // 0043F3EB: only +C4 == 1 crashes before the shared response.
            Ok(FamilyUpdateClass::Selector35) if state == 1 => {
                self.crash_selector35(actor, host)?;
            }
            // 00434ED6: every +C4 but 3 crashes before the shared response.
            Ok(FamilyUpdateClass::Selector34) if state != 3 => {
                self.crash_selector34(actor, host)?;
            }
            Ok(_) => {}
            Err(_)
                if self.selector2_actor(actor).is_ok()
                    || self.selector57(actor).is_ok()
                    || self.selector73(actor).is_ok()
                    || self.selector55(actor).is_ok()
                    || self.is_selector56(actor)
                    || self.selector17(actor).is_ok() => {}
            Err(_) if self.selector44(actor).is_ok() => {
                return host
                    .destruction_effect(self, actor, TerrainDestructionEffect::Retire)
                    .map_err(|error| WorldContactError::Destruction {
                        actor,
                        address: 0x0045_8c30,
                        error,
                    });
            }
            Err(_) if self.selector59(actor).is_ok() => {
                self.respond_terrain_push(actor, contact, host)?;
                return self
                    .selector59_contact(actor, contact, &mut BoulderContact { host })
                    .map_err(WorldContactError::Boulder);
            }
            Err(_) => return Err(WorldContactError::ResponseClass { actor }),
        }
        self.respond_terrain_push(actor, contact, host)
    }

    /// 00434EE3..F81: burst, radius effect, pickup and state 0, each a host
    /// service that sees the preceding ones' writes.
    fn crash_selector34<H: WorldContactHost>(
        &mut self,
        actor: ObjectId,
        host: &mut H,
    ) -> Result<(), WorldContactError<H::Error>> {
        let burst = crash_burst(self.crash_position(actor)?);
        self.crash_effect(actor, 0x0043_4f3c, burst, host)?;
        self.crash_radius(actor, 0x0043_4f5f, host)?;
        let pickup = TerrainDestructionEffect::Pickup {
            kind: 0x83,
            value: 500,
        };
        self.crash_effect(actor, 0x0043_4f70, pickup, host)?;
        self.crash_state_zero(actor, 0x0043_4f81, host)
    }

    /// 0043F3F7..47D: radius effect, burst and state 0. The burst block copies
    /// +32 before the radius effect runs.
    fn crash_selector35<H: WorldContactHost>(
        &mut self,
        actor: ObjectId,
        host: &mut H,
    ) -> Result<(), WorldContactError<H::Error>> {
        let burst = crash_burst(self.crash_position(actor)?);
        self.crash_radius(actor, 0x0043_f462, host)?;
        self.crash_effect(actor, 0x0043_f46b, burst, host)?;
        self.crash_state_zero(actor, 0x0043_f47d, host)
    }

    fn crash_position<E>(&self, actor: ObjectId) -> Result<[u16; 3], WorldContactError<E>> {
        Ok(self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?)
    }

    fn crash_effect<H: WorldContactHost>(
        &mut self,
        actor: ObjectId,
        address: u32,
        effect: TerrainDestructionEffect,
        host: &mut H,
    ) -> Result<(), WorldContactError<H::Error>> {
        host.destruction_effect(self, actor, effect)
            .map_err(|error| WorldContactError::Destruction {
                actor,
                address,
                error,
            })
    }

    /// World 004887D0 with EXE block 00531250 and this handle, skipped by
    /// flag bit 31 (read at the call).
    fn crash_radius<H: WorldContactHost>(
        &mut self,
        actor: ObjectId,
        address: u32,
        host: &mut H,
    ) -> Result<(), WorldContactError<H::Error>> {
        if self.object(actor)?.base.flags & 0x8000_0000 != 0 {
            return Ok(());
        }
        let source = self.actor(actor)?.handle;
        let radius = TerrainDestructionEffect::Radius {
            block_address: 0x0053_1250,
            source,
        };
        self.crash_effect(actor, address, radius, host)
    }

    /// 0047D670(0, process NULL, 0), the NULL read just before the call.
    fn crash_state_zero<H: WorldContactHost>(
        &mut self,
        actor: ObjectId,
        address: u32,
        host: &mut H,
    ) -> Result<(), WorldContactError<H::Error>> {
        let source = host
            .process_null_handle()
            .ok_or(WorldContactError::MissingProcessNull { actor })?;
        let change = TerrainDestructionEffect::ChangeState(ActorStateChangeRequest {
            target_state: 0,
            argument: 0,
            source,
        });
        self.crash_effect(actor, address, change, host)
    }

    /// 0047D2F0: push the position out along the contact normal, then add
    /// 3/2 of the closing speed back along it. Position WORDs change without
    /// dirty bits; 0047B510 owns the velocity add.
    fn respond_terrain_push<H: WorldContactHost>(
        &mut self,
        actor: ObjectId,
        contact: CollisionContact,
        host: &mut H,
    ) -> Result<(), WorldContactError<H::Error>> {
        let missing = |offset| WorldContactError::RequiredContact { actor, offset };
        let displacement = contact.displacement().ok_or(missing(0x10))?;
        let mut normal = [0.0; 3];
        for (component, bits) in contact.normal_bits().into_iter().enumerate() {
            let bits = bits.ok_or(missing(4 + 4 * component as u8))?;
            normal[component] = f64::from(f32::from_bits(bits));
        }
        // 0047D2FD: the signed low WORD of +10 scales each component.
        let push = f64::from(displacement as i16);
        let offset = normal.map(|n| ftol(push * n) as u16);
        let base = &mut self.owned_mut(actor)?.state.base;
        let mut position = base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        for (word, offset) in position.iter_mut().zip(offset) {
            *word = word.wrapping_add(offset);
        }
        base.position_words = Some(position);
        let velocity = self
            .actor(actor)?
            .words_8e_92
            .map(|word| f64::from(word as i16));
        let along =
            ftol((velocity[2] * normal[2] + velocity[1] * normal[1]) + velocity[0] * normal[0]);
        let closing = along.wrapping_neg();
        if closing <= 0 {
            return Ok(());
        }
        // 0047D389..A0: signed (3 * closing) / 2, then its low WORD.
        let rebound = (closing.wrapping_mul(3) / 2) as i16;
        let saved = self.actor(actor)?.words_8e_92;
        let impulse = normal.map(|n| ftol(f64::from(rebound) * n) as u16);
        self.add_actor_linear_velocity(actor, impulse)?;
        if self.object(actor)?.base.flags & 0x8000_0000 != 0 || closing <= 0x96 {
            return Ok(());
        }
        // Native reads the gate before the 0x96 test; both reads are pure.
        match host.network_impact_gate() {
            NetworkImpactGate::Offline => {}
            NetworkImpactGate::Session(0) => return Ok(()),
            NetworkImpactGate::Session(_) => {}
            NetworkImpactGate::Unowned => {
                return Err(WorldContactError::RequiredNetworkGate { actor })
            }
        }
        // 0047D40F..19: strength WORD from 0047B590, channel byte 1, and the
        // sign byte left in the reused argument DWORD by the rebound WORD.
        let packet = ActorDamagePacket {
            strength: self.velocity_change_strength(actor, saved)?,
            channel: 1,
            byte_03: Some(if rebound < 0 { 0xff } else { 0 }),
        };
        let source = self.actor(actor)?.handle;
        host.impact_damage(self, actor, ActorDamageRequest { packet, source })
            .map_err(|error| WorldContactError::ImpactDamage { actor, error })
    }

    /// 0047B590: the WORD-wrapped change from `saved` to the current
    /// velocity, its 0048EFC0 length times mass WORD +8C, divided by 192.
    fn velocity_change_strength(
        &self,
        actor: ObjectId,
        saved: [u16; 3],
    ) -> Result<i16, ObjectError> {
        let length = self.velocity_change_length(actor, saved)?;
        let scaled = i32::from(self.actor(actor)?.word_8c).wrapping_mul(length as i32);
        // 0047B5F9..60A: signed division by 192 through 2AAAAAAB and SAR 5.
        let high = ((i64::from(scaled) * 0x2aaa_aaab) >> 32) as i32 >> 5;
        Ok(high.wrapping_add((high as u32 >> 31) as i32) as i16)
    }

    /// `00469030..004690BD`: whether the knock kills a scientist. The
    /// partner's `+64` must answer (only the vehicle's does), the scientist
    /// must be a mutant (team 0x12) alive in state 3, and the contact must
    /// have changed its velocity by more than 500.
    pub fn knock_kills_scientist(
        &self,
        scientist: ObjectId,
        partner_answers_64: bool,
        saved: [u16; 3],
    ) -> Result<bool, ObjectError> {
        let state = self.actor(scientist)?;
        Ok(partner_answers_64
            && state.word_b4 == 0x12
            && state.word_c4 == 3
            && self.velocity_change_length(scientist, saved)? > SCIENTIST_RAM_SPEED)
    }

    /// How far a contact changed the actor's velocity +8E..+92 from
    /// `saved` (`0047B5D2`, `00469063..004690B8`): signed WORD deltas'
    /// wrapping squares, and a sign-set sum takes the root of a quarter,
    /// doubled.
    pub fn velocity_change_length(
        &self,
        actor: ObjectId,
        saved: [u16; 3],
    ) -> Result<u32, ObjectError> {
        let state = self.actor(actor)?;
        let delta: [i32; 3] =
            std::array::from_fn(|i| i32::from(saved[i].wrapping_sub(state.words_8e_92[i]) as i16));
        let square = delta
            .iter()
            .fold(0i32, |sum, &d| sum.wrapping_add(d.wrapping_mul(d)));
        Ok(if square < 0 {
            isqrt_48efc0(((square as u32) >> 2) as i32) << 1
        } else {
            isqrt_48efc0(square)
        })
    }
}

#[cfg(test)]
mod tests;
