use crate::compatibility::{CompatibilityEntry, CompatibilityMatrix, MigrationEffort};
use crate::deps::{module_map, ResolvedDependency};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// A migration recommendation for a single external dependency.
///
/// Surfaced to AI porting agents so they know, per dependency:
/// - how compatible it is with the target language,
/// - how much effort porting it represents,
/// - the suggested equivalent crate/package,
/// - concrete guidance, and
/// - which modules import it (so the work can be scoped).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyRecommendation {
    pub package: String,
    #[serde(default = "default_version")]
    pub version: String,
    pub compatibility: crate::compatibility::CompatibilityLevel,
    pub effort: MigrationEffort,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equivalent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub risk_tags: Vec<String>,
    /// Modules (relative file paths) that import this dependency.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub affected_modules: Vec<String>,
    pub affected_module_count: usize,
    /// True when the dependency is referenced by many modules and has low
    /// compatibility — i.e. high-leverage, high-risk to port.
    pub is_high_impact: bool,
}

fn default_version() -> String {
    String::new()
}

/// Full set of recommendations for a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendationReport {
    pub source_language: String,
    pub target_language: String,
    /// Count of dependencies with no compatibility mapping.
    pub unmapped_count: usize,
    /// Count of dependencies rated `rewrite` or `heavy` effort.
    pub heavy_count: usize,
    pub dependencies: Vec<DependencyRecommendation>,
}

/// Build recommendations for every resolved dependency in the project.
///
/// `module_deps` should come from [`module_map::module_external_deps`]; it maps
/// each analyzed module to the external packages it imports. When `None`, the
/// affected-module lists are left empty.
pub fn build_recommendations(
    dependencies: &[ResolvedDependency],
    matrix: &CompatibilityMatrix,
    module_deps: Option<&HashMap<String, Vec<String>>>,
) -> RecommendationReport {
    let compatibility_map = matrix.evaluate(dependencies);

    // Reverse module_deps -> package -> modules.
    let mut package_modules: HashMap<String, Vec<String>> = HashMap::new();
    if let Some(md) = module_deps {
        for (module, pkgs) in md {
            let mut seen: HashSet<String> = HashSet::new();
            for pkg in pkgs {
                if seen.insert(pkg.clone()) {
                    package_modules.entry(pkg.clone()).or_default().push(module.clone());
                }
            }
        }
    }

    let mut dependencies_out: Vec<DependencyRecommendation> = Vec::new();
    let mut unmapped = 0usize;
    let mut heavy = 0usize;

    for dep in dependencies {
        let entry: CompatibilityEntry = compatibility_map
            .get(&dep.name)
            .cloned()
            .unwrap_or_else(|| CompatibilityEntry {
                source_language: matrix.source_language().to_string(),
                target_language: matrix.target_language().to_string(),
                equivalent: None,
                compatibility: crate::compatibility::CompatibilityLevel::Unknown,
                effort: MigrationEffort::Unknown,
                guidance: None,
                note: Some("No compatibility mapping available.".to_string()),
                tags: None,
                risk_tags: vec!["unmapped".to_string()],
            });

        let affected = package_modules.get(&dep.name).cloned().unwrap_or_default();
        let affected_module_count = affected.len();
        let is_high_impact = affected_module_count > 5
            && matches!(
                entry.compatibility,
                crate::compatibility::CompatibilityLevel::Unknown
                    | crate::compatibility::CompatibilityLevel::None
            );

        if entry.risk_tags.iter().any(|t| t == "unmapped") {
            unmapped += 1;
        }
        if matches!(entry.effort, MigrationEffort::Rewrite | MigrationEffort::Heavy) {
            heavy += 1;
        }

        dependencies_out.push(DependencyRecommendation {
            package: dep.name.clone(),
            version: dep.version.clone(),
            compatibility: entry.compatibility,
            effort: entry.effort,
            equivalent: entry.equivalent,
            guidance: entry.guidance,
            note: entry.note,
            risk_tags: entry.risk_tags,
            affected_modules: affected,
            affected_module_count,
            is_high_impact,
        });
    }

    dependencies_out.sort_by(|a, b| {
        b.is_high_impact
            .cmp(&a.is_high_impact)
            .then(b.affected_module_count.cmp(&a.affected_module_count))
            .then(a.package.cmp(&b.package))
    });

    RecommendationReport {
        source_language: matrix.source_language().to_string(),
        target_language: matrix.target_language().to_string(),
        unmapped_count: unmapped,
        heavy_count: heavy,
        dependencies: dependencies_out,
    }
}

/// Convenience wrapper that resolves `module_deps` from disk.
pub fn build_recommendations_with_modules(
    root: &Path,
    files: &[std::path::PathBuf],
    source_language: crate::project::SourceLanguage,
    dependencies: &[ResolvedDependency],
    matrix: &CompatibilityMatrix,
) -> RecommendationReport {
    let module_deps = module_map::module_external_deps(root, files, source_language);
    build_recommendations(dependencies, matrix, Some(&module_deps))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compatibility::{CompatibilityLevel, CompatibilityMatrix};
    use crate::project::SourceLanguage;

    fn dep(name: &str) -> ResolvedDependency {
        ResolvedDependency {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            resolved: None,
            dependencies: vec![],
            children: vec![],
            dep_type: "prod".to_string(),
        }
    }

    #[test]
    fn builds_recommendations_for_express() {
        let matrix = CompatibilityMatrix::new("typescript".to_string(), "rust".to_string());
        let deps = vec![dep("express"), dep("unknown-pkg")];

        // module_deps uses the axis from module_map: module_path -> [package_names]
        let module_deps: HashMap<String, Vec<String>> = [
            ("src/server.ts".to_string(), vec!["express".to_string()]),
            ("src/x.ts".to_string(), vec!["unknown-pkg".to_string()]),
        ]
        .into_iter()
        .collect();

        let report = build_recommendations(&deps, &matrix, Some(&module_deps));
        let express = report.dependencies.iter().find(|d| d.package == "express").unwrap();
        assert_eq!(express.compatibility, CompatibilityLevel::Partial);
        assert_eq!(express.affected_module_count, 1);
        assert!(!express.is_high_impact);

        let unknown = report
            .dependencies
            .iter()
            .find(|d| d.package == "unknown-pkg")
            .unwrap();
        assert_eq!(unknown.compatibility, CompatibilityLevel::Unknown);
        assert_eq!(report.unmapped_count, 1);
    }
}
