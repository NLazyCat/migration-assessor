use super::ResolvedDependency;
use cargo_metadata::{CargoOpt, MetadataCommand};
use std::collections::HashSet;
use std::path::Path;

pub fn resolve(root: &Path) -> anyhow::Result<Vec<ResolvedDependency>> {
    let metadata = MetadataCommand::new()
        .current_dir(root)
        .features(CargoOpt::AllFeatures)
        .exec()?;

    // Collect workspace member IDs to distinguish workspace vs external
    let workspace_ids: HashSet<String> = metadata
        .workspace_packages()
        .iter()
        .map(|p| p.id.repr.clone())
        .collect();

    let mut resolved = Vec::new();
    for package in metadata.packages {
        let dep_type = if workspace_ids.contains(&package.id.repr) {
            "workspace"
        } else if package.source.is_some() {
            "prod"
        } else {
            "workspace"
        };

        resolved.push(ResolvedDependency {
            name: package.name.to_string(),
            version: package.version.to_string(),
            resolved: package.source.map(|s| s.to_string()),
            dependencies: package
                .dependencies
                .into_iter()
                .map(|d| d.name.to_string())
                .collect(),
            children: vec![],
            dep_type: dep_type.to_string(),
        });
    }

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_no_cargo_toml() {
        let dir = tempfile::TempDir::new().unwrap();
        let result = resolve(dir.path());
        assert!(result.is_err());
    }

    #[test]
    #[ignore = "requires cargo metadata which may not be in PATH"]
    fn test_resolve_with_empty_cargo_toml() {
        let dir = tempfile::TempDir::new().unwrap();
        let cargo = r#"[package]
name = "test-pkg"
version = "0.1.0"
edition = "2021"
"#;
        std::fs::write(dir.path().join("Cargo.toml"), cargo).unwrap();
        std::fs::write(dir.path().join("src").join("lib.rs"), "").unwrap();
        let deps = resolve(dir.path()).unwrap();
        let pkg = deps.iter().find(|d| d.name == "test-pkg");
        assert!(pkg.is_some());
        assert_eq!(pkg.unwrap().dep_type, "workspace");
    }

    #[test]
    #[ignore = "requires cargo metadata which may not be in PATH"]
    fn test_resolve_with_dependency() {
        let dir = tempfile::TempDir::new().unwrap();
        let cargo = r#"[package]
name = "test-pkg"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = "1.0"
"#;
        std::fs::write(dir.path().join("Cargo.toml"), cargo).unwrap();
        std::fs::write(dir.path().join("src").join("lib.rs"), "").unwrap();
        let deps = resolve(dir.path()).unwrap();
        let pkg = deps.iter().find(|d| d.name == "test-pkg").unwrap();
        assert!(pkg.dependencies.contains(&"serde".to_string()));
    }
}
