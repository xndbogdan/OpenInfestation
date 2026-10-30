//! Interactive, provenance-conscious workbench for raw Infestation assets.
//!
//! The viewer reads retail containers through `inf-formats`; it never depends
//! on generated extraction output. Models use recovered Handler-3 textures,
//! Handler-7 colors, culling, UVs, and a software depth buffer while retaining
//! explicit diagnostics for runtime-dependent geometry and lighting.

pub mod app;
pub mod camera;
mod capture_view;
mod captured_model;
pub mod catalog;
pub mod input;
mod inspection_eye;
mod model_composite;
mod model_presentation;
pub mod render;
pub mod ui;
mod view;
