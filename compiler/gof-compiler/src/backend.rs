use crate::ssa::SsaModule;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct BackendArtifact {
    pub backend: String,
    pub module_kind: String,
    pub ssa: SsaModule,
}

pub fn lower(ssa: &SsaModule, module_kind: &str) -> BackendArtifact {
    BackendArtifact {
        backend: "bootstrap-ir-v0".to_string(),
        module_kind: module_kind.to_string(),
        ssa: ssa.clone(),
    }
}
