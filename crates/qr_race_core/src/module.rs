//! Module manifests.
//!
//! Modules are admin-uploaded Python programs run through RustPython. A manifest
//! is the part the room config sees and the server validates before distributing
//! code to clients. The capability list is deliberately open: the sandbox
//! boundary is still an undecided product fact (see PRODUCT.md).

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// A module source larger than this is rejected before it reaches storage or a
/// client. Modules are small hooks, not applications.
pub const MAX_MODULE_SOURCE_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookPoint {
    /// Runs for every captured frame.
    Frame,
    /// Runs on a user action (a relay publish, a join, a tap).
    Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Camera,
    Network,
    Storage,
    Ui,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleManifest {
    pub id: String,
    pub version: String,
    pub entry: String,
    pub hooks: Vec<HookPoint>,
    pub capabilities: Vec<Capability>,
}

impl ModuleManifest {
    pub fn new(
        id: impl Into<String>,
        version: impl Into<String>,
        entry: impl Into<String>,
        hooks: Vec<HookPoint>,
        capabilities: Vec<Capability>,
    ) -> Result<Self, CoreError> {
        let manifest = Self {
            id: id.into(),
            version: version.into(),
            entry: entry.into(),
            hooks,
            capabilities,
        };
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn runs_per_frame(&self) -> bool {
        self.hooks.contains(&HookPoint::Frame)
    }

    fn validate(&self) -> Result<(), CoreError> {
        if self.id.trim().is_empty() {
            return Err(CoreError::Manifest("id is empty"));
        }
        if self.version.trim().is_empty() {
            return Err(CoreError::Manifest("version is empty"));
        }
        if self.entry.trim().is_empty() {
            return Err(CoreError::Manifest("entry is empty"));
        }
        if self.hooks.is_empty() {
            return Err(CoreError::Manifest("no hook points declared"));
        }
        let mut seen = self.hooks.clone();
        seen.sort();
        seen.dedup();
        if seen.len() != self.hooks.len() {
            return Err(CoreError::Manifest("duplicate hook points"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_valid_manifest() {
        let manifest = ModuleManifest::new(
            "stat-overlay",
            "0.1.0",
            "main.py",
            vec![HookPoint::Frame, HookPoint::Action],
            vec![Capability::Ui],
        )
        .unwrap();
        assert!(manifest.runs_per_frame());
    }

    #[test]
    fn rejects_empty_fields_and_no_hooks() {
        assert_eq!(
            ModuleManifest::new("", "0.1.0", "main.py", vec![HookPoint::Frame], vec![]),
            Err(CoreError::Manifest("id is empty"))
        );
        assert_eq!(
            ModuleManifest::new("m", "0.1.0", "main.py", vec![], vec![]),
            Err(CoreError::Manifest("no hook points declared"))
        );
    }

    #[test]
    fn rejects_duplicate_hooks() {
        assert_eq!(
            ModuleManifest::new(
                "m",
                "1",
                "main.py",
                vec![HookPoint::Frame, HookPoint::Frame],
                vec![]
            ),
            Err(CoreError::Manifest("duplicate hook points"))
        );
    }
}
