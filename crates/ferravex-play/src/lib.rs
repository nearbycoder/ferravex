//! Original, dependency-free gameplay systems shared by small Ferravex games.
//! Renderer-independent systems; cameras share the first-party scalar math crate.
#![forbid(unsafe_code)]

pub mod ai;
pub mod animation;
pub mod audio;
pub mod camera;
pub mod input;
pub mod navigation;
pub mod particles;
pub mod rounds;
pub mod scene;
pub mod ui;
