//! Original, dependency-free gameplay systems shared by small Ferravex games.
//! Renderer-independent systems; cameras share the first-party scalar math crate.
#![forbid(unsafe_code)]

pub mod ability;
pub mod ai;
pub mod animation;
pub mod attributes;
pub mod audio;
pub mod authoring;
pub mod camera;
pub mod feedback;
pub mod input;
pub mod motion;
pub mod navigation;
pub mod particles;
pub mod persistence;
pub mod replay;
pub mod rounds;
pub mod scene;
pub mod tuning;
pub mod ui;
