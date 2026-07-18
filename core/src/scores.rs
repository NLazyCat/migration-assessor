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
    pub breakdown: ScoreBreakdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub fn calculate(
    root: &Path,
    files: &[std::path::PathBuf],
    symbol_results: &[(SymbolIndex, ApiContract)],
    reverse: &ReverseIndex,
    compatibility_matrix: &HashMap<String, CompatibilityEntry>,
    cycle_detection: &CycleDetectionResult,
) -> anyhow::Result<Vec<ModuleReadiness>> {
    // Build module → data lookup maps
    let mut module_complexity: HashMap<String, f64> = HashMap::new();
    for (index, _) in symbol_results {
        let symbol_count = index.symbols.len() as f64;
        // Estimate LOC from the last symbol's end line
        let approx_loc = index
            .symbols
            .last()
            .map(|s| s.line_range[1] as f64)
            .unwrap_or(1.0);
        // Complexity = symbol_count * log(LOC)
        let complexity = symbol_count * approx_loc.max(1.0).ln();
        module_complexity.insert(index.module.clone(), complexity);
    }

    // Compute in-degree per module: how many unique SOURCE files reference this module's symbols
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    for (target_symbol, refs) in reverse {
        // Extract module from target symbol like "src/utils.ts:formatDate"
        if let Some(module) = target_symbol.rsplitn(2, ':').nth(1) {
            // Count unique referencing files
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

    // Compute test coverage: does a test file exist?
    let test_files: HashSet<String> = files
        .iter()
        .filter_map(|f| {
            let name = f.file_stem()?.to_string_lossy();
            if name == "mod" || name == "index" {
                // For mod.rs / index.ts, check parent dir for test sibling
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
            // Check for *.test.* / *.spec.* files
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

    // Collect all modules from files + symbol results
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

    // Gather raw metrics
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
        let compat = compute_module_compatibility(module, compatibility_matrix);
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

    // Normalize each dimension to [0.0, 1.0]
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

    // Build sorted results
    let mut readiness_scores: Vec<ModuleReadiness> = all_modules
        .iter()
        .map(|module| {
            let raw = &raw_map[module];

            // Normalize: higher in_degree = more foundational = higher score
            let norm_in_degree = raw.in_degree as f64 / max_in_degree as f64;
            // Normalize: lower complexity = higher score
            let norm_complexity = 1.0 - (raw.complexity / max_complexity);
            // Normalize: already in [0, 1]
            let norm_compatibility = raw.compatibility;
            // Normalize: fewer cycles = higher score
            let norm_cycles = 1.0 - (raw.cycle_count as f64 / max_cycles as f64);
            // Normalize: test presence = 1.0 or 0.0
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

            ModuleReadiness {
                module: module.clone(),
                score: (score * 100.0).round() / 100.0,
                rank: 0, // Will set after sorting
                in_degree: raw.in_degree,
                complexity: (raw.complexity * 100.0).round() / 100.0,
                external_compatibility: (raw.compatibility * 100.0).round() / 100.0,
                cycle_count: raw.cycle_count,
                has_tests: raw.has_tests,
                breakdown: ScoreBreakdown {
                    in_degree_score: (in_degree_score * 100.0).round() / 100.0,
                    complexity_score: (complexity_score * 100.0).round() / 100.0,
                    external_compatibility_score: (external_compatibility_score * 100.0).round() / 100.0,
                    cycle_score: (cycle_score * 100.0).round() / 100.0,
                    test_coverage_score: (test_coverage_score * 100.0).round() / 100.0,
                },
            }
        })
        .collect();

    // Sort by score descending (highest = migrate first)
    readiness_scores.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // Assign ranks
    for (i, entry) in readiness_scores.iter_mut().enumerate() {
        entry.rank = i + 1;
    }

    Ok(readiness_scores)
}

/// Compute a module's external compatibility score in [0, 1].
/// Looks at the module's imports, maps them against the compatibility matrix.
fn compute_module_compatibility(
    _module: &str,
    _compatibility_matrix: &HashMap<String, CompatibilityEntry>,
) -> f64 {
    // For v1, we use a simplified heuristic:
    // - If the module file references known external packages, check compatibility.
    // - Default to 1.0 (neutral) since we lack per-file import-to-dependency mapping at this stage.
    // Future: parse the module's imports, look up each dependency in the matrix, and average scores.
    1.0
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
        )
        .unwrap();
        assert!(scores.is_empty());
    }
}
