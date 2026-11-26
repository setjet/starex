//! Core of the Starex launcher.
//!
//! This crate has no UI or Tauri dependency: it owns the game library, inspects game folders,
//! verifies game files against a manifest and prepares the command that launches a game. The
//! desktop application in `src-tauri` is a thin layer over it.

pub mod antivirus;
pub mod binary;
pub mod error;
pub mod inspect;
pub mod launch;
pub mod library;
pub mod manifest;
pub mod model;
pub mod pe;
pub mod saves;
pub mod settings;
mod util;
pub mod verify;

pub use error::{Error, Result};
pub use util::now_secs;
