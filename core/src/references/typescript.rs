use super::{ForwardIndex, ImportBinding, Location, ReferenceKind, ReverseIndex, SymbolReference};
use rayon::prelude::*;
use oxc_allocator::Allocator;
use oxc_ast::ast::{self, Function, Statement};
use oxc_ast::AstKind;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::{GetSpan, SourceType};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Parse import binding information from a TypeScript file.
pub fn parse_import_bindings(
    source: &str,
    file_path: Option<&Path>,
) -> anyhow::Result<Vec<ImportBinding>> {
    let source_type = detect_source_type(file_path);
    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, source, source_type).parse();
    let mut bindings = Vec::new();

    for stmt in &ret.program.body {
        match stmt {
            Statement::ImportDeclaration(import) => {
                let source_module = import.source.value.to_string();
                if !source_module.starts_with('.') && !source_module.starts_with('/') {
                    continue;
                }
                if let Some(specifiers) = &import.specifiers {
                    for spec in specifiers {
                        match spec {
                            ast::ImportDeclarationSpecifier::ImportSpecifier(s) => {
                                let imported = imported_name(&s.imported);
                                let local = s.local.name.to_string();
                                bindings.push(ImportBinding {
                                    local_name: local,
                                    source_module: source_module.clone(),
                                    exported_name: imported,
                                });
                            }
                            ast::ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
                                let local = s.local.name.to_string();
                                bindings.push(ImportBinding {
                                    local_name: local,
                                    source_module: source_module.clone(),
                                    exported_name: "default".to_string(),
                                });
                            }
                            ast::ImportDeclarationSpecifier::ImportNamespaceSpecifier(_) => {}
                        }
                    }
                }
            }
            Statement::ExportNamedDeclaration(export) => {
                if let Some(source) = &export.source {
                    let source_module = source.value.to_string();
                    if !source_module.starts_with('.') && !source_module.starts_with('/') {
                        continue;
                    }
                    for spec in &export.specifiers {
                            let local = imported_name(&spec.local);
                            let exported_name = imported_name(&spec.exported);
                            bindings.push(ImportBinding {
                                local_name: exported_name,
                                source_module: source_module.clone(),
                                exported_name: local,
                            });
                        }
                }
            }
            _ => {}
        }
    }

    Ok(bindings)
}

/// Get the name from a ModuleExportName.
fn imported_name(name: &ast::ModuleExportName) -> String {
    match name {
        ast::ModuleExportName::IdentifierName(id) => id.name.to_string(),
        ast::ModuleExportName::IdentifierReference(id) => id.name.to_string(),
        ast::ModuleExportName::StringLiteral(s) => s.value.to_string(),
    }
}

/// Resolve a relative import path to an actual file path.
fn resolve_import(imp: &str, current_file: &Path, project_root: &Path) -> Option<PathBuf> {
    let dir = current_file.parent()?;
    let resolved = dir.join(imp);
    let normalized = normalize_path(&resolved);

    if normalized.extension().and_then(|e| e.to_str()).is_some() {
        if normalized.exists() {
            return Some(project_root.join(trim_to_relative(&normalized, project_root)));
        }
    } else {
        for ext in &["ts", "tsx"] {
            let with_ext = normalized.with_extension(ext);
            if with_ext.exists() {
                return Some(project_root.join(trim_to_relative(&with_ext, project_root)));
            }
        }
        for ext in &["ts", "tsx"] {
            let index = normalized.join(format!("index.{}", ext));
            if index.exists() {
                return Some(project_root.join(trim_to_relative(&index, project_root)));
            }
        }
    }

    None
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                components.pop();
            }
            other => components.push(other.as_os_str().to_os_string()),
        }
    }
    let mut result = PathBuf::new();
    for c in components {
        result.push(c);
    }
    result
}

fn trim_to_relative(abs_path: &Path, project_root: &Path) -> PathBuf {
    abs_path
        .strip_prefix(project_root)
        .unwrap_or(abs_path)
        .to_path_buf()
}

/// Build import map: file_path -> (local_name -> (target_file, exported_name))
fn build_import_map(
    root: &Path,
    files: &[PathBuf],
) -> anyhow::Result<HashMap<String, HashMap<String, (String, String)>>> {
    let entries: Vec<(String, anyhow::Result<HashMap<String, (String, String)>>)> = files
        .par_iter()
        .map(|file| {
            let source = match fs::read_to_string(file) {
                Ok(s) => s,
                Err(e) => return (String::new(), Err(e.into())),
            };
            let relative = file.strip_prefix(root).unwrap_or(file);
            let module = relative.to_string_lossy().replace('\\', "/");

            let bindings = match parse_import_bindings(&source, Some(file)) {
                Ok(b) => b,
                Err(e) => return (module, Err(e)),
            };
            let mut file_bindings: HashMap<String, (String, String)> = HashMap::new();

            for binding in bindings {
                if let Some(target_path) = resolve_import(&binding.source_module, file, root) {
                    let target_relative = target_path
                        .strip_prefix(root)
                        .unwrap_or(&target_path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    file_bindings.insert(
                        binding.local_name,
                        (target_relative, binding.exported_name),
                    );
                }
            }

            (module, Ok(file_bindings))
        })
        .collect();

    let mut import_map: HashMap<String, HashMap<String, (String, String)>> = HashMap::new();
    for (module, result) in entries {
        import_map.insert(module, result?);
    }

    Ok(import_map)
}

/// Classify a reference by walking up the AST ancestors.
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
                        // Check if reference is inside the implements clause
                        let ref_span = ancestor.kind().span();
                        if impl_span.start <= ref_span.start
                            && ref_span.end <= impl_span.end
                        {
                            return ReferenceKind::Implements;
                        }
                    }
                }
            }
            AstKind::TSTypeReference(_) => {
                result = ReferenceKind::TypeReference;
            }
            AstKind::TSInterfaceDeclaration(iface) => {
                for ext in &iface.extends {
                    let ext_span = ext.span;
                    let ref_span = ancestor.kind().span();
                    if ext_span.start <= ref_span.start && ref_span.end <= ext_span.end {
                        return ReferenceKind::ExtendsType;
                    }
                }
            }
            // Member expressions - Static and Computed
            AstKind::StaticMemberExpression(_)
            | AstKind::ComputedMemberExpression(_)
            | AstKind::PrivateFieldExpression(_) => {
                result = ReferenceKind::PropertyAccess;
            }
            AstKind::ExportNamedDeclaration(_)
            | AstKind::ExportDefaultDeclaration(_)
            | AstKind::VariableDeclaration(_)
            | AstKind::Function(Function {
                id: Some(_), ..
            })
            | AstKind::ArrowFunctionExpression(_) => {
                return result;
            }
            AstKind::Program(_) => return result,
            _ => {}
        }
    }

    result
}

/// Extract cross-file references from TypeScript files.
pub fn extract_all(
    root: &Path,
    files: &[PathBuf],
) -> anyhow::Result<(ForwardIndex, ReverseIndex)> {
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
    import_map: &HashMap<String, HashMap<String, (String, String)>>,
) -> Option<(ForwardIndex, ReverseIndex)> {
    let source = fs::read_to_string(file).ok()?;

    let relative = file.strip_prefix(root).unwrap_or(file);
    let module = relative.to_string_lossy().replace('\\', "/");

    let file_bindings = import_map.get(&module)?;
    if file_bindings.is_empty() {
        return None;
    }

    let source_type = detect_source_type(Some(file));
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

            let ref_span = ast_nodes.get_node(ref_node_id).kind().span();

            let line = source[..ref_span.start as usize].matches('\n').count() + 1;
            let column = ref_span.start as usize
                - source[..ref_span.start as usize]
                    .rfind('\n')
                    .map_or(0, |p| p + 1);

            let kind = classify_reference_kind(ref_node_id, &ast_nodes);

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

fn detect_source_type(file_path: Option<&Path>) -> SourceType {
    match file_path.and_then(|p| p.extension().and_then(|e| e.to_str())) {
        Some("tsx") => SourceType::tsx(),
        Some("ts") => SourceType::ts(),
        Some("mts") | Some("cts") => {
            SourceType::default().with_typescript(true).with_module(true)
        }
        _ => SourceType::ts(),
    }
}
