// arcella/arcella-core/src/runtime/mutators.rs
//
// Copyright (c) 2025 Alexey Rybakov, Arcella Team
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
    //pub wasm_bytes: Vec<u8>, // или хэш, или путь — зависит от политики хранения
}

impl Mutator<ArcellaState> for InstallModule {
    fn apply(&self, state: &mut ArcellaState) {
        state.installed_modules.insert(self.manifest.id.to_string(), self.manifest.clone());
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
        state.deployments.insert(self.spec.module_id.to_string(), self.spec.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::test_utils::create_test_manifest;

    #[cfg(test)]
    mod serialization_tests {
        use super::*;
        use std::fs;
        use tempfile::TempDir;
        use serde_json;

        #[test]
        fn test_install_module_jsonl_roundtrip() {
            let temp_dir = TempDir::new().unwrap();
            let jsonl_path = temp_dir.path().join("install_module.jsonl");

            // 1. Создаём мутацию
            let manifest = create_test_manifest().unwrap();
            let mutation = InstallModule {
                manifest: manifest.clone(),
            };

            // 2. Сериализуем в JSONL (одна запись — одна строка)
            let json_line = serde_json::to_string(&mutation).unwrap();
            fs::write(&jsonl_path, json_line + "\n").unwrap();

            // 3. Читаем обратно из JSONL
            let contents = fs::read_to_string(&jsonl_path).unwrap();
            let lines: Vec<&str> = contents.lines().collect();
            assert_eq!(lines.len(), 1);

            let restored: InstallModule = serde_json::from_str(lines[0]).unwrap();

            // 4. Проверяем, что восстановленная мутация идентична исходной
            assert_eq!(restored.manifest.id, mutation.manifest.id);
            assert_eq!(restored.manifest.description, mutation.manifest.description);
            assert_eq!(restored.manifest.exports, mutation.manifest.exports);
            assert_eq!(restored.manifest.imports, mutation.manifest.imports);
        }
    }    

    #[test]
    fn test_install_module() {
        let mut state = ArcellaState::default();
        let manifest = create_test_manifest().unwrap();
        let module_id = manifest.id.to_string();

        let mutation = InstallModule {
            manifest: manifest.clone(),
        };

        // Применяем мутацию
        mutation.apply(&mut state);

        // Проверяем, что модуль появился
        assert!(state.installed_modules.contains_key(&module_id));
        assert_eq!(state.installed_modules.get(&module_id).unwrap(), &manifest);
    }    

    #[test]
    fn test_install_module_idempotent() {
        let mut state = ArcellaState::default();
        let manifest = create_test_manifest().unwrap();
        let module_id = manifest.id.to_string();

        let mutation = InstallModule {
            manifest: manifest.clone(),
        };

        // Применяем дважды
        mutation.apply(&mut state);
        mutation.apply(&mut state);

        // Должен остаться ровно один модуль
        assert_eq!(state.installed_modules.len(), 1);
        assert_eq!(state.installed_modules.get(&module_id).unwrap(), &manifest);
    }

}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::fs;
    use tempfile::TempDir;

    use ministate::StateManager;

    use crate::manifest::load_component_manifest_from_toml;

    fn create_test_manifest() -> Option<ComponentManifest> {
        let temp_dir = TempDir::new().unwrap();
        let toml_path = temp_dir.path().join("component.toml");

        let toml_content = r#"
            [component]
            name = "test-component"
            version = "0.1.0"
            description = "A test component"
            exports = ["foo:bar@1.0"]
            imports = ["wasi:cli@0.2.0"]
        "#;

        fs::write(&toml_path, toml_content).unwrap();
        load_component_manifest_from_toml(&toml_path).unwrap()
    }

    async fn new_tmp_state_manager(state_dir: &PathBuf) -> StateManager<ArcellaState, InstallModule> {
        StateManager::open(state_dir, "counter.wal.jsonl")
            .await
            .unwrap()
    }    

    #[tokio::test]
    async fn test_state_recovery_from_wal() {
        let temp_dir = TempDir::new().unwrap();
        let state_dir = temp_dir.path().to_path_buf();

        // === Шаг 1: Открываем менеджер и устанавливаем модуль ===
        let mut manager = new_tmp_state_manager(&state_dir).await;

        let manifest = create_test_manifest().unwrap();
        let module_id = manifest.id.to_string();

        let install_mutation = InstallModule {
            manifest: manifest.clone(),
        };

        manager.apply(install_mutation).await.unwrap();

        // Проверим, что сразу после apply состояние корректно
        assert!(manager.snapshot().await.installed_modules.contains_key(&module_id));

        // Явно закрываем (не обязательно, но для ясности)
        drop(manager);

        // === Шаг 2: Открываем заново — должно восстановиться из WAL ===
        let manager2 = new_tmp_state_manager(&state_dir).await;
        let recovered_state = manager2.snapshot().await;

        assert!(recovered_state.installed_modules.contains_key(&module_id));
        assert_eq!(
            recovered_state.installed_modules.get(&module_id).unwrap(),
            &manifest
        );
    }

}
