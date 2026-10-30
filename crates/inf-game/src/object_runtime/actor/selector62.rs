//! Selector 62, the Alien Brain's attack: the constructor 00463F60 over the
//! shared 0047A7A0, final table 00526DC8. The table is the shared actor's
//! 00527C00 but for +00 (00463FD0 over the destructor 00463FF0, which
//! deletes the target +15C), +24 (the update 00464050), +28 (the light
//! 004647E0) and +90 (00464930, which runs the update and answers 1). The
//! brain itself, selector 85, builds on this constructor.
//!
//! The update steps +138 through four states on the durations at 00539808
//! (3, 2, 2 and 2 s). Entering 1 it takes the first player's unit within
//! 3000 (004825D0) as its target, or else a fixed point at itself, and
//! raises five ribbons (00486600), which it then swings round the target.
//! Entering 2 converges them; every 500 ms it blasts the target point
//! (resource 0x38, a radius block of 00539820 and a world flash), and
//! leaving 2 blasts once more (resource 0x36 and the blocks left). Entering
//! 3 disposes the ribbons and the target; entering 4 clears flag 8 and sets
//! 0x40. Flag 0x80 holds state 4 (and sends a restart there).

use thiserror::Error;

use super::{
    derived::kinematics::ftol, ActorConstructionError, ActorConstructionRequest, ActorExtension,
};
use crate::object_runtime::{
    mark_flag_change, ActorHandle, ActorServiceAllocationRequest, ActorServiceResourceKind,
    FixedPositionTarget, FollowTargetRequest, NativeHeapRef, NativeTarget, NativeTargetClass,
    ObjectArena, ObjectError, ObjectId,
};

/// 00539808: each state's duration in microseconds, by state.
pub const SELECTOR62_DURATIONS: [u32; 5] = [1, 3_000_000, 2_000_000, 2_000_000, 2_000_000];
/// 00539820: the four 10-byte radius blocks, in order.
pub const SELECTOR62_BLOCKS: u32 = 0x0053_9820;
/// 004640D9 and 004645D6: the blasts' resources.
pub const SELECTOR62_LEAVING_BLAST: u32 = 0x36;
pub const SELECTOR62_BLAST: u32 = 0x38;
/// 0046412B: the flash's radius.
const FLASH_RADIUS: u32 = 0xf00;
/// 004641F9: the squared horizontal reach of the unit it targets.
const REACH_SQUARED: u32 = 9_000_000;
/// 004645B7: a blast every 500 ms in state 2.
const BLAST_PERIOD: u32 = 500_000;

/// BrainRibbonAimApproximation: the brain clears `+160` (`00405FC0`), so its
/// attack has no unit to aim from, and entering state 1 the ribbons' cross
/// aims along `[esp+18]` (`004642B4`), a stack DWORD `00464050` never writes
/// on that path, and the delta's low WORD (EBX). In both runs of the lab's
/// `brain-attack-aim.json` (125 and 16 ms steps; tampered-run: the aliens'
/// node count emptied and the lock's writes made) the DWORD held the world
/// object `[0055F160]`, `0x2801114C`, which the camera lookup `00478E00` had
/// pushed as its ECX earlier in the tick, and the offsets came out (2356,
/// 4000, -3231) and (1066, 4000, 3855). Retail's own heap puts that object
/// anywhere; the port takes the lab's low WORD.
pub const BRAIN_RIBBON_AIM_APPROXIMATION: u16 = 0x114c;

fn log_brain_ribbon_aim_approximation() {
    static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        eprintln!("BrainRibbonAimApproximation: the Alien Brain's attack aims its ribbons along a stack DWORD 00464050 never writes (004642B4), which held the world object's address in retail; the port takes the lab's low WORD 0x114C (ALIEN_BRAIN_RUNTIME.md)");
    }
}

/// 00463F60's fields after 0047A7A0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector62State {
    /// +138, the state: 0 until the first update, then 1..4.
    pub state_138: u32,
    /// +13C, the state's clock, counting down.
    pub clock_13c: u32,
    /// +140..+144, the target point; state 1 writes it first.
    pub point_140: Option<[u16; 3]>,
    /// WORDs +146/+148/+14A, the ribbons' offset; state 1 writes them.
    pub offset_146: Option<[u16; 3]>,
    /// +14C, 4000: the offset's reach (read as a WORD).
    pub reach_14c: u32,
    /// +150, 4000: the swing's lift (read as a WORD).
    pub lift_150: u32,
    /// +154, the next blast's clock in state 2; entering 2 writes it.
    pub blast_154: Option<u32>,
    /// +158, the radius blocks used, 0..4.
    pub blasts_158: u32,
    /// +15C, the target: a follow target (0x20) or a fixed point (0x14).
    pub target_15c: Option<NativeHeapRef>,
    /// BYTE +160, 1: state 1 may target the first player's unit.
    pub may_target_160: u8,
    /// +164..+174, the five ribbons.
    pub ribbons_164: [Option<ObjectId>; 5],
    /// The brain's own fields (selector 85, +178..+19A), which runs this
    /// attack in its state 2.
    pub brain: Option<Box<super::Selector85State>>,
}

/// What the update reaches beyond the arena.
pub trait Selector62Host {
    type Error: std::fmt::Display;
    /// 0054E894, which a fixed target keeps as its handle.
    fn process_null(&self) -> Option<ActorHandle>;
    /// Settings +7C, which the follow target's refresh 00482690 reads.
    fn settings_7c(&self) -> u32;
    /// 00486600 with the actor's handle: a new ribbon, or none.
    fn create_ribbon(
        &mut self,
        arena: &mut ObjectArena,
        source: ActorHandle,
    ) -> Result<Option<ObjectId>, Self::Error>;
    /// 004869B0 on a ribbon with the delta and a point.
    fn extend_ribbon(
        &mut self,
        arena: &mut ObjectArena,
        ribbon: ObjectId,
        delta: u32,
        point: [u16; 3],
    ) -> Result<(), Self::Error>;
    /// 00460310 of `resource` at `position` (EDX 0, placement -1, null
    /// source).
    fn spawn(
        &mut self,
        arena: &mut ObjectArena,
        resource: u32,
        position: [u16; 3],
    ) -> Result<(), Self::Error>;
    /// World 004887D0(block, position, source, 1).
    fn blast(
        &mut self,
        arena: &mut ObjectArena,
        block_address: u32,
        position: [u16; 3],
        source: ActorHandle,
    ) -> Result<(), Self::Error>;
    /// 00487D70(&actor+32, 0xF00, 0), the world flash.
    fn flash(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        radius: u32,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector62Error<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("selector 62 {actor:?} reads its unwritten +{offset:03X} at {address:08X}")]
    Unwritten {
        actor: ObjectId,
        offset: u16,
        address: u32,
    },
    /// 004642B4: without the unit to aim from, a placed attack's swing
    /// offset comes from a stack slot this call never wrote (and the delta's
    /// low WORD); the brain's takes [`BRAIN_RIBBON_AIM_APPROXIMATION`].
    #[error("selector 62 {actor:?} aims its ribbons from an unwritten stack slot at 004642B4")]
    ResidueDirection { actor: ObjectId },
    #[error("selector 62 {actor:?} builds a fixed target without the process NULL 0054E894")]
    MissingProcessNull { actor: ObjectId },
    #[error("selector 62 {actor:?} service at {address:08X}: {error}")]
    Host {
        actor: ObjectId,
        address: u32,
        error: E,
    },
}

type Result62<T, E> = Result<T, Selector62Error<E>>;

impl ObjectArena {
    /// 00463F60: 0047A7A0, then +138 = +13C = 0, +14C = +150 = 4000,
    /// +158 = +15C = 0, +160 = 1 and the ribbons +164..+174 = 0.
    pub fn allocate_selector62(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        self.actor_mut(object)?.extension = ActorExtension::Selector62(Box::new(Selector62State {
            state_138: 0,
            clock_13c: 0,
            point_140: None,
            offset_146: None,
            reach_14c: 4000,
            lift_150: 4000,
            blast_154: None,
            blasts_158: 0,
            target_15c: None,
            may_target_160: 1,
            ribbons_164: [None; 5],
            brain: None,
        }));
        Ok(object)
    }

    pub fn selector62(&self, actor: ObjectId) -> Result<&Selector62State, ObjectError> {
        match &self.actor(actor)?.extension {
            ActorExtension::Selector62(state) => Ok(state),
            _ => Err(ObjectError::NotSelector62(actor)),
        }
    }

    pub(in crate::object_runtime) fn selector62_mut(
        &mut self,
        actor: ObjectId,
    ) -> Result<&mut Selector62State, ObjectError> {
        match &mut self.actor_mut(actor)?.extension {
            ActorExtension::Selector62(state) => Ok(state),
            _ => Err(ObjectError::NotSelector62(actor)),
        }
    }

    fn selector62_position<E>(&self, actor: ObjectId) -> Result62<[u16; 3], E> {
        Ok(self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?)
    }

    fn selector62_point<E>(&self, actor: ObjectId, address: u32) -> Result62<[u16; 3], E> {
        self.selector62(actor)?
            .point_140
            .ok_or(Selector62Error::Unwritten {
                actor,
                offset: 0x140,
                address,
            })
    }

    fn selector62_flags(
        &mut self,
        actor: ObjectId,
        set: u32,
        clear: u32,
    ) -> Result<(), ObjectError> {
        let base = &mut self.owned_mut(actor)?.state.base;
        mark_flag_change(base, (base.flags & !clear) | set);
        Ok(())
    }

    /// The target's point +08..+0D, for either kind.
    fn selector62_target_point<E>(
        &self,
        actor: ObjectId,
        target: NativeHeapRef,
    ) -> Result62<[u16; 3], E> {
        let point = match self.native_target(target)? {
            NativeTarget::ActorFollow(data) => data.focus_08,
            NativeTarget::FixedPosition(data) => data.focus_08,
            _ => None,
        };
        point.ok_or(Selector62Error::Unwritten {
            actor,
            offset: 0x15c,
            address: 0x0046_44fb,
        })
    }

    /// A 0x14 kind-8 target at `point` with the process null (0051F154);
    /// none when the allocation fails.
    fn selector62_fixed_target<H: Selector62Host>(
        &mut self,
        actor: ObjectId,
        point: Option<[u16; 3]>,
        host: &H,
    ) -> Result62<Option<NativeHeapRef>, H::Error> {
        let Some(reference) = self
            .allocate_actor_service(ActorServiceAllocationRequest {
                kind: ActorServiceResourceKind::Target(NativeTargetClass::FixedPosition),
            })
            .map_err(ObjectError::from)?
        else {
            return Ok(None);
        };
        let null = host
            .process_null()
            .ok_or(Selector62Error::MissingProcessNull { actor })?;
        self.construct_native_target(
            reference,
            NativeTarget::FixedPosition(FixedPositionTarget::from_point(null, point)),
        )?;
        Ok(Some(reference))
    }

    /// 00464050, the attack's +24.
    pub fn tick_selector62<H: Selector62Host>(
        &mut self,
        actor: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> Result62<(), H::Error> {
        let clock = self.selector62(actor)?.clock_13c;
        if clock >= delta {
            self.selector62_mut(actor)?.clock_13c = clock - delta;
        } else {
            let next = match self.selector62(actor)?.state_138 {
                1 => 2,
                2 => {
                    self.selector62_leave_attack(actor, host)?;
                    3
                }
                3 => 4,
                _ if self.object(actor)?.base.flags & 0x80 != 0 => 4,
                _ => 1,
            };
            let state = self.selector62_mut(actor)?;
            state.state_138 = next;
            state.clock_13c = SELECTOR62_DURATIONS[next as usize];
            match next {
                1 => self.selector62_enter_aim(actor, delta, host)?,
                2 => {
                    let state = self.selector62_mut(actor)?;
                    state.blasts_158 = 0;
                    let ribbons = state.ribbons_164;
                    for ribbon in ribbons.into_iter().flatten() {
                        self.set_selector67_word_40(ribbon, 0)?;
                    }
                    let state = self.selector62_mut(actor)?;
                    state.blast_154 = Some(state.clock_13c.wrapping_add(1));
                }
                3 => {
                    for index in 0..5 {
                        if let Some(ribbon) = self.selector62(actor)?.ribbons_164[index] {
                            self.dispose(ribbon)?;
                            self.selector62_mut(actor)?.ribbons_164[index] = None;
                        }
                    }
                    if let Some(target) = self.selector62_mut(actor)?.target_15c.take() {
                        self.release_actor_service(target)?;
                    }
                }
                _ => {
                    self.selector62_flags(actor, 0, 8)?;
                    self.selector62_flags(actor, 0x40, 0)?;
                }
            }
        }
        // 00464473: a target that died becomes a fixed point where it was.
        let state = self.selector62(actor)?.state_138;
        if let Some(target) = self.selector62(actor)?.target_15c {
            if matches!(state, 1 | 2) {
                let mut data = self.native_target(target)?;
                let lifecycle = self.refresh_native_target(&mut data, host.settings_7c())?;
                self.write_native_target(target, data)?;
                if (lifecycle as i32) < 3 {
                    let point = self.selector62_target_point::<H::Error>(actor, target)?;
                    if let Some(fixed) = self.selector62_fixed_target(actor, Some(point), host)? {
                        self.release_actor_service(target)?;
                        self.selector62_mut(actor)?.target_15c = Some(fixed);
                    }
                }
                let target = self.selector62(actor)?.target_15c.expect("held above");
                let point = self.selector62_target_point(actor, target)?;
                self.selector62_mut(actor)?.point_140 = Some(point);
            }
        }
        match state {
            1 => self.selector62_swing(actor, delta, host),
            2 => self.selector62_converge(actor, delta, host),
            3 => Ok(()),
            _ => {
                if self.object(actor)?.base.flags & 0x80 != 0 {
                    self.selector62_mut(actor)?.clock_13c = SELECTOR62_DURATIONS
                        .get(state as usize)
                        .copied()
                        .ok_or(Selector62Error::Unwritten {
                            actor,
                            offset: 0x138,
                            address: 0x0046_452d,
                        })?;
                }
                Ok(())
            }
        }
    }

    /// 004640C3: leaving state 2, resource 0x36 at the target point, the
    /// blocks not yet used, then the flash.
    fn selector62_leave_attack<H: Selector62Host>(
        &mut self,
        actor: ObjectId,
        host: &mut H,
    ) -> Result62<(), H::Error> {
        let fail = |address| {
            move |error| Selector62Error::Host {
                actor,
                address,
                error,
            }
        };
        let point = self.selector62_point(actor, 0x0046_40c9)?;
        host.spawn(self, SELECTOR62_LEAVING_BLAST, point)
            .map_err(fail(0x0046_40de))?;
        while self.selector62(actor)?.blasts_158 < 4 {
            let used = self.selector62(actor)?.blasts_158;
            self.selector62_mut(actor)?.blasts_158 = used + 1;
            let source = self.actor(actor)?.handle;
            let point = self.selector62_point(actor, 0x0046_410e)?;
            host.blast(self, SELECTOR62_BLOCKS + used * 10, point, source)
                .map_err(fail(0x0046_4110))?;
        }
        host.flash(self, actor, FLASH_RADIUS)
            .map_err(fail(0x0046_4131))
    }

    /// 00464170: entering state 1.
    fn selector62_enter_aim<H: Selector62Host>(
        &mut self,
        actor: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> Result62<(), H::Error> {
        self.selector62_flags(actor, 8, 0)?;
        self.selector62_flags(actor, 0, 0x40)?;
        let mut aim = None;
        if self.selector62(actor)?.may_target_160 != 0 {
            if let Some(unit) = self.first_player_primary_actor()? {
                if self.actor(unit)?.word_c4 == 3 {
                    let own = self.selector62_position(actor)?;
                    let theirs = self.selector62_position(unit)?;
                    let dx = own[0].wrapping_sub(theirs[0]) as i16;
                    let dz = own[2].wrapping_sub(theirs[2]) as i16;
                    aim = Some((dx, dz));
                    let squared = i32::from(dx) * i32::from(dx) + i32::from(dz) * i32::from(dz);
                    if (squared as u32) < REACH_SQUARED {
                        let handle = self.actor(unit)?.handle;
                        let target = self
                            .follow_target_resource(FollowTargetRequest::resolve_actor(handle))?;
                        let state = self.selector62_mut(actor)?;
                        state.target_15c = target;
                        state.point_140 = Some(theirs);
                    }
                }
            }
        }
        if self.selector62(actor)?.target_15c.is_none() {
            let own = self.selector62_position(actor)?;
            let fixed = self.selector62_fixed_target(actor, Some(own), host)?;
            let state = self.selector62_mut(actor)?;
            state.target_15c = fixed;
            state.point_140 = Some(own);
        }
        // 004642B4: without the unit the offset reads a stack slot this call
        // never wrote, beside the delta's low WORD.
        let (dx, dz) = match aim {
            Some(aim) => aim,
            None if self.selector62(actor)?.brain.is_some() => {
                log_brain_ribbon_aim_approximation();
                (BRAIN_RIBBON_AIM_APPROXIMATION as i16, delta as u16 as i16)
            }
            None => return Err(Selector62Error::ResidueDirection { actor }),
        };
        let reach = self.selector62(actor)?.reach_14c as u16 as i16;
        let offset = if dx == 0 && dz == 0 {
            [reach as u16, 0, 0]
        } else {
            // 004642C5..00464378 under the CRT's 53-bit precision: the
            // length rounds to f32, the X ratio stays in the register and
            // the Z ratio is stored as an f32.
            let (x, z) = (f64::from(dx), f64::from(f32::from(dz)));
            let length = f64::from(((z * z) + (x * x)).sqrt() as f32);
            let unit_x = x / length;
            let unit_z = f64::from((z / length) as f32);
            let reach = f64::from(reach);
            [
                ftol(reach * unit_x) as u16,
                ftol(reach * 0.0) as u16,
                ftol(reach * unit_z) as u16,
            ]
        };
        let lift = self.selector62(actor)?.lift_150 as u16;
        self.selector62_mut(actor)?.offset_146 = Some([offset[0], lift, offset[2]]);
        let source = self.actor(actor)?.handle;
        for index in 0..5 {
            let ribbon =
                host.create_ribbon(self, source)
                    .map_err(|error| Selector62Error::Host {
                        actor,
                        address: 0x0046_43b8,
                        error,
                    })?;
            self.selector62_mut(actor)?.ribbons_164[index] = ribbon;
            if let Some(ribbon) = ribbon {
                self.set_selector67_word_44(ribbon, 1)?;
            }
        }
        Ok(())
    }

    /// 00464631: state 1 swings the ribbons round the target point, rising
    /// with the square root of the clock left.
    fn selector62_swing<H: Selector62Host>(
        &mut self,
        actor: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> Result62<(), H::Error> {
        let state = self.selector62(actor)?;
        let [x1, lift, z1] = state.offset_146.ok_or(Selector62Error::Unwritten {
            actor,
            offset: 0x146,
            address: 0x0046_4644,
        })?;
        let clock = state.clock_13c;
        let rise = (inf_formats::native_basis::isqrt_48efc0(clock as i32) as i32)
            .wrapping_mul(i32::from(lift as i16))
            / (inf_formats::native_basis::isqrt_48efc0(SELECTOR62_DURATIONS[1] as i32) as i32);
        let left = (clock as i32) >> 10;
        let span = (SELECTOR62_DURATIONS[state.state_138 as usize] as i32) >> 10;
        let x = (i32::from(x1 as i16).wrapping_mul(left) / span) as u16;
        let z = (i32::from(z1 as i16).wrapping_mul(left) / span) as u16;
        let y = rise.wrapping_add(0x96) as u16;
        let ribbons = state.ribbons_164;
        let [px, py, pz] = self.selector62_point(actor, 0x0046_46b2)?;
        let height = py.wrapping_add(y);
        let points = [
            [px, height, pz],
            [px.wrapping_add(x), height, pz.wrapping_add(z)],
            [px.wrapping_sub(x), height, pz.wrapping_sub(z)],
            [px.wrapping_sub(z), height, pz.wrapping_add(x)],
            [px.wrapping_add(z), height, pz.wrapping_sub(x)],
        ];
        for (ribbon, point) in ribbons.into_iter().zip(points) {
            if let Some(ribbon) = ribbon {
                host.extend_ribbon(self, ribbon, delta, point)
                    .map_err(|error| Selector62Error::Host {
                        actor,
                        address: 0x0046_46da,
                        error,
                    })?;
            }
        }
        Ok(())
    }

    /// 0046453F: state 2 draws the ribbons to the target point, falling as
    /// the clock runs down, and every 500 ms blasts it.
    fn selector62_converge<H: Selector62Host>(
        &mut self,
        actor: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> Result62<(), H::Error> {
        let fail = |address| {
            move |error| Selector62Error::Host {
                actor,
                address,
                error,
            }
        };
        let [px, py, pz] = self.selector62_point(actor, 0x0046_4552)?;
        let state = self.selector62(actor)?;
        let span = ((SELECTOR62_DURATIONS[2] as i32) >> 10) as u32;
        let fall = ((state.clock_13c >> 10).wrapping_mul(180) / span).wrapping_sub(0x1e);
        let point = [px, py.wrapping_add(fall as u16), pz];
        let ribbons = state.ribbons_164;
        for ribbon in ribbons.into_iter().flatten() {
            host.extend_ribbon(self, ribbon, delta, point)
                .map_err(fail(0x0046_4598))?;
        }
        let state = self.selector62(actor)?;
        let blast = state.blast_154.ok_or(Selector62Error::Unwritten {
            actor,
            offset: 0x154,
            address: 0x0046_45a3,
        })?;
        if (state.clock_13c as i32) >= (blast as i32) {
            return Ok(());
        }
        self.selector62_mut(actor)?.blast_154 = Some(blast.wrapping_sub(BLAST_PERIOD));
        host.spawn(self, SELECTOR62_BLAST, point)
            .map_err(fail(0x0046_45db))?;
        let used = self.selector62(actor)?.blasts_158;
        if used < 4 {
            self.selector62_mut(actor)?.blasts_158 = used + 1;
            let source = self.actor(actor)?.handle;
            let target = self.selector62_point(actor, 0x0046_460f)?;
            host.blast(self, SELECTOR62_BLOCKS + used * 10, target, source)
                .map_err(fail(0x0046_4611))?;
            host.flash(self, actor, FLASH_RADIUS)
                .map_err(fail(0x0046_4627))?;
        }
        Ok(())
    }

    /// 004647E0, the attack's +28: one ground light at the target point,
    /// fading with the state's clock. Returns (radius, rgb).
    pub fn selector62_light(
        &self,
        actor: ObjectId,
    ) -> Result<Option<crate::world_lighting::TerrainLightRequest>, ObjectError> {
        let state = self.selector62(actor)?;
        let clamp = |value: u32| if value < 0x7f { value as u8 } else { 0x7f };
        let clock = state.clock_13c;
        let (radius, rgb) = match state.state_138 {
            1 => {
                let duration = SELECTOR62_DURATIONS[1];
                let radius = 0x800_u32.wrapping_sub(clock / (((duration as i32) >> 11) as u32));
                let level = 0x20_u32.wrapping_sub(clock / (((duration as i32) >> 5) as u32));
                (radius as i32, [0, clamp(level), clamp(level)])
            }
            2 => {
                let duration = SELECTOR62_DURATIONS[2];
                let red = 0x40_u32.wrapping_sub(clock / (((duration as i32) >> 6) as u32));
                let green = red.wrapping_add(0x40);
                let blue = green.wrapping_add(0x20);
                (0x800, [clamp(red), clamp(green), clamp(blue)])
            }
            3 => {
                let duration = SELECTOR62_DURATIONS[3];
                let level = clock.wrapping_sub(1) / (((duration as i32) >> 6) as u32);
                let radius = clock / (((duration as i32) >> 11) as u32);
                (radius as i32, [level as u8; 3])
            }
            _ => return Ok(None),
        };
        let [x, _, z] = state
            .point_140
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        Ok(Some(crate::world_lighting::TerrainLightRequest {
            world_xz: [i32::from(x as i16), i32::from(z as i16)],
            radius,
            rgb: rgb.map(|byte| byte as i8),
        }))
    }
}

#[cfg(test)]
#[path = "selector62_tests.rs"]
mod tests;
