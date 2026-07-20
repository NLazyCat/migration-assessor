use super::{
    FileBindings, ForwardIndex, Location, ReferenceKind, ReverseIndex,
    SymbolReference,
};
use crate::util;
use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_ast::ast::Function;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::GetSpan;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Build an import map across all JS files: for each file, list all bindings.
pub fn build_import_map(
    root: &Path,
    files: &[PathBuf],
) -> anyhow::Result<HashMap<String, FileBindings>> {
    let entries: Vec<(String, anyhow::Result<FileBindings>)> = files
        .par_iter()
        .map(|file| {
            let source = match fs::read_to_string(file) {
                Ok(s) => s,
                Err(e) => return (String::new(), Err(e.into())),
            };
            let relative = file.strip_prefix(root).unwrap_or(file);
            let module = path_to_forward_slash(relative);

            let bindings = match crate::parser::javascript::parse_import_bindings(&source, Some(file))
            {
                Ok(b) => b,
                Err(e) => return (module, Err(e)),
            };
            let mut file_bindings: FileBindings = HashMap::new();

            for binding in bindings {
                if let Some(target_relative) =
                    resolve_js_import(&binding.source_module, file, root)
                {
                    let target_str = path_to_forward_slash(&target_relative);
                    file_bindings.insert(binding.local_name, (target_str, binding.exported_name));
                }
            }

            (module, Ok(file_bindings))
        })
        .collect();

    let mut import_map: HashMap<String, FileBindings> = HashMap::new();
    for (module, result) in entries {
        import_map.insert(module, result?);
    }

    Ok(import_map)
}

/// Resolve a relative import path to an actual file path relative to project root.
fn resolve_js_import(
    imp: &str,
    current_file: &Path,
    project_root: &Path,
) -> Option<PathBuf> {
    let dir = current_file.parent()?;
    let resolved = dir.join(imp);
    let normalized = util::normalize_path(&resolved);

    if let Some(path) = probe_path(&normalized, project_root) {
        return Some(to_relative(&path, project_root));
    }

    None
}

fn probe_path(path: &Path, project_root: &Path) -> Option<PathBuf> {
    if path.extension().and_then(|e| e.to_str()).is_some() {
        if path.exists() {
            return Some(project_root.join(trim_to_relative(path, project_root)));
        }
    } else {
        for ext in &["js", "jsx", "mjs", "cjs"] {
            let with_ext = path.with_extension(ext);
            if with_ext.exists() {
                return Some(project_root.join(trim_to_relative(&with_ext, project_root)));
            }
        }
        for ext in &["js", "jsx", "mjs", "cjs"] {
            let index = path.join(format!("index.{}", ext));
            if index.exists() {
                return Some(project_root.join(trim_to_relative(&index, project_root)));
            }
        }
    }
    None
}

fn to_relative(abs: &Path, project_root: &Path) -> PathBuf {
    let stripped = abs.strip_prefix(project_root).unwrap_or(abs);
    PathBuf::from(path_to_forward_slash(stripped))
}

fn path_to_forward_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn trim_to_relative(abs_path: &Path, project_root: &Path) -> PathBuf {
    abs_path
        .strip_prefix(project_root)
        .unwrap_or(abs_path)
        .to_path_buf()
}

/// Extract cross-file references from JavaScript files.
pub fn extract_all(root: &Path, files: &[PathBuf]) -> anyhow::Result<(ForwardIndex, ReverseIndex)> {
    let import_map = build_import_map(root, files)?;

    let per_file: Vec<(ForwardIndex, ReverseIndex)> = files
        .par_iter()
        .filter_map(|file| extract_file_refs(file, root, &import_map))
        .collect();

    let mut forward: ForwardIndex = HashMap::new();
    let mut reverse: ReverseIndex = HashMap::new();
    for (fwd, rev) in per_file {
        for (k, v) in fwd {
            forward.entry(k).or_default().extend(v);
        }
        for (k, v) in rev {
            reverse.entry(k).or_default().extend(v);
        }
    }

    Ok((forward, reverse))
}

fn extract_file_refs(
    file: &Path,
    root: &Path,
    import_map: &HashMap<String, FileBindings>,
) -> Option<(ForwardIndex, ReverseIndex)> {
    let source = fs::read_to_string(file).ok()?;

    let relative = file.strip_prefix(root).unwrap_or(file);
    let module = relative.to_string_lossy().replace('\\', "/");

    let file_bindings = import_map.get(&module)?;
    if file_bindings.is_empty() {
        return None;
    }

    let source_type = util::detect_source_type(Some(file));
    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, &source, source_type).parse();
    let program = ret.program;

    let semantic_ret = SemanticBuilder::new()
        .with_build_nodes(true)
        .build(&program);
    let scoping = semantic_ret.semantic.scoping();
    let ast_nodes = semantic_ret.semantic.nodes();

    let mut import_symbols: HashMap<String, oxc_semantic::SymbolId> = HashMap::new();
    for sym_id in scoping.symbol_ids() {
        let name = scoping.symbol_name(sym_id);
        if file_bindings.contains_key(name) {
            import_symbols.insert(name.to_string(), sym_id);
        }
    }

    let mut forward: ForwardIndex = HashMap::new();
    let mut reverse: ReverseIndex = HashMap::new();

    for (local_name, (target_file, exported_name)) in file_bindings {
        let sym_id = *import_symbols.get(local_name)?;

        let target_symbol_id = format!("{}:{}", target_file, exported_name);
        let ref_ids = scoping.get_resolved_reference_ids(sym_id);

        for ref_id in ref_ids {
            let reference = scoping.get_reference(*ref_id);
            let ref_node_id = reference.node_id();

            let ref_node = ast_nodes.get_node(ref_node_id);
            let ref_span = ref_node.kind().span();

            let line = source[..ref_span.start as usize].matches('\n').count() + 1;
            let column = ref_span.start as usize
                - source[..ref_span.start as usize]
                    .rfind('\n')
                    .map_or(0, |p| p + 1);

            let kind = classify_reference_kind(ref_node_id, ast_nodes);

            let reference_entry = SymbolReference {
                symbol: target_symbol_id.clone(),
                location: Location {
                    file: module.clone(),
                    line,
                    column,
                },
                kind: kind.clone(),
            };

            let source_symbol_id = format!("{}:{}", module, local_name);

            forward
                .entry(source_symbol_id.clone())
                .or_default()
                .push(reference_entry);

            reverse
                .entry(target_symbol_id.clone())
                .or_default()
                .push(SymbolReference {
                    symbol: source_symbol_id,
                    location: Location {
                        file: module.clone(),
                        line,
                        column,
                    },
                    kind,
                });
        }
    }

    Some((forward, reverse))
}

fn classify_reference_kind(
    node_id: oxc_semantic::NodeId,
    nodes: &oxc_semantic::AstNodes,
) -> ReferenceKind {
    let mut result = ReferenceKind::Usage;

    for ancestor in nodes.ancestors(node_id) {
        match ancestor.kind() {
            AstKind::CallExpression(_) => return ReferenceKind::Call,
            AstKind::NewExpression(_) => return ReferenceKind::Instantiation,
            AstKind::Class(class) => {
                if class.super_class.is_some() {
                    result = ReferenceKind::Extends;
                }
                if !class.implements.is_empty() {
                    for impl_clause in &class.implements {
                        let impl_span = impl_clause.expression.span();
                        let ref_span = ancestor.kind().span();
                        if impl_span.start <= ref_span.start && ref_span.end <= impl_span.end {
                            return ReferenceKind::Implements;
                        }
                    }
                }
            }
            AstKind::StaticMemberExpression(_)
            | AstKind::ComputedMemberExpression(_)
            | AstKind::PrivateFieldExpression(_) => {
                result = ReferenceKind::PropertyAccess;
            }
            AstKind::ExportNamedDeclaration(_)
            | AstKind::ExportDefaultDeclaration(_)
            | AstKind::VariableDeclaration(_)
            | AstKind::Function(Function { id: Some(_), .. })
            | AstKind::ArrowFunctionExpression(_) => {
                return result;
            }
            _ => {}
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_all_empty_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let (fwd, rev) = extract_all(dir.path(), &[]).unwrap();
        assert!(fwd.is_empty());
        assert!(rev.is_empty());
    }
}
