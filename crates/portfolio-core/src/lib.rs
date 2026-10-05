//! Pure domain logic for Portfolio Desk.
//!
//! This crate has no Tauri, filesystem, network, or database dependencies so it
//! can be reused unchanged on Windows, macOS, and iOS.

pub mod accounting;
pub mod address;
pub mod clock;
pub mod decimal;
pub mod error;
pub mod network;

pub use error::CoreError;
