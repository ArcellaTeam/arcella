// arcella/arcella-core/src/runtime/state.rs
//
// Copyright (c) 2025 Alexey Rybakov, Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use std::collections::{HashMap, HashSet};
use crate::manifest::DeploymentSpec;

use arcella_types::{
    manifest::ComponentManifest, 
};

#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArcellaState {
    pub installed_modules: HashMap<String, ComponentManifest>,   // key = "name@version"
    pub deployments: HashMap<String, DeploymentSpec>,            // key = deployment_id
    pub running_instances: HashSet<String>,                      // instance_id или deployment_id
}
