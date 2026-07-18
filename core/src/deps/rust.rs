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
