use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use migration_core::diff::FileDiffResult;
use migration_core::output_paths;
use migration_core::recommendation::{DependencyRecommendation, RecommendationReport};

use crate::commands::context::ProjectContext;

use super::types::{
    ChangeDetailInfo, DocChangeDetail, FileChangeGroup, ImportChangeDetail, PropagationLink,
    PropagationResult, ReverseIndex, SymbolChangeDetail,
};

pub(crate) fn convert_to_output_format(
    file_changes: &[FileDiffResult],
    file_recs: &HashMap<String, Vec<DependencyRecommendation>>,
) -> Vec<FileChangeGroup> {
    let mut result = Vec::new();
    for fc in file_changes {
        let mut changes = Vec::new();
        for sc in &fc.symbol_changes {
            let mut details = Vec::new();
            for d in &sc.details {
                details.push(ChangeDetailInfo {
                    aspect: d.aspect.clone(),
                    change_type: d.change_type.clone(),
                    description: d.description.clone(),
                    old_value: d.old_value.clone(),
                    new_value: d.new_value.clone(),
                    migration_note: d.migration_note.clone(),
                });
            }
            changes.push(SymbolChangeDetail {
                symbol: sc.symbol.clone(),
                kind: sc.kind.clone(),
                change_type: sc.change_type.clone(),
                severity: sc.severity.clone(),
                old_name: sc.old_name.clone(),
                rename_confidence: sc.rename_confidence,
                details,
                old_line_range: sc.old_line_range,
                new_line_range: sc.new_line_range,
                old_source: sc.old_source.clone(),
                new_source: sc.new_source.clone(),
                target_file: sc.target_file.clone(),
                target_symbol: sc.target_symbol.clone(),
                target_child: sc.target_child.clone(),
                target_line_range: sc.target_line_range,
            });
        }

        let mut import_changes = Vec::new();
        for ic in &fc.import_changes {
            import_changes.push(ImportChangeDetail {
                change_type: ic.change_type.clone(),
                package: ic.package.clone(),
                old_path: ic.old_path.clone(),
                new_path: ic.new_path.clone(),
                is_external: ic.is_external,
            });
        }

        let mut doc_changes = Vec::new();
        for dc in &fc.doc_changes {
            doc_changes.push(DocChangeDetail {
                change_type: dc.change_type.clone(),
                symbol: dc.symbol.clone(),
                is_deprecated: dc.is_deprecated,
                has_todo: dc.has_todo,
                has_safety_note: dc.has_safety_note,
            });
        }

        let mut recommendations = Vec::new();
        if let Some(recs) = file_recs.get(&fc.file) {
            recommendations = recs.clone();
        }

        result.push(FileChangeGroup {
            file: fc.file.clone(),
            source_attached: true,
            changes,
            import_changes,
            doc_changes,
            recommendations,
        });
    }
    result
}

pub(crate) fn load_reverse_index(ctx: &ProjectContext) -> ReverseIndex {
    ctx.load_reverse_index().unwrap_or_default()
}

pub(crate) fn propagate_changes(
    triggered_symbols: &[String],
    reverse_index: &ReverseIndex,
) -> PropagationResult {
    let mut visited: HashSet<String> = HashSet::new();
    let mut chain: Vec<PropagationLink> = Vec::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    let mut affected_files: HashSet<String> = HashSet::new();

    // Seed queue with triggered symbols
    for sym in triggered_symbols {
        visited.insert(sym.clone());
        queue.push_back(sym.clone());
    }

    while let Some(current) = queue.pop_front() {
        if let Some(refs) = reverse_index.get(&current) {
            for r in refs {
                let dependent_symbol = &r.symbol;
                if !visited.contains(dependent_symbol) {
                    visited.insert(dependent_symbol.clone());
                    queue.push_back(dependent_symbol.clone());

                    // Extract file from dependent_symbol (format: "file:symbol" or just "symbol")
                    if let Some((dep_file, _)) = dependent_symbol.rsplit_once(':') {
                        affected_files.insert(dep_file.to_string());
                    } else if let Some((_, _)) = current.rsplit_once(':') {
                        // The dependent_symbol may be just a symbol name; try extracting
                        // the file from the r.location.file field
                        affected_files.insert(r.location.file.clone());
                    }

                    chain.push(PropagationLink {
                        from: current.clone(),
                        to: dependent_symbol.clone(),
                        via: r.kind.clone(),
                    });
                }
            }
        }
    }

    // Add defining files as affected (they contain the symbols that broke)
    for sym in triggered_symbols {
        if let Some((file, _)) = sym.rsplit_once(':') {
            affected_files.insert(file.to_string());
        }
    }

    let mut sorted_files: Vec<String> = affected_files.into_iter().collect();
    sorted_files.sort();

    PropagationResult {
        triggered_by: triggered_symbols.to_vec(),
        affected_files: sorted_files,
        chain,
    }
}

pub(crate) fn compute_summary(file_changes: &[FileDiffResult]) -> migration_core::diff::DiffSummary {
    let mut summary = migration_core::diff::DiffSummary {
        total_files_changed: file_changes.len(),
        symbols_added: 0,
        symbols_removed: 0,
        symbols_renamed: 0,
        symbols_modified: 0,
        breaking_changes: 0,
        new_dependencies: 0,
        removed_dependencies: 0,
    };

    for fc in file_changes {
        for sc in &fc.symbol_changes {
            match sc.change_type.as_str() {
                "added" => summary.symbols_added += 1,
                "removed" => summary.symbols_removed += 1,
                "renamed" => summary.symbols_renamed += 1,
                "modified" => summary.symbols_modified += 1,
                _ => {}
            }
            if sc.severity == "breaking" {
                summary.breaking_changes += 1;
            }
        }
        summary.new_dependencies += fc.import_changes.iter().filter(|ic| ic.change_type == "added").count();
        summary.removed_dependencies += fc.import_changes.iter().filter(|ic| ic.change_type == "removed").count();
    }

    summary
}

pub(crate) fn load_file_recommendations(report_dir: &Path) -> HashMap<String, Vec<DependencyRecommendation>> {
    let rec_path = report_dir.join(output_paths::external::RECOMMENDATIONS);
    if !rec_path.exists() {
        return HashMap::new();
    }
    let content = match std::fs::read_to_string(&rec_path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let report: RecommendationReport = match serde_json::from_str(&content) {
        Ok(r) => r,
        Err(_) => return HashMap::new(),
    };

    let mut map: HashMap<String, Vec<DependencyRecommendation>> = HashMap::new();
    for dep in &report.dependencies {
        for module in &dep.affected_modules {
            map.entry(module.clone()).or_default().push(dep.clone());
        }
    }
    map
}
