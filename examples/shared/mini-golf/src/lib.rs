//! Copper Greens: twelve authored miniature-golf holes, shared by two views.
//!
//! Rules run at 120 Hz independently of rendering and platform input. Both
//! front ends use the same ball, hazards, course geometry and scorecard.
#![doc = include_str!("../README.md")]

pub mod course;
pub mod game;
pub mod presentation;
pub use presentation::{App, View};
