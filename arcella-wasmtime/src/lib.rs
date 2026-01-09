// arcella/arcella-wasmtime/src/lib.rs
//
// Copyright (c) 2026 Alexey Rybakov, Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

//! Wasmtime-specific utilities for Arcella.
//!
//! This crate provides conversion from `wasmtime`'s internal component model types
//! into the stable, serializable types defined in `arcella-types`.
//!
//! It is intended for use by the Arcella runtime and CLI tools that need to
//! inspect WebAssembly components using Wasmtime as the engine.

mod engine;
mod from_wasmtime;
pub mod manifest;

pub use from_wasmtime::{ComponentItemSpecExt, ComponentTypeExt};
