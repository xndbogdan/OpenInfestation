//! The actor markers of 00472D30 → 004736E0, for the phase-3 briefing and
//! the phase-7 map. A pass belongs to ONE wrapped map tile. The separate
//! mission annotation list is not fabricated here.

use thiserror::Error;

use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        native_trunc_low32, ActorExtension, DerivedActorClass, Family4236d0Class,
        Family428650Class, Family429e20Class, ObjectArena, ObjectError, ObjectId,
    },
    scene_sprite::{DirectSpriteMode, DirectSpriteQuad},
};

/// Virtual `+70`'s pickup spawner (selector 53): its kind `+138` and count
/// `+139`, both signed bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapSpawner {
    pub kind: i8,
    pub count: i8,
}

/// Virtual `+64`'s vehicle (selector 2): its signed lifecycle `+C4` and the
/// basis DWORDs `+48` and `+50` its `+48` (`004026B0`) exposes, the X row's
/// X and Z.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapVehicle {
    pub word_c4: i32,
    pub basis_x: [u32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BriefingMapActor {
    pub flags: u32,
    pub position_words: Option<[u16; 3]>,
    /// Signed native DWORD +B4, independent of lifecycle +C4 or owner +84.
    pub material_word_b4: i32,
    /// Virtual +70 returns the actor: the pickup spawner's 00404610.
    pub pickup_spawner: Option<MapSpawner>,
    /// Virtual +64 returns the actor: the vehicle's 00404610.
    pub vehicle: Option<MapVehicle>,
    /// Virtual +B8 returns the actor: the banner's 00404610.
    pub banner: bool,
}

impl BriefingMapActor {
    /// These admitted final vtables all have null 00401300 at +B8 but for
    /// the banner's, at +70 but for the pickup spawner's, and at +64 but for
    /// the vehicle's.
    /// Keep exhaustive class matches: a new class needs its own query proof.
    pub fn from_arena(arena: &ObjectArena, id: ObjectId) -> Result<Self, ObjectError> {
        let object = arena.object(id)?;
        let actor = arena.actor(id)?;
        let mut pickup_spawner = None;
        let mut vehicle = None;
        let mut banner = false;
        match &actor.extension {
            ActorExtension::Selector40(_) => banner = true,
            ActorExtension::Selector53(spawner) => {
                pickup_spawner = Some(MapSpawner {
                    kind: spawner.kind_138 as i8,
                    count: spawner.count_139 as i8,
                });
            }
            ActorExtension::Shared
            | ActorExtension::Selector78(_)
            | ActorExtension::Selector61(_)
            | ActorExtension::Selector62(_)
            | ActorExtension::Selector16(_)
            | ActorExtension::Selector30(_)
            | ActorExtension::Selector15(_)
            | ActorExtension::Selector37(_)
            | ActorExtension::Selector47(_)
            | ActorExtension::Selector49(_)
            | ActorExtension::Selector39(_)
            | ActorExtension::Selector88(_)
            | ActorExtension::Selector66(_)
            | ActorExtension::Selector90(_)
            | ActorExtension::Selector93(_)
            | ActorExtension::Selector69(_)
            | ActorExtension::Selector52(_)
            | ActorExtension::Selector73(_)
            | ActorExtension::Selector55(_)
            | ActorExtension::Selector17(_)
            | ActorExtension::Selector56
            | ActorExtension::Selector74(_)
            | ActorExtension::Selector84(_)
            | ActorExtension::Selector57
            | ActorExtension::Selector41
            | ActorExtension::Selector44(_)
            | ActorExtension::Selector59(_)
            | ActorExtension::Selector18(_)
            | ActorExtension::BehaviorResource(_) => {}
            ActorExtension::Derived(derived) => match &derived.class {
                DerivedActorClass::Selector2(_) => {
                    let words = actor.rotating.basis.words;
                    vehicle = Some(MapVehicle {
                        word_c4: actor.word_c4 as i32,
                        basis_x: [words[1], words[3]],
                    });
                }
                DerivedActorClass::Base
                | DerivedActorClass::Selector11(_)
                | DerivedActorClass::Selector60(_)
                | DerivedActorClass::Selector77(_) => {}
                DerivedActorClass::Family428650(family) => match &family.class {
                    Family428650Class::Base
                    | Family428650Class::Selector34(_)
                    | Family428650Class::Selector35(_) => {}
                    Family428650Class::Family429e20(family) => match &family.class {
                        Family429e20Class::Base | Family429e20Class::Selector12(_) => {}
                        Family429e20Class::Family4236d0(family) => match &family.class {
                            Family4236d0Class::Base
                            | Family4236d0Class::Selector10(_)
                            | Family4236d0Class::Selector36(_) => {}
                        },
                    },
                },
            },
        }
        Ok(Self {
            flags: object.base.flags,
            position_words: object.base.position_words,
            material_word_b4: actor.word_b4 as i32,
            pickup_spawner,
            vehicle,
            banner,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BriefingMarkerTile {
    pub origin: [i32; 2],
    pub size: [i32; 2],
}

#[derive(Clone, Copy, Debug)]
pub struct BriefingMarkerInputs {
    pub tile: BriefingMarkerTile,
    /// C390 supplies screen age in milliseconds shifted left by two; the
    /// map screen its age in microseconds over 1000.
    pub scan_age: i32,
    /// Actual map owner DWORD +24, not a presentation default.
    pub jitter_radius: i32,
    /// 00472DF4 reads preceding screen X from an unwritten stack local on
    /// the first actor. None fails at that read when jitter is nonzero.
    pub initial_jitter_x: Option<i32>,
    /// C390 supplies final marker argument zero; EDI starts with that value.
    /// Its low byte also gates the pickup spawners' markers: the map screen
    /// passes whether the player owns the Resource scanner (grant 0x13).
    pub initial_jitter_y: i32,
    pub network_active: bool,
    /// Display +24/+28/+2C, also owned by the active D3dFlatState.
    pub material_shifts: [u32; 3],
    /// `004730EE..004730FA`: the session's current phase `+64C`. Phase 3
    /// draws no vehicle trail.
    pub session_phase: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BriefingMarkerMaterialLookup {
    pub initial_slot: i32,
    /// Native +B4=2 obtains H7/56 before testing network0055F448. Only then
    /// does it replace the color with H7/5B; both lookups are required.
    pub replacement_slot: Option<i32>,
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum BriefingMarkerError<E> {
    #[error("admitted map marker has unknown object position WORDs")]
    MissingPosition,
    #[error("nonzero native marker jitter needs the first previous-X stack value")]
    MissingInitialJitterX,
    #[error("marker material/style lookup failed: {0}")]
    Resource(E),
    /// `[esp+18]` keeps the previous spawner's style word; the pass's first
    /// spawner of a kind outside 1..3 reads unwritten stack.
    #[error("a kind-{kind} spawner reads the style its predecessor left, and none did")]
    SpawnerStyle { kind: i8 },
}

/// Streaming actor-list planner. Resolve/draw each returned quad before
/// advancing the next actor, preserving reached RNG prefixes on errors.
pub struct BriefingMarkerPass {
    inputs: BriefingMarkerInputs,
    previous_x: Option<i32>,
    previous_y: i32,
    spawner_style: Option<u32>,
}

impl BriefingMarkerPass {
    pub fn new(inputs: BriefingMarkerInputs) -> Self {
        Self {
            previous_x: inputs.initial_jitter_x,
            previous_y: inputs.initial_jitter_y,
            spawner_style: None,
            inputs,
        }
    }

    /// The quads the actor submits through `004736E0`, in order. `material`
    /// converts the H7 entry resolved through 00472170/004721A0; `style`
    /// reads an H6 slot: 0E for a marker, after the spawner gate, and
    /// 22..24 for a spawner. A style's high byte is retained in the packet.
    pub fn actor<E>(
        &mut self,
        actor: BriefingMapActor,
        rng: &mut u32,
        mut material: impl FnMut(BriefingMarkerMaterialLookup) -> Result<u32, E>,
        mut style: impl FnMut(i32) -> Result<u32, E>,
    ) -> Result<Vec<DirectSpriteQuad>, BriefingMarkerError<E>> {
        // 00472D66..D8D: excluded actors do not read positions or colors. A
        // banner or a pickup spawner enters whatever its flags.
        if actor.flags & 0x10021 != 0x21 && !actor.banner && actor.pickup_spawner.is_none() {
            return Ok(Vec::new());
        }
        // 00472DA0..00472DC2: a banner blinks, shown while the scan's age
        // over 15 is even.
        if actor.banner && (self.inputs.scan_age / 15) % 2 != 0 {
            return Ok(Vec::new());
        }
        let [mut x, _, mut z] = actor
            .position_words
            .ok_or(BriefingMarkerError::MissingPosition)?;
        if self.inputs.jitter_radius != 0 {
            // Native 48F060 precedes the first previous-X local read.
            let first = name_entry_rng_next(rng) as i32 % self.inputs.jitter_radius;
            let previous_x = self
                .previous_x
                .ok_or(BriefingMarkerError::MissingInitialJitterX)?;
            x = first.wrapping_add(previous_x) as u16;
            let second = name_entry_rng_next(rng) as i32 % self.inputs.jitter_radius;
            z = second.wrapping_add(self.previous_y) as u16;
        }
        let tile = self.inputs.tile;
        let projected = |word: u16, size: i32| {
            (i32::from(word) / 2).wrapping_mul(size).wrapping_shr(15) as i16 as i32
        };
        let center_x = projected(x, tile.size[0]).wrapping_add(tile.origin[0]);
        let center_y = tile.origin[1]
            .wrapping_sub(projected(z, tile.size[1]))
            .wrapping_add(tile.size[1]);
        self.previous_x = Some(center_x);
        self.previous_y = center_y;
        // 00472170's lookup is reached after both jitter draws/coordinates.
        let mut material = material(briefing_marker_material_lookup(
            actor.material_word_b4,
            self.inputs.network_active,
        ))
        .map_err(BriefingMarkerError::Resource)?;
        // 00472E93..00472EA7: a spawner draws only with the final argument's
        // low byte set, which C390 never passes.
        if let Some(spawner) = actor.pickup_spawner {
            if self.inputs.initial_jitter_y as u8 == 0 {
                return Ok(Vec::new());
            }
            // 00472EB5..00472F4A: kinds 1, 2 and 3 take H6 22, 23 and 24.
            if let 1..=3 = spawner.kind {
                self.spawner_style = Some(
                    style(0x21 + i32::from(spawner.kind)).map_err(BriefingMarkerError::Resource)?,
                );
            }
            let color_word = self
                .spawner_style
                .ok_or(BriefingMarkerError::SpawnerStyle { kind: spawner.kind })?;
            // 00472F56..00472F77: twice the count is the half size.
            return Ok(vec![blip(
                [center_x, center_y],
                i32::from(spawner.count).wrapping_mul(2),
                color_word,
            )]);
        }
        if actor.flags & 0x8000 != 0 {
            material = (material & marker_dim_mask(self.inputs.material_shifts)) >> 1;
        }
        let scan = (i32::from(z)
            .wrapping_sub(self.inputs.scan_age.wrapping_mul(0x40))
            .wrapping_sub(0x8000)
            >> 8)
            & 0xff;
        let radius = scan.wrapping_mul(tile.size[1]) / 0x4000;
        let shifts = self.inputs.material_shifts;
        let base = [
            (material >> (shifts[0].wrapping_add(1) & 31)) & 0xf8,
            (material >> (shifts[1].wrapping_add(1) & 31)) & 0xf8,
            (u32::from(material as u8) << (shifts[2].wrapping_sub(1) & 31)) & 0xf8,
        ];
        let style = style(0x0e).map_err(BriefingMarkerError::Resource)?;
        let rgb = std::array::from_fn::<_, 3, _>(|channel| {
            let value = (style >> (channel * 8)) & 0xff;
            (((value * scan as u32) >> 8) * base[channel] >> 8) as u8
        });
        let color_word = (style & 0xff00_0000)
            | u32::from(rgb[0])
            | (u32::from(rgb[1]) << 8)
            | (u32::from(rgb[2]) << 16);
        let center = [center_x, center_y];
        let mut quads = vec![blip(center, radius, color_word)];
        if let Some(vehicle) = actor.vehicle {
            if vehicle.word_c4 >= 3 && self.inputs.session_phase != 3 {
                quads.extend(vehicle_trail(vehicle, center, radius, color_word));
            }
        }
        Ok(quads)
    }
}

/// `004736E0(x, y, half_size, colour)`: H3 7 as a Near effect quad at depth
/// 0x40 with descriptor zero, from the low WORDs of the sums.
pub fn blip(center: [i32; 2], half_size: i32, color_word: u32) -> DirectSpriteQuad {
    let [x, y] = center;
    DirectSpriteQuad {
        sprite_slot: 7,
        corners: [
            [x.wrapping_sub(half_size), y.wrapping_sub(half_size)],
            [x.wrapping_add(half_size), y.wrapping_sub(half_size)],
            [x.wrapping_add(half_size), y.wrapping_add(half_size)],
            [x.wrapping_sub(half_size), y.wrapping_add(half_size)],
        ]
        .map(|pair| pair.map(|value| value as i16)),
        vertex_depths: [64; 4],
        descriptor_word: 0,
        mode: DirectSpriteMode::Effect { color_word },
    }
}

/// `004730D7..0047323B`: eight pairs of the marker's blip along the
/// vehicle's X row, scaled by 16 (`ftol` of the products with -16.0 and
/// 16.0, as signed WORDs). Four accumulators start at zero, so the first
/// pair lies on the marker; each pair is drawn before they step, and every
/// position is an eighth of its accumulator (signed truncation).
fn vehicle_trail(
    vehicle: MapVehicle,
    [x, y]: [i32; 2],
    radius: i32,
    color_word: u32,
) -> Vec<DirectSpriteQuad> {
    let scaled = |word: u32, factor: f64| {
        native_trunc_low32(f64::from(f32::from_bits(word)) * factor) as i16 as i32
    };
    let across = scaled(vehicle.basis_x[1], -16.0);
    let along = scaled(vehicle.basis_x[0], 16.0);
    let steps = [
        along.wrapping_sub(across / 2),
        (along / 2).wrapping_add(across),
        (across / 2).wrapping_add(along),
        (along / 2).wrapping_sub(across),
    ];
    let mut sums = [0i32; 4];
    let mut quads = Vec::with_capacity(16);
    for _ in 0..8 {
        let first = [
            x.wrapping_sub(sums[1] / 8) as i16 as i32,
            y.wrapping_add(sums[0] / 8) as i16 as i32,
        ];
        let second = [
            x.wrapping_add(sums[3] / 8) as i16 as i32,
            y.wrapping_add(sums[2] / 8) as i16 as i32,
        ];
        quads.push(blip(first, radius, color_word));
        quads.push(blip(second, radius, color_word));
        for (sum, step) in sums.iter_mut().zip(steps) {
            *sum = sum.wrapping_add(step);
        }
    }
    quads
}

/// 004721A0's H7 palette, retaining case 2's preliminary H7/56 lookup.
pub fn briefing_marker_material_lookup(
    word_b4: i32,
    network_active: bool,
) -> BriefingMarkerMaterialLookup {
    let initial_slot = match word_b4 {
        1 | 2 => 0x56,
        10 => 0x5b,
        3 | 11 => 0x5a,
        4 | 12 => 0x5c,
        5 | 13 => 0x5d,
        6 | 14 => 0x5e,
        7 | 15 => 0x5f,
        8 | 16 => 0x60,
        9 | 17 => 0x61,
        18 => 0x57,
        _ => 0x59,
    };
    BriefingMarkerMaterialLookup {
        initial_slot,
        replacement_slot: (word_b4 == 2 && network_active).then_some(0x5b),
    }
}

/// Renderer +30 is constructor 004A49B0's persistent dim mask, derived once
/// from the same display shifts. No later mask write has been identified.
pub(crate) fn marker_dim_mask(shifts: [u32; 3]) -> u32 {
    let carry = 0x100_u32.wrapping_shr(shifts[2].wrapping_sub(1))
        | 0x100_u32.wrapping_shl(shifts[1].wrapping_add(1))
        | 0x100_u32.wrapping_shl(shifts[0].wrapping_add(1));
    !carry & 0xffff
}

#[cfg(test)]
#[path = "briefing_markers_tests.rs"]
mod tests;
