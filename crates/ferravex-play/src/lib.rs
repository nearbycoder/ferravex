//! Original, dependency-free gameplay systems shared by small Ferravex games.
//! Arrays keep these systems independent of a renderer or a particular math crate.
#![forbid(unsafe_code)]

pub mod animation;
pub mod navigation;
pub mod particles;
pub mod rounds;
pub mod scene;
pub mod ui;
