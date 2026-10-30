//! 00465FA0/00465570 projectile collision admission and response wrappers.
//! Geometry and damage services remain live: the temporary actor list retains
//! identities, while every 0047CEB0 query rereads the reached actor's state.

use inf_formats::{native_basis::isqrt_48efc0, FrontendExecutable};

use super::super::{
    audio::ParticleSoundRequest,
    births::ParticleBirthRequest,
    update::{ParticleUpdateError, ParticleUpdateHost, ParticleUpdateRequest},
    ParticleId, ProcessParticleRuntime,
};
use crate::{name_entry::name_entry_rng_next, object_runtime::ActorHandle};

/// Values read by the 004884F0 broadphase builder, in manager+48 list order.
/// They are not used by the subsequent precise geometry scans.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleActorCollisionCandidate {
    pub actor: ActorHandle,
    pub flags_2c: u32,
    pub radius_28: u16,
    pub position_32: [u16; 3],
    pub source_84: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleActorGeometryRequest {
    pub actor: ActorHandle,
    pub center: [u16; 3],
    pub radius: i32,
}

/// 00435CB0's retained contact+18/+1C, read by 00466CF0 only after a hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleFurnitureContact {
    pub xz: [u16; 2],
}

pub struct ParticleCollisionContext<'a, 'world, 'exe> {
    pub particles: &'a mut ProcessParticleRuntime,
    pub request: &'a mut ParticleUpdateRequest<'world>,
    pub executable: FrontendExecutable<'exe>,
    pub rng: &'a mut u32,
}

/// Reached live services. Each callback must preserve its completed prefix on
/// failure; the particle manager owns containment, retirement and retry policy.
pub trait ParticleProjectileHost: ParticleUpdateHost {
    fn actor_geometry(
        &mut self,
        request: ParticleActorGeometryRequest,
        context: &mut ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<bool, Self::Error>;

    fn furniture_query(
        &mut self,
        center: [u16; 3],
        radius: i32,
        wave_clock: u32,
        context: &mut ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<Option<ParticleFurnitureContact>, Self::Error>;

    /// 00466080: 0047C6E0 followed by live manager+1BC statistics and the
    /// source47F750 -> target47F710 -> captured481050 chain.
    fn actor_impact(
        &mut self,
        particle: ParticleId,
        target: ActorHandle,
        packet: u32,
        source: u32,
        context: &mut ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<(), Self::Error>;

    /// Non-native: a projectile `id` met the furniture item at `contact`'s
    /// cell, its refined probe centre `at` (world words), before any impact
    /// service runs. The port's crystals flare from it
    /// ([`crate::crystal_flares`]); retail does nothing here.
    fn furniture_contact(
        &mut self,
        _id: ParticleId,
        _contact: ParticleFurnitureContact,
        _at: [u16; 3],
        _context: &mut ParticleCollisionContext<'_, '_, '_>,
    ) {
    }

    /// 00466CF0 -> 00436020, with native contact cell coordinates.
    fn furniture_impact(
        &mut self,
        contact: ParticleFurnitureContact,
        packet: u32,
        context: &mut ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<(), Self::Error>;

    /// 00466530 non-water arm: 00489390(selector,0x800) -> 00488E00.
    fn terrain_impact(
        &mut self,
        selector: u32,
        position: [u16; 3],
        strength: u32,
        context: &mut ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<(), Self::Error>;

    /// 004887D0: terrain436390, actors479870, then the conditional network
    /// notice492500. Reached only after a true collision and a nonnull packet.
    fn radial_impact(
        &mut self,
        packet: u32,
        particle: ParticleId,
        source: u32,
        context: &mut ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<(), Self::Error>;
}

impl ProcessParticleRuntime {
    /// 00465FA0 after its animation-radius calculation. Source identities,
    /// flags and secondary packets are reread at their actual native sites.
    pub fn collide_projectile_ordered<H: ParticleProjectileHost>(
        &mut self,
        id: ParticleId,
        movement: [u16; 3],
        radius: i32,
        candidates: &[ParticleActorCollisionCandidate],
        request: &mut ParticleUpdateRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
        host: &mut H,
    ) -> Result<bool, ParticleUpdateError<H::Error>> {
        let mut context = ParticleCollisionContext {
            particles: self,
            request,
            executable,
            rng,
        };
        let hit = collide_actors(id, movement, radius, candidates, &mut context, host)?
            || collide_furniture(id, movement, radius, &mut context, host)?
            || collide_terrain(id, movement, radius, &mut context, host)?;
        if hit {
            let packet = context.particles.record(id)?.read_dword(0x38)?;
            if packet != 0 && context.particles.record(id)?.read_word(0x12)? & 1 == 0 {
                let source = context.particles.record(id)?.read_dword(0x14)?;
                host.radial_impact(packet, id, source, &mut context)
                    .map_err(ParticleUpdateError::CollisionService)?;
            }
        }
        Ok(hit)
    }
}

/// Signed DWORD sum, integer root and signed low WORD are all intentional.
fn movement_half_length(movement: [u16; 3]) -> i32 {
    let squared = movement.into_iter().fold(0_i32, |sum, component| {
        let value = i32::from(component as i16);
        sum.wrapping_add(value.wrapping_mul(value))
    });
    i32::from(isqrt_48efc0(squared) as i16) / 2
}

fn midpoint(position: [u16; 3], movement: [u16; 3]) -> ([u16; 3], [i16; 3]) {
    let half = movement.map(|word| word as i16 / 2);
    (
        std::array::from_fn(|i| position[i].wrapping_sub(half[i] as u16)),
        half,
    )
}

/// DWORD differences here, unlike the low-WORD differences in 0047CEB0.
fn broadphase(center: [u16; 3], radius: i32, candidate: ParticleActorCollisionCandidate) -> bool {
    let combined = radius.wrapping_add(i32::from(candidate.radius_28));
    let extent = combined.wrapping_mul(2) as u32;
    let mut squared = 0_i32;
    for i in 0..3 {
        let delta = i32::from(center[i] as i16) - i32::from(candidate.position_32[i] as i16);
        if combined.wrapping_add(delta) as u32 > extent {
            return false;
        }
        squared = squared.wrapping_add(delta.wrapping_mul(delta));
    }
    squared as u32 <= combined.wrapping_mul(combined) as u32
}

/// 00465390 into 004884F0: the broadphase candidates, then 0047DA70 queries
/// each in order (0047CEB0) and takes the first hit.
fn collide_actors<H: ParticleProjectileHost>(
    id: ParticleId,
    movement: [u16; 3],
    radius: i32,
    candidates: &[ParticleActorCollisionCandidate],
    context: &mut ParticleCollisionContext<'_, '_, '_>,
    host: &mut H,
) -> Result<bool, ParticleUpdateError<H::Error>> {
    let position = context.particles.record(id)?.position()?;
    let (mut center, mut half) = midpoint(position, movement);
    let scan_radius = movement_half_length(movement).max(radius) as i16 as i32;
    let mut actors = Vec::new();
    for candidate in candidates {
        if candidate.flags_2c & 4 != 0
            && candidate.radius_28 != 0
            && broadphase(center, scan_radius, *candidate)
            && candidate.source_84 != context.particles.record(id)?.read_dword(0x14)?
        {
            actors.push(candidate.actor);
        }
    }
    let mut query =
        |center, scan_radius| -> Result<Option<ActorHandle>, ParticleUpdateError<H::Error>> {
            for &actor in &actors {
                if host
                    .actor_geometry(
                        ParticleActorGeometryRequest {
                            actor,
                            center,
                            radius: scan_radius,
                        },
                        context,
                    )
                    .map_err(ParticleUpdateError::CollisionService)?
                {
                    return Ok(Some(actor));
                }
            }
            Ok(None)
        };
    let Some(target) = subdivide(
        &mut center,
        &mut half,
        scan_radius,
        radius,
        &mut query,
        || ParticleUpdateError::PortableNonProgressingProjectileSubdivision { id },
    )?
    else {
        return Ok(false);
    };
    if context.particles.record(id)?.read_word(0x12)? & 1 == 0 {
        let packet = context.particles.record(id)?.read_dword(0x34)?;
        let source = context.particles.record(id)?.read_dword(0x14)?;
        host.actor_impact(id, target, packet, source, context)
            .map_err(ParticleUpdateError::CollisionService)?;
    }
    Ok(true)
}

/// 004653B0: the furniture probe (00435CB0).
fn collide_furniture<H: ParticleProjectileHost>(
    id: ParticleId,
    movement: [u16; 3],
    radius: i32,
    context: &mut ParticleCollisionContext<'_, '_, '_>,
    host: &mut H,
) -> Result<bool, ParticleUpdateError<H::Error>> {
    let position = context.particles.record(id)?.position()?;
    let (mut center, mut half) = midpoint(position, movement);
    //465413 adds the particle radius; actor4885C2 takes max instead.
    let scan_radius = movement_half_length(movement).wrapping_add(radius);
    let wave_clock = context
        .request
        .terrain
        .as_ref()
        .ok_or(ParticleUpdateError::MissingTerrain { id })?
        .wave_clock;
    let mut query = |center, scan_radius: i32| {
        host.furniture_query(center, scan_radius.wrapping_add(4), wave_clock, context)
            .map_err(ParticleUpdateError::CollisionService)
    };
    let Some(contact) = subdivide(
        &mut center,
        &mut half,
        scan_radius,
        radius,
        &mut query,
        || ParticleUpdateError::PortableNonProgressingProjectileSubdivision { id },
    )?
    else {
        return Ok(false);
    };
    host.furniture_contact(id, contact, center, context);
    if context.particles.record(id)?.read_word(0x12)? & 1 == 0 {
        let packet = context.particles.record(id)?.read_dword(0x34)?;
        host.furniture_impact(contact, packet, context)
            .map_err(ParticleUpdateError::CollisionService)?;
    }
    Ok(true)
}

/// Shared arithmetic only: the actor and furniture callers supply their
/// distinct initial radius and query policy. Native search tries the earlier
/// half first, then the later half if that query misses, and stops on both miss.
fn subdivide<T, E>(
    center: &mut [u16; 3],
    half: &mut [i16; 3],
    mut scan_radius: i32,
    radius: i32,
    query: &mut impl FnMut([u16; 3], i32) -> Result<Option<T>, E>,
    stalled: impl FnOnce() -> E,
) -> Result<Option<T>, E> {
    let mut hit = query(*center, scan_radius)?;
    while hit.is_some() && i32::from(scan_radius as i16) > radius {
        let before = (*center, *half, scan_radius);
        scan_radius = (i32::from(scan_radius as i16) / 2).max(radius);
        for axis in 0..3 {
            half[axis] /= 2;
            center[axis] = center[axis].wrapping_sub(half[axis] as u16);
        }
        hit = query(*center, scan_radius)?;
        if hit.is_none() {
            for axis in 0..3 {
                center[axis] = center[axis].wrapping_add((half[axis] as u16).wrapping_mul(2));
            }
            hit = query(*center, scan_radius)?;
        }
        // A negative native radius can keep the integer radius/half at zero
        // forever. This portable guard preserves the completed query prefix;
        // it is not a native false/no-contact result.
        if hit.is_some() && before == (*center, *half, scan_radius) {
            return Err(stalled());
        }
    }
    Ok(hit)
}

#[cfg(test)]
mod tests;

fn collide_terrain<H: ParticleProjectileHost>(
    id: ParticleId,
    movement: [u16; 3],
    radius: i32,
    context: &mut ParticleCollisionContext<'_, '_, '_>,
    host: &mut H,
) -> Result<bool, ParticleUpdateError<H::Error>> {
    let Some(contact) = context
        .particles
        .classify_terrain_contact(id, radius, context.request)?
    else {
        return Ok(false);
    };
    // 00465110 calls +10(selector, height, movement, radius).
    match context.particles.record(id)?.read_dword(0)? {
        // 00466A00: water (6) and selector 7 end the particle. Any other
        // contact rests it at height + radius and turns a fall into a small
        // rise of 1 - vy/4, truncated.
        0x0052_6f60 => {
            if matches!(contact.selector, 6 | 7) {
                return Ok(true);
            }
            let record = context.particles.record_mut(id)?;
            record.write_word(6, contact.height.wrapping_add(radius) as u16)?;
            let vy = record.read_word(0x0c)? as i16;
            if vy < 0 {
                record.write_word(0x0c, (1 - i32::from(vy) / 4) as u16)?;
            }
            return Ok(false);
        }
        // 004661F0: a non-water contact rests at the height and reverses
        // +0C. Water takes the projectile's sound, height and splash arm.
        0x0052_6f20 if contact.selector != 6 => {
            let record = context.particles.record_mut(id)?;
            record.write_word(6, contact.height as u16)?;
            let vy = record.read_word(0x0c)?;
            record.write_word(0x0c, vy.wrapping_neg())?;
            return Ok(false);
        }
        _ => {}
    }
    if contact.selector != 6 {
        let position = context.particles.record(id)?.position()?;
        let previous = std::array::from_fn(|i| position[i].wrapping_sub(movement[i]));
        host.terrain_impact(contact.selector, previous, 0x800, context)
            .map_err(ParticleUpdateError::CollisionService)?;
        return Ok(true);
    }
    // Water owns an optional sound, then Y publication, then splash. It returns
    // false, so the projectile continues; even this callback's flag1 is ignored.
    if name_entry_rng_next(context.rng) & 3 == 0 {
        // 004655B0: sound 9 or 10 by the draw's bit 12, then the pitch draw.
        let sound = if name_entry_rng_next(context.rng) & 0x1000 != 0 {
            9
        } else {
            10
        };
        let pitch = ((name_entry_rng_next(context.rng) & 0xffff) >> 2) + 0xc000;
        let position = context.particles.record(id)?.position()?;
        host.impact_sound(
            ParticleSoundRequest {
                sound,
                position,
                volume: 65536,
                pitch,
                flags: 0,
            },
            context.particles,
            context.rng,
        )
        .map_err(ParticleUpdateError::Audio)?;
    }
    context
        .particles
        .record_mut(id)?
        .write_word(6, contact.height as u16)?;
    let position = context.particles.record(id)?.position()?;
    context.particles.birth_splash(
        ParticleBirthRequest {
            particle_type: 0,
            position,
            velocity: None,
            flags: 0,
            inputs: context.request.birth_inputs.borrowed(),
        },
        context.executable,
        context.rng,
    )?;
    Ok(false)
}
