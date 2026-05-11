//! Core library for IHSTAY.
//!
//! This crate contains the platform-agnostic logic:
//!
//! - [`board`] — JSONL parser, in-memory state store, file watcher, and compaction
//! - [`visibility`] — finite state machine controlling HUD visibility, cooldown,
//!   and reminding behavior
//! - [`reaper`] — periodic liveness check that promotes dead Claude Code processes
//!   to stale entries on the board
//! - [`terminal`] — adapter trait and process ancestor walk for terminal focus
//! - [`config`] — user-editable settings persisted to TOML
//! - [`types`] — shared domain types (`Entry`, `Op`, `NotificationType`, etc.)
//!
//! The core crate has no Tauri dependency; the Tauri app in `crates/app`
//! composes these pieces.

pub mod board;
pub mod config;
pub mod reaper;
pub mod terminal;
pub mod types;
pub mod visibility;
