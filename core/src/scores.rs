use crate::compatibility::CompatibilityEntry;
use crate::graph::CycleDetectionResult;
use crate::references::ReverseIndex;
use crate::symbols::{ApiContract, SymbolIndex};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Per-module migration readiness score.
/// Higher score = recommended to migrate first.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleReadiness {
    pub module: String,
    pub score: f64,
    pub rank: usize,
    pub in_degree: usize,
    pub complexity: f64,
    pub external_compatibility: f64,
    pub cycle_count: usize,
    pub has_tests: bool,
    /// Derived label: "trivial", "moderate", "heavy", or "rewrite".
    /// Maps the composite score into a human-readable migration effort hint.
    pub migration_effort: String,
    pub breakdown: ScoreBreakdown,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScoreBreakdown {
    /// Normalized in_degree contribution (0–30)
    pub in_degree_score: f64,
    /// Normalized complexity contribution (0–25)
    pub complexity_score: f64,
    /// Normalized external compatibility contribution (0–20)
    pub external_compatibility_score: f64,
    /// Normalized cycle contribution (0–15)
    pub cycle_score: f64,
    /// Normalized test coverage contribution (0–10)
    pub test_coverage_score: f64,
}

/// Compute migration readiness scores for all analyzed files.
///
/// `module_deps` maps each module (relative file path) to the external package
/// names it imports, as produced by [`crate::deps::module_map::module_external_deps`].
/// When `None` or empty, module-level compatibility falls back to the project-wide
/// average for all modules.
pub fn calculate(
    root: &Path,
    files: &[std::path::PathBuf],
    symbol_results: &[(SymbolIndex, ApiContract)],
    reverse: &ReverseIndex,
    compatibility_matrix: &HashMap<String, CompatibilityEntry>,
    cycle_detection: &CycleDetectionResult,
    module_deps: Option<&HashMap<String, Vec<String>>>,
) -> anyhow::Result<Vec<ModuleReadiness>> {
    // Build module → data lookup maps
    let mut module_complexity: HashMap<String, f64> = HashMap::new();
    for (index, _) in symbol_results {
        let symbol_count = index.symbols.len() as f64;
        let approx_loc = index
            .symbols
            .last()
            .map(|s| s.line_range[1] as f64)
            .unwrap_or(1.0);
        let complexity = symbol_count * approx_loc.max(1.0).ln();
        module_complexity.insert(index.module.clone(), complexity);
    }

    // Compute in-degree per module
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    for (target_symbol, refs) in reverse {
        if let Some(module) = target_symbol.rsplit_once(':').map(|x| x.0) {
            let mut ref_files: HashSet<&str> = HashSet::new();
            for r in refs {
                ref_files.insert(r.location.file.as_str());
            }
            *in_degree.entry(module.to_string()).or_default() += ref_files.len();
        }
    }

    // Compute cycle participation per module
    let mut cycle_count: HashMap<String, usize> = HashMap::new();
    for cycle in &cycle_detection.cycles {
        for node in &cycle.nodes {
            *cycle_count.entry(node.clone()).or_default() += 1;
        }
    }

    // Compute test coverage
    let test_files: HashSet<String> = files
        .iter()
        .filter_map(|f| {
            let name = f.file_stem()?.to_string_lossy();
            if name == "mod" || name == "index" {
                let parent = f.parent()?;
                let test_candidate = parent.join("mod.test.rs").exists()
                    || parent.join("mod.spec.ts").exists()
                    || parent.join("index.test.ts").exists()
                    || parent.join("index.spec.ts").exists();
                if test_candidate {
                    return f
                        .strip_prefix(root)
                        .ok()
                        .map(|r| r.to_string_lossy().replace('\\', "/"));
                }
                return None;
            }
            let dir = f.parent()?;
            let test_rs = dir.join(format!("{}.test.rs", name));
            let test_ts = dir.join(format!("{}.test.ts", name));
            let spec_ts = dir.join(format!("{}.spec.ts", name));
            if test_rs.exists() || test_ts.exists() || spec_ts.exists() {
                f.strip_prefix(root)
                    .ok()
                    .map(|r| r.to_string_lossy().replace('\\', "/"))
            } else {
                None
            }
        })
        .collect();

    let mut all_modules: Vec<String> = Vec::new();
    for file in files {
        let module = file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        if !all_modules.contains(&module) {
            all_modules.push(module);
        }
    }

    // Pre-compute project-wide average compatibility for modules with no imports
    let project_avg = if compatibility_matrix.is_empty() {
        0.5
    } else {
        let total: f64 = compatibility_matrix
            .values()
            .map(|entry| entry.compatibility.numeric_score())
            .sum();
        total / compatibility_matrix.len() as f64
    };

    struct RawMetrics {
        in_degree: usize,
        complexity: f64,
        compatibility: f64,
        cycle_count: usize,
        has_tests: bool,
    }

    let mut raw_map: HashMap<String, RawMetrics> = HashMap::new();
    for module in &all_modules {
        let deg = *in_degree.get(module).unwrap_or(&0);
        let comp = module_complexity.get(module).copied().unwrap_or(1.0);
        let compat = module_compatibility(module, compatibility_matrix, module_deps, project_avg);
        let cycles = *cycle_count.get(module).unwrap_or(&0);
        let tests = test_files.contains(module);
        raw_map.insert(
            module.clone(),
            RawMetrics {
                in_degree: deg,
                complexity: comp,
                compatibility: compat,
                cycle_count: cycles,
                has_tests: tests,
            },
        );
    }

    let max_in_degree = raw_map
        .values()
        .map(|m| m.in_degree)
        .max()
        .unwrap_or(1)
        .max(1);
    let max_complexity = raw_map
        .values()
        .map(|m| m.complexity)
        .fold(0.0, f64::max)
        .max(1.0);
    let max_cycles = raw_map
        .values()
        .map(|m| m.cycle_count)
        .max()
        .unwrap_or(1)
        .max(1);

    let mut readiness_scores: Vec<ModuleReadiness> = all_modules
        .iter()
        .map(|module| {
            let raw = &raw_map[module];

            let norm_in_degree = raw.in_degree as f64 / max_in_degree as f64;
            let norm_complexity = 1.0 - (raw.complexity / max_complexity);
            let norm_compatibility = raw.compatibility;
            let norm_cycles = 1.0 - (raw.cycle_count as f64 / max_cycles as f64);
            let norm_tests = if raw.has_tests { 1.0 } else { 0.0 };

            let in_degree_score = 30.0 * norm_in_degree;
            let complexity_score = 25.0 * norm_complexity.max(0.0);
            let external_compatibility_score = 20.0 * norm_compatibility;
            let cycle_score = 15.0 * norm_cycles.max(0.0);
            let test_coverage_score = 10.0 * norm_tests;

            let score = in_degree_score
                + complexity_score
                + external_compatibility_score
                + cycle_score
                + test_coverage_score;

            let effort = effort_label(score);

            ModuleReadiness {
                module: module.clone(),
                score: (score * 100.0).round() / 100.0,
                rank: 0,
                in_degree: raw.in_degree,
                complexity: (raw.complexity * 100.0).round() / 100.0,
                external_compatibility: (raw.compatibility * 100.0).round() / 100.0,
                cycle_count: raw.cycle_count,
                has_tests: raw.has_tests,
                migration_effort: effort,
                breakdown: ScoreBreakdown {
                    in_degree_score: (in_degree_score * 100.0).round() / 100.0,
                    complexity_score: (complexity_score * 100.0).round() / 100.0,
                    external_compatibility_score: (external_compatibility_score * 100.0).round()
                        / 100.0,
                    cycle_score: (cycle_score * 100.0).round() / 100.0,
                    test_coverage_score: (test_coverage_score * 100.0).round() / 100.0,
                },
            }
        })
        .collect();

    readiness_scores.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for (i, entry) in readiness_scores.iter_mut().enumerate() {
        entry.rank = i + 1;
    }

    Ok(readiness_scores)
}

/// Map a composite score (0–100) to a migration effort label.
fn effort_label(score: f64) -> String {
    if score >= 70.0 {
        "trivial"
    } else if score >= 50.0 {
        "moderate"
    } else if score >= 30.0 {
        "heavy"
    } else {
        "rewrite"
    }
    .to_string()
}

/// Compute a per-module external compatibility score in [0, 1].
///
/// Unlike the old project-wide average, this function looks up the **actual**
/// external packages imported by the module and averages their compatibility
/// scores. Modules that import no external packages fall back to the project-wide
/// average.
fn module_compatibility(
    module: &str,
    compatibility_matrix: &HashMap<String, CompatibilityEntry>,
    module_deps: Option<&HashMap<String, Vec<String>>>,
    project_avg: f64,
) -> f64 {
    let imports = module_deps
        .and_then(|md| md.get(module))
        .map(|v| v.as_slice())
        .unwrap_or(&[]);

    if imports.is_empty() {
        // No external deps: neutral or project-level signal
        let lower = module.to_lowercase();
        if lower.contains("test") || lower.contains("spec") {
            return (project_avg + 0.15).min(1.0);
        }
        return project_avg;
    }

    let mut total = 0.0;
    let mut count = 0usize;
    for pkg in imports {
        if let Some(entry) = compatibility_matrix.get(pkg) {
            total += entry.compatibility.numeric_score();
            count += 1;
        }
    }

    if count == 0 {
        project_avg
    } else {
        (total / count as f64).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_range() {
        let scores = calculate(
            Path::new("/project"),
            &[],
            &[],
            &HashMap::new(),
            &HashMap::new(),
            &CycleDetectionResult {
                has_cycles: false,
                cycles: vec![],
                self_loops: vec![],
            },
            None,
        )
        .unwrap();
        assert!(scores.is_empty());
    }

    #[test]
    fn test_effort_labels() {
        assert_eq!(effort_label(85.0), "trivial");
        assert_eq!(effort_label(60.0), "moderate");
        assert_eq!(effort_label(40.0), "heavy");
        assert_eq!(effort_label(20.0), "rewrite");
    }

    #[test]
    fn test_module_compatibility_with_imports() {
        use crate::compatibility::CompatibilityLevel;
        let mut cm: HashMap<String, CompatibilityEntry> = HashMap::new();
        cm.insert(
            "axum".to_string(),
            CompatibilityEntry {
                source_language: "typescript".to_string(),
                target_language: "rust".to_string(),
                equivalent: None,
                compatibility: CompatibilityLevel::Full,
                effort: crate::compatibility::MigrationEffort::Trivial,
                guidance: None,
                note: None,
                tags: None,
                risk_tags: vec![],
            },
        );
        cm.insert(
            "lodash".to_string(),
            CompatibilityEntry {
                source_language: "typescript".to_string(),
                target_language: "rust".to_string(),
                equivalent: None,
                compatibility: CompatibilityLevel::None,
                effort: crate::compatibility::MigrationEffort::Rewrite,
                guidance: None,
                note: None,
                tags: None,
                risk_tags: vec![],
            },
        );

        let mut md: HashMap<String, Vec<String>> = HashMap::new();
        md.insert(
            "src/server.ts".to_string(),
            vec!["axum".to_string(), "lodash".to_string()],
        );

        let score = module_compatibility("src/server.ts", &cm, Some(&md), 0.5);
        // axum=1.0 + lodash=0.0 → avg=0.5
        assert!((score - 0.5).abs() < 1e-6);

        // Module with no imports uses project_avg
        let score2 = module_compatibility("src/util.ts", &cm, Some(&md), 0.5);
        assert!((score2 - 0.5).abs() < 1e-6);
    }
}
