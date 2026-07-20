//! Pixtimize application library.
//!
//! The crate is a layered modular monolith:
//! HTTP concerns live in [`api`], use-case orchestration in [`application`],
//! business rules in [`domain`], and external-system adapters in
//! [`infrastructure`]. [`app`] is the composition root that wires the layers
//! together.

pub mod api;
pub mod app;
pub mod application;
pub mod config;
pub mod domain;
pub mod error;
pub mod infrastructure;
