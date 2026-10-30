//! Frontend-facing names for the shared native basis kernels.
//!
//! Existing menu/model execution APIs use these names. The implementation and
//! its arithmetic regressions live in inf-formats so asset tools share exactly
//! the same original-table rotations without depending on the game runtime.

pub(crate) use inf_formats::native_basis::sine_q31;
pub use inf_formats::native_basis::{NativeBasisMath as MenuBasisMath, RotationPlane};
