use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RuntimeProfile {
    pub gc_strategy: &'static str,
    pub scheduler: &'static str,
    pub ffi_boundary: &'static str,
    pub allocation_model: &'static str,
}

impl Default for RuntimeProfile {
    fn default() -> Self {
        Self {
            gc_strategy: "precise-non-moving-concurrent-mark-sweep",
            scheduler: "structured-task-groups",
            ffi_boundary: "stable-c-abi",
            allocation_model: "stack-first-with-escape-analysis",
        }
    }
}

pub fn profile() -> RuntimeProfile {
    RuntimeProfile::default()
}

#[cfg(test)]
mod tests {
    use super::profile;

    #[test]
    fn runtime_profile_matches_governance_contract() {
        let profile = profile();
        assert_eq!(
            profile.gc_strategy,
            "precise-non-moving-concurrent-mark-sweep"
        );
        assert_eq!(profile.scheduler, "structured-task-groups");
    }
}
