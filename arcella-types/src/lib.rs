// arcella/arcella-types/src/lib.rs
//
// Copyright (c) 2025 Alexey Rybakov, Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

pub mod alme;
pub mod config;
pub mod manifest;
pub mod module_id;
pub mod interface_list;
pub mod spec;

mod error;
pub use error::{ArcellaError as ArcellaTypeError, ArcellaResult as ArcellaTypeResult};
