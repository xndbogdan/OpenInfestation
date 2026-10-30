//! Reached captured-object virtual+10, family2/10, exact class30/36/60 captures,
//! the class60 exact7 trail, selector10's legs, the vehicle's exact3 wheels,
//! selector77 LOWBYTE2/10 and the shared actor table's 0047A240.
//!
//! Class policy comes from the final native table, never a model, placement,
//! level, or actor+B4 value. Other source classes/selector arms stay explicit.
//! Actor/world/relative writes use the real retained arena. Exact incomplete
//! prefixes survive a portable required-backing error.

use thiserror::Error;

use super::{
    selector10_custom_425870, ActorExtension, DerivedActorClass, DerivedActorError,
    Family4236d0Class, Family428650Class, Family429e20Class, ObjectArena, ObjectClass, ObjectError,
    ObjectId, Selector10CustomInputs, Selector10CustomServices, Selector10LegError,
};
use crate::model_vertices::{
    FurnitureCustomTrail, ModelCustomVertexRequest, ModelVertexFault, ModelVertices,
};

mod selector2;
mod selector77;

/// Source+10 phase, before 00478950's enclosing caller-origin rebasing.
pub struct ObjectCustomVertexRequest<'a> {
    pub source: ObjectId,
    pub compute: ModelCustomVertexRequest<'a>,
    pub vertices: &'a ModelVertices,
    pub output: &'a mut Option<[i32; 3]>,
    /// Process services for arms that birth particles; None fails at reach.
    pub effects: Option<&'a mut dyn ObjectCustomVertexEffects>,
}

/// Process services reached by a source+10 arm that births a trail.
pub trait ObjectCustomVertexEffects {
    /// Raw signed DWORD 0055F154.
    fn load_factor(&mut self) -> Result<i32, String>;
    /// One 0048F060 draw from the shared process RNG.
    fn random(&mut self) -> Result<u32, String>;
    /// 00489D90 with a valid type; its NULL return is not an error.
    fn birth_trail(&mut self, trail: FurnitureCustomTrail) -> Result<(), String>;
    /// Selector10's leg services, with the executable's descriptor tables
    /// and the 0053DF00 sine table.
    fn legs(&mut self) -> Result<ObjectLegServices<'_>, String>;
    /// 004894A0 with type 0, flags 0 and no shadow: a wheel's dust.
    fn birth_material_sprite(
        &mut self,
        position: [u16; 3],
        velocity: [u16; 3],
    ) -> Result<(), String>;
    /// 00489600 with type 0, a zero velocity and flags 0: a wheel's splash.
    fn birth_impact(&mut self, position: [u16; 3]) -> Result<(), String>;
}

/// The ground, clock, flags, byte 0053B890, RNG and trails 00425870 reaches.
pub struct ObjectLegServices<'a> {
    pub services: &'a mut dyn Selector10CustomServices,
    pub executable: inf_formats::FrontendExecutable<'a>,
    pub sine: std::sync::Arc<[u16; 4096]>,
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum ObjectCustomVertexError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Vertex(#[from] ModelVertexFault),
    #[error("object {object:?} custom selector {selector} has an unowned source+10 arm")]
    Unowned { object: ObjectId, selector: i32 },
    #[error("object {object:?} custom selector {selector} requires extra WORD {index}")]
    RequiredExtra {
        object: ObjectId,
        selector: i32,
        index: usize,
    },
    #[error("object {object:?} custom callback requires current field +{offset:X}")]
    RequiredField { object: ObjectId, offset: u32 },
    #[error("object {object:?} behavior group {index} lacks backing in {groups} groups")]
    BehaviorGroup {
        object: ObjectId,
        index: u16,
        groups: usize,
    },
    #[error("object {object:?} custom selector {selector} needs particle and RNG services")]
    RequiredEffects { object: ObjectId, selector: i32 },
    #[error("object {object:?} custom trail service failed: {reason}")]
    Effects { object: ObjectId, reason: String },
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error("object {object:?} selector10 legs: {error}")]
    Legs {
        object: ObjectId,
        error: Selector10LegError,
    },
}

impl ObjectArena {
    /// Actual table policy selects familyLOWBYTE2/10, selector10 legs
    /// LOWBYTE0/1/5, selector30 full8, selector36 full11, selector60 full7
    /// trail or cached9, selector77LOWBYTE2/10, or the shared actor's full10
    /// capture and ground.
    /// Unsigned439E10 uses current caller slot bytes for each reached capture.
    /// Earlier flags and successful captures stay in the real source state.
    pub fn compute_model_custom_vertex(
        &mut self,
        request: ObjectCustomVertexRequest<'_>,
    ) -> Result<(), ObjectCustomVertexError> {
        let source = request.source;
        let ObjectClass::Actor(actor) = &self.object(source)?.class else {
            return Err(ObjectCustomVertexError::Unowned {
                object: source,
                selector: request.compute.selector,
            });
        };
        // 0047FA80, the Siren's +10: its position, whatever the selector.
        if matches!(actor.extension, ActorExtension::Selector47(_)) {
            let position = self.object(source)?.base.position_words.ok_or(
                ObjectCustomVertexError::RequiredField {
                    object: source,
                    offset: 0x32,
                },
            )?;
            *request.output = Some(position.map(|axis| i32::from(axis as i16)));
            return Ok(());
        }
        // 00413060, the Pursuit Ship's +10: low byte 2 publishes the row
        // extra 0, which 0048CF70 first captures into behavior group extra 1
        // while +138 is held; every other selector, the ground under it.
        if matches!(actor.extension, ActorExtension::Selector61(_)) {
            if request.compute.selector & 0xff != 2 {
                return self.ground_model_vertex(request);
            }
            let extra = |index| {
                request.compute.extras.get(index).copied().ok_or(
                    ObjectCustomVertexError::RequiredExtra {
                        object: source,
                        selector: request.compute.selector,
                        index,
                    },
                )
            };
            let world = request.vertices.row_words_439e10(extra(0)?)?;
            if self.selector61(source)?.behavior_138.is_some() {
                self.capture_model_behavior_group(source, extra(1)?, world)?;
            }
            *request.output = Some(world.map(|axis| i32::from(axis as i16)));
            return Ok(());
        }
        let origin = match &actor.extension {
            ActorExtension::Selector30(_) => {
                Some(ObjectCustomCallbackOrigin::Selector30Capture48b290)
            }
            ActorExtension::Selector15(_) => {
                Some(ObjectCustomCallbackOrigin::Selector15Factory42e3d0)
            }
            ActorExtension::Derived(derived) => object_custom_callback_origin(&derived.class),
            // The shared table and every other actor class keep its +10.
            _ => Some(ObjectCustomCallbackOrigin::Shared47a240),
        };
        match origin {
            //48B295 compares the full DWORD; only exact8 captures16C.
            Some(ObjectCustomCallbackOrigin::Selector30Capture48b290)
                if request.compute.selector == 8 =>
            {
                return self.capture_selector30_model_vertex(request);
            }
            //48B29D..2E7: every other selector, the ground under the actor.
            Some(ObjectCustomCallbackOrigin::Selector30Capture48b290) => {
                return self.ground_model_vertex(request);
            }
            Some(ObjectCustomCallbackOrigin::Selector77Capture4074e0) => {
                return self.capture_selector77_model_vertex(request);
            }
            Some(ObjectCustomCallbackOrigin::Selector15Factory42e3d0) => {
                return self.factory_model_vertex(request);
            }
            Some(ObjectCustomCallbackOrigin::Direct4260c0) => {}
            //0047A247 compares the full DWORD: exact 10 is 004260C0's height
            //capture, every other selector the ground under the actor.
            Some(ObjectCustomCallbackOrigin::Shared47a240) if request.compute.selector == 10 => {}
            Some(ObjectCustomCallbackOrigin::Shared47a240) => {
                return self.ground_model_vertex(request);
            }
            //4823F5 compares the full DWORD before forwarding unchanged this,
            //context and extras at482453. Only exact11 captures actor278.
            Some(ObjectCustomCallbackOrigin::Selector36Wrapper4823f0) => {
                if request.compute.selector == 11 {
                    return self.capture_selector36_model_vertex(request);
                }
            }
            //00425870 owns low bytes 0, 1 and 5 and forwards every other
            //low byte, unchanged, to 004260C0 at00425A20.
            Some(ObjectCustomCallbackOrigin::Selector10Legs425870)
                if matches!(request.compute.selector & 0xff, 0 | 1 | 5) =>
            {
                return self.selector10_model_vertex(request);
            }
            Some(ObjectCustomCallbackOrigin::Selector10Legs425870) => {}
            //00415D37 compares the full DWORD; every other selector jumps
            //to 004260C0 with this, context and extras unchanged.
            Some(ObjectCustomCallbackOrigin::Selector2Wheels415d30)
                if request.compute.selector == 3 =>
            {
                return self.selector2_wheel_vertex(request);
            }
            Some(ObjectCustomCallbackOrigin::Selector2Wheels415d30) => {}
            //480CFF takes exact fullDWORD7 before the cached9 test at480D08.
            Some(ObjectCustomCallbackOrigin::Selector60Cached480cf0)
                if request.compute.selector == 7 =>
            {
                return selector60_trail_model_vertex(request);
            }
            Some(ObjectCustomCallbackOrigin::Selector60Cached480cf0)
                if request.compute.selector == 9 =>
            {
                return self.cached_selector60_model_vertex(request);
            }
            _ => {
                return Err(ObjectCustomVertexError::Unowned {
                    object: source,
                    selector: request.compute.selector,
                });
            }
        }
        let selector = request.compute.selector & 0xff;
        if !matches!(selector, 2 | 10) {
            return Err(ObjectCustomVertexError::Unowned {
                object: source,
                selector: request.compute.selector,
            });
        }
        let extra = |index| {
            request.compute.extras.get(index).copied().ok_or(
                ObjectCustomVertexError::RequiredExtra {
                    object: source,
                    selector: request.compute.selector,
                    index,
                },
            )
        };
        let world = request.vertices.row_words_439e10(extra(0)?)?;
        match selector {
            2 => {
                if self.derived_actor(source)?.behavior_140.is_some() {
                    self.capture_model_behavior_group(source, extra(1)?, world)?;
                }
            }
            10 => {
                //426159 reads actorY only after439E10;426160 stores WORD74
                //before publishing outputXYZ. This is not a dirty setter.
                let actor_y = self.object(source)?.base.position_words.ok_or(
                    ObjectCustomVertexError::RequiredField {
                        object: source,
                        offset: 0x34,
                    },
                )?[1];
                self.actor_mut(source)?.word_74 = world[1].wrapping_sub(actor_y);
            }
            _ => unreachable!("admitted family custom selector"),
        }
        *request.output = Some(world.map(|axis| i32::from(axis as i16)));
        Ok(())
    }

    /// `0042E3D0`, the factory's source+10. Exact selector 6 captures the
    /// dock row (extra WORD 0) at +190..+194; exact 7 births the chimney
    /// trail while the family's free capacity (virtual +CC(0) = `0040E090`,
    /// +13C - +140) is zero. Every other selector, and 7 with capacity left,
    /// returns the factory's X and Z at the ground height `00450A30(x, z, 0)`.
    fn factory_model_vertex(
        &mut self,
        request: ObjectCustomVertexRequest<'_>,
    ) -> Result<(), ObjectCustomVertexError> {
        let source = request.source;
        let selector = request.compute.selector;
        let extra = |index: usize| {
            request.compute.extras.get(index).copied().ok_or(
                ObjectCustomVertexError::RequiredExtra {
                    object: source,
                    selector,
                    index,
                },
            )
        };
        if selector == 6 {
            let row = request.vertices.row_words_439e10(extra(0)?)?;
            self.selector15_mut(source)?.dock_190 = Some(row);
            *request.output = Some(row.map(|axis| i32::from(axis as i16)));
            return Ok(());
        }
        let family = &self.selector15(source)?.intermediate;
        let capacity = family.word_13c.wrapping_sub(family.word_140);
        if selector == 7 && capacity == 0 {
            return factory_trail_model_vertex(request);
        }
        self.ground_model_vertex(request) //0042E3F7
    }

    /// `0047A1E0`'s body, which `0047A240` and `0042E3D0` repeat for their
    /// other selectors: X first, then the ground `00450A30(x, z, 0)` at the
    /// object's X/Z, then Z.
    fn ground_model_vertex(
        &self,
        request: ObjectCustomVertexRequest<'_>,
    ) -> Result<(), ObjectCustomVertexError> {
        let source = request.source;
        let [x, _, z] = self.object(source)?.base.position_words.ok_or(
            ObjectCustomVertexError::RequiredField {
                object: source,
                offset: 0x32,
            },
        )?;
        let effects = request
            .effects
            .ok_or(ObjectCustomVertexError::RequiredEffects {
                object: source,
                selector: request.compute.selector,
            })?;
        let legs = effects
            .legs()
            .map_err(|reason| ObjectCustomVertexError::Effects {
                object: source,
                reason,
            })?;
        let height = legs.services.height(x, z, 0);
        *request.output = Some([i32::from(x as i16), i32::from(height), i32::from(z as i16)]);
        Ok(())
    }

    /// Exact fullDWORD8 of 0048B290. Every reach captures unsigned extra0
    /// through439E10 into the actual16C/16E/170 row, then publishes signed XYZ.
    fn capture_selector30_model_vertex(
        &mut self,
        request: ObjectCustomVertexRequest<'_>,
    ) -> Result<(), ObjectCustomVertexError> {
        let source = request.source;
        let index = request.compute.extras.first().copied().ok_or(
            ObjectCustomVertexError::RequiredExtra {
                object: source,
                selector: request.compute.selector,
                index: 0,
            },
        )?;
        let world = request.vertices.row_words_439e10(index)?;
        let ActorExtension::Selector30(state) = &mut self.actor_mut(source)?.extension else {
            unreachable!("actual source callback checked");
        };
        //48B2F0/300 pass the retained row to439E10;48B305..320 read its
        //signed WORDs. This loan admits complete known current XYZ rows and
        //does not represent native invalid pointers or per-axis partial faults.
        state.position_16c = Some(world);
        *request.output = Some(world.map(|axis| i32::from(axis as i16)));
        Ok(())
    }

    /// Exact full-DWORD11 of 004823F0. The actual BYTE265 flag precedes
    /// extra0 and439E10. Captured278/27A/27C survive later caller failures.
    fn capture_selector36_model_vertex(
        &mut self,
        request: ObjectCustomVertexRequest<'_>,
    ) -> Result<(), ObjectCustomVertexError> {
        let source = request.source;
        //482400 OR BYTE265,8. The retained WORD264 owns both byte halves.
        self.family4236d0_actor_mut(source)?.word_264 |= 0x0800;
        let index = request.compute.extras.first().copied().ok_or(
            ObjectCustomVertexError::RequiredExtra {
                object: source,
                selector: request.compute.selector,
                index: 0,
            },
        )?;
        let world = request.vertices.row_words_439e10(index)?;
        let Family4236d0Class::Selector36(state) = &mut self.family4236d0_actor_mut(source)?.class
        else {
            unreachable!("actual source callback checked");
        };
        //482411..41A lends the actual retained row to439E10, then48241F..43A
        //publishes its signed WORDs. Unknown row containment is checked before
        //these stores; arbitrary native buffer aliases are outside this loan.
        state.words_278_27c = world.map(Some);
        *request.output = Some(world.map(|axis| i32::from(axis as i16)));
        Ok(())
    }

    /// 00425870 on an actual selector10: the helper's leg solve, then the
    /// lifecycle-1 trail at the hip row. A base child's selector 5 leaves
    /// the output unwritten, as natively.
    fn selector10_model_vertex(
        &mut self,
        request: ObjectCustomVertexRequest<'_>,
    ) -> Result<(), ObjectCustomVertexError> {
        let object = request.source;
        let selector = request.compute.selector;
        let effects = request
            .effects
            .ok_or(ObjectCustomVertexError::RequiredEffects { object, selector })?;
        let legs = effects
            .legs()
            .map_err(|reason| ObjectCustomVertexError::Effects { object, reason })?;
        let actor = self.actor(object)?;
        let basis = std::array::from_fn(|i| f32::from_bits(actor.rotating.basis.words[i + 1]));
        let lifecycle = actor.word_c4 as i32;
        let word_188 = self.family428650_actor(object)?.words_188_19c[0];
        let family = self.family4236d0_actor(object)?;
        let (word_240, table_244) = (family.word_240, family.table_244);
        let inputs = Selector10CustomInputs {
            selector,
            extras: request.compute.extras,
            rows: request.vertices,
            basis,
            lifecycle,
            word_188,
            word_240,
            table_244,
            executable: legs.executable,
            sine: &legs.sine,
        };
        let helper = self.selector10_actor_mut(object)?.helper_26c.as_deref_mut();
        if let Some(output) = selector10_custom_425870(helper, &inputs, legs.services)
            .map_err(|error| ObjectCustomVertexError::Legs { object, error })?
        {
            *request.output = Some(output);
        }
        Ok(())
    }

    /// Exact fullDWORD9 of 00480CF0. The constructor leaves198 unknown;
    /// nonzero184 never retries extras/439E10, even after an earlier failure.
    fn cached_selector60_model_vertex(
        &mut self,
        request: ObjectCustomVertexRequest<'_>,
    ) -> Result<(), ObjectCustomVertexError> {
        let source = request.source;
        let DerivedActorClass::Selector60(state) = &mut self.derived_actor_mut(source)?.class
        else {
            unreachable!("actual source callback checked");
        };
        if state.byte_184 == 0 {
            //480D69 stores1 before the unsigned WORD read at480D76. The safe
            //extras loan does not represent the earlier raw pointer load.
            state.byte_184 = 1;
            let index = request.compute.extras.first().copied().ok_or(
                ObjectCustomVertexError::RequiredExtra {
                    object: source,
                    selector: request.compute.selector,
                    index: 0,
                },
            )?;
            //480D7E passes actual198/19A/19C to439E10. The portable current
            //row and retained cache both require one complete known triple.
            state.position_198 = Some(request.vertices.row_words_439e10(index)?);
        }
        let world = state
            .position_198
            .ok_or(ObjectCustomVertexError::RequiredField {
                object: source,
                offset: 0x198,
            })?;
        //480D83..DA3 publish retained signed WORDs, not actorXYZ or a new row.
        *request.output = Some(world.map(|axis| i32::from(axis as i16)));
        Ok(())
    }

    /// 0048CF70, in the unsigned-WORD index domain of 004260C0. Count+0D
    /// remains signedBYTE; an out-of-count index reads neither backing nor
    /// actor position/basis. The all-or-none actor XYZ owner fails at firstX.
    fn capture_model_behavior_group(
        &mut self,
        source: ObjectId,
        index: u16,
        world: [u16; 3],
    ) -> Result<(), ObjectCustomVertexError> {
        let behavior = self
            .actor_behavior_storage(source)?
            .as_ref()
            .expect("reached nonnull140");
        if i32::from(index) >= i32::from(behavior.byte_0d as i8) {
            return Ok(());
        }
        let groups = behavior.groups_1c.len();
        if usize::from(index) >= groups {
            return Err(ObjectCustomVertexError::BehaviorGroup {
                object: source,
                index,
                groups,
            });
        }
        let group = &mut self
            .actor_behavior_storage_mut(source)?
            .as_mut()
            .unwrap()
            .groups_1c[usize::from(index)];
        //48CF93 DWORD+6,48CF99 WORD+A precede flags and actor reads.
        group.coordinates_00_0a[3] = Some(world[0]);
        group.coordinates_00_0a[4] = Some(world[1]);
        group.coordinates_00_0a[5] = Some(world[2]);
        if group.word_0c & 4 != 0 {
            group.word_0c &= !4; //48CFAE before the first actor coordinate.
            let mut relative = [0_i16; 3];
            for axis in 0..3 {
                let position = self.object(source)?.base.position_words.ok_or(
                    ObjectCustomVertexError::RequiredField {
                        object: source,
                        offset: 0x32 + axis as u32 * 2,
                    },
                )?[axis];
                relative[axis] = world[axis].wrapping_sub(position) as i16;
                self.actor_behavior_storage_mut(source)?
                    .as_mut()
                    .unwrap()
                    .groups_1c[usize::from(index)]
                .coordinates_00_0a[axis] = Some(relative[axis] as u16);
            }
            //47A660 supplies the actual actor+44 basis to490A80. It captures
            //all three signed input WORDs before ordered destination writes.
            let local = self.actor(source)?.rotating.basis.inverse_words(relative);
            let group = &mut self
                .actor_behavior_storage_mut(source)?
                .as_mut()
                .unwrap()
                .groups_1c[usize::from(index)];
            for axis in 0..3 {
                group.coordinates_00_0a[axis] = Some(local[axis] as u16);
            }
        }
        self.actor_behavior_storage_mut(source)?
            .as_mut()
            .unwrap()
            .groups_1c[usize::from(index)]
        .word_0c = 3;
        Ok(())
    }
}

/// Exact fullDWORD7 of 00480CF0 (480DAD). It publishes the unsigned extra0
/// row before reading the load factor, then births trail5 (extra1 0) or 6
/// (extra1 1) at that row. Any other extra1 passes the output pointer as the
/// type, which 00489D90 rejects before using the RNG; the velocity draw before
/// that call still happens. No object field is read or written.
fn selector60_trail_model_vertex(
    request: ObjectCustomVertexRequest<'_>,
) -> Result<(), ObjectCustomVertexError> {
    let object = request.source;
    let selector = request.compute.selector;
    let extras = request.compute.extras;
    let extra = |index: usize| {
        extras
            .get(index)
            .copied()
            .ok_or(ObjectCustomVertexError::RequiredExtra {
                object,
                selector,
                index,
            })
    };
    let row = request.vertices.row_words_439e10(extra(0)?)?;
    *request.output = Some(row.map(|axis| i32::from(axis as i16)));
    let effects = request
        .effects
        .ok_or(ObjectCustomVertexError::RequiredEffects { object, selector })?;
    let failed = |reason| ObjectCustomVertexError::Effects { object, reason };
    //480DE5..DF7: wrapping SUB, then signed division by2048 toward zero.
    let divisor = 0xc000_i32.wrapping_sub(effects.load_factor().map_err(failed)?) / 2048;
    if divisor >= 2 {
        let random = effects.random().map_err(failed)?;
        if i32::from(random as u16) % divisor != 0 {
            return Ok(());
        }
    }
    let particle_type = match extra(1)? {
        0 => Some(5),
        1 => Some(6),
        _ => None,
    };
    let random = effects.random().map_err(failed)?;
    let Some(particle_type) = particle_type else {
        return Ok(());
    };
    effects
        .birth_trail(FurnitureCustomTrail {
            particle_type,
            position: row,
            velocity: [0, 128 + ((random as u16) >> 9), 0],
            flags: 0x800,
        })
        .map_err(failed)
}

/// `0042E445`: the factory's chimney trail, as `00480CF0`'s exact 7 but with
/// the load slack divided by 8192. A type other than 5 or 6 (extra WORD 1
/// outside 0..1) passes the output pointer to `00489D90`, which births
/// nothing; the velocity draw still happens.
fn factory_trail_model_vertex(
    request: ObjectCustomVertexRequest<'_>,
) -> Result<(), ObjectCustomVertexError> {
    let object = request.source;
    let selector = request.compute.selector;
    let extras = request.compute.extras;
    let extra = |index: usize| {
        extras
            .get(index)
            .copied()
            .ok_or(ObjectCustomVertexError::RequiredExtra {
                object,
                selector,
                index,
            })
    };
    let row = request.vertices.row_words_439e10(extra(0)?)?;
    *request.output = Some(row.map(|axis| i32::from(axis as i16)));
    let effects = request
        .effects
        .ok_or(ObjectCustomVertexError::RequiredEffects { object, selector })?;
    let failed = |reason| ObjectCustomVertexError::Effects { object, reason };
    //0042E477..0042E48F: wrapping SUB, then signed division by 8192.
    let divisor = 0xc000_i32.wrapping_sub(effects.load_factor().map_err(failed)?) / 8192;
    if divisor >= 2 {
        let random = effects.random().map_err(failed)?;
        if i32::from(random as u16) % divisor != 0 {
            return Ok(());
        }
    }
    let particle_type = match extra(1)? {
        0 => Some(5),
        1 => Some(6),
        _ => None,
    };
    let random = effects.random().map_err(failed)?;
    let Some(particle_type) = particle_type else {
        return Ok(());
    };
    effects
        .birth_trail(FurnitureCustomTrail {
            particle_type,
            position: row,
            velocity: [0, 128 + ((random as u16) >> 9), 0],
            flags: 0x800,
        })
        .map_err(failed)
}

/// Actual source+10 identity, separate from the inherited family dispatcher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObjectCustomCallbackOrigin {
    Direct4260c0,
    Shared47a240,
    Selector15Factory42e3d0,
    Selector2Wheels415d30,
    Selector10Legs425870,
    Selector30Capture48b290,
    Selector36Wrapper4823f0,
    Selector60Cached480cf0,
    Selector77Capture4074e0,
}

fn object_custom_callback_origin(class: &DerivedActorClass) -> Option<ObjectCustomCallbackOrigin> {
    use ObjectCustomCallbackOrigin::{
        Direct4260c0, Selector36Wrapper4823f0, Selector60Cached480cf0,
    };
    match class {
        DerivedActorClass::Base | DerivedActorClass::Selector11(_) => Some(Direct4260c0),
        DerivedActorClass::Family428650(family) => match &family.class {
            Family428650Class::Base
            | Family428650Class::Selector34(_)
            | Family428650Class::Selector35(_) => Some(Direct4260c0),
            Family428650Class::Family429e20(family) => match &family.class {
                Family429e20Class::Base | Family429e20Class::Selector12(_) => Some(Direct4260c0),
                Family429e20Class::Family4236d0(family) => match &family.class {
                    Family4236d0Class::Base => Some(Direct4260c0),
                    Family4236d0Class::Selector36(_) => Some(Selector36Wrapper4823f0),
                    Family4236d0Class::Selector10(_) => {
                        Some(ObjectCustomCallbackOrigin::Selector10Legs425870)
                    }
                },
            },
        },
        DerivedActorClass::Selector60(_) => Some(Selector60Cached480cf0),
        DerivedActorClass::Selector77(_) => {
            Some(ObjectCustomCallbackOrigin::Selector77Capture4074e0)
        }
        DerivedActorClass::Selector2(_) => Some(ObjectCustomCallbackOrigin::Selector2Wheels415d30),
    }
}

#[cfg(test)]
mod tests;
