//! The pet and how it behaves. No I/O and no dependencies: every display, input
//! source and file format lives outside this crate.
mod activity;
mod companion;
mod pet;
mod sprite;

pub use activity::{Activity, Report, Signal};
pub use companion::Companion;
pub use pet::{Clip, Mood, Pet, PetError};
pub use sprite::Sprite;
