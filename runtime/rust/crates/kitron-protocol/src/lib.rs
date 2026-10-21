//! Kitron runtime protocol types.
//!
//! This crate defines every value that crosses a process boundary in Kitron:
//! JS↔Rust payloads, Rust↔Node frames, config schemas and the typed error
//! model from the architecture spec (`instruction.md` §33).

pub mod config;
pub mod error;
pub mod wire;

pub use config::*;
pub use error::*;
pub use wire::*;
