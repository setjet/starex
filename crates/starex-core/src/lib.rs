//! Core of the Starex launcher.
//!
//! This crate has no UI or Tauri dependency: it owns the game library, inspects game folders,
//! verifies game files against a manifest and prepares the command that launches a game. The
//! desktop application in `src-tauri` is a thin layer over it.

/// The number of a private test build, set at compile time by `STAREX_TEST_BUILD` (only the local
/// test-build script sets it). `None` in every normal and release build, which then behave exactly
/// as before. A test build keeps its own data folder, links and settings, apart from Starex.
pub const TEST_BUILD: Option<&str> = option_env!("STAREX_TEST_BUILD");

pub mod antivirus;
pub mod binary;
pub mod error;
pub mod inspect;
pub mod launch;
pub mod library;
pub mod manifest;
pub mod model;
pub mod pe;
pub mod runtimes;
pub mod savedb;
pub mod saves;
pub mod settings;
pub mod stores;
mod util;
pub mod verify;

pub use error::{Error, Result};
pub use util::now_secs;
