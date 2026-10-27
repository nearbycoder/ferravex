//! Copper Greens: twelve authored miniature-golf holes, shared by two views.
//!
//! Rules run at 120 Hz independently of rendering and platform input. Both
//! front ends use the same ball, hazards, course geometry and scorecard.
#![doc = include_str!("../README.md")]

pub mod capture;
pub mod card;
pub mod course;
pub mod game;
pub mod juice;
pub mod presentation;
pub mod quips;
pub use presentation::{App, View};
