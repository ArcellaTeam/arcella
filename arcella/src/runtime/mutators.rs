// arcella/arcella/src/runtime/mutators.rs
//
// Copyright (c) 2025 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use ministate::Mutator;

use arcella_types::{
    manifest::ComponentManifest, 
    //deployment::DeploymentSpec
};
use super::state::ArcellaState;
use crate::manifest::DeploymentSpec;

#[derive(serde::Serialize, serde::Deserialize)]
pub enum ArcellaMutation {
    InstallModule(InstallModule),
    DeployModule(DeployModule),
    //StartDeployment(StartDeployment),
    //StopDeployment(StopDeployment),
    // ...
}

impl Mutator<ArcellaState> for ArcellaMutation {
    fn apply(&self, state: &mut ArcellaState) {
        match self {
            ArcellaMutation::InstallModule(m) => m.apply(state),
            ArcellaMutation::DeployModule(m) => m.apply(state),
            //ArcellaMutation::StartDeployment(m) => m.apply(state),
            //ArcellaMutation::StopDeployment(m) => m.apply(state),
        }
    }
}

// Install
#[derive(serde::Serialize, serde::Deserialize)]
pub struct InstallModule {
    pub manifest: ComponentManifest,
    pub wasm_bytes: Vec<u8>, // или хэш, или путь — зависит от политики хранения
}

impl Mutator<ArcellaState> for InstallModule {
    fn apply(&self, state: &mut ArcellaState) {
        state.installed_modules.insert(self.manifest.id(), self.manifest.clone());
        // wasm_bytes можно сохранить в storage отдельно
    }
}

// Deploy
#[derive(serde::Serialize, serde::Deserialize)]
pub struct DeployModule {
    pub spec: DeploymentSpec,
}

impl Mutator<ArcellaState> for DeployModule {
    fn apply(&self, state: &mut ArcellaState) {
        state.deployments.insert(self.spec.module_id.clone(), self.spec.clone());
    }
}
