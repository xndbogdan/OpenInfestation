//! Shared auxiliary contact geometry behind 004F1420 and 004F2B30, and the
//! terrain sphere kernel 0044EC90.
//!
//! These kernels own only supplied primitive inputs and the native 20-byte
//! contact's reached writes. Recursive 004F2C50 execution, vertex production,
//! model admission and actor/furniture callbacks remain separate owners.

mod box_sphere;
mod contact;
mod primitives;
mod sphere_box;
mod terrain_sphere;

pub use contact::CollisionContact;
pub use primitives::{apply_contact_geometry, ContactGeometryRequest, ContactSphere};
pub use sphere_box::ContactAxisNormals;
pub use terrain_sphere::{terrain_sphere_contact, TerrainSphereError, TerrainSphereGrid};

#[cfg(test)]
mod tests;
