use super::{
    FileBindings, ForwardIndex, ImportBinding, Location, ReferenceKind, ReverseIndex,
    SymbolReference,
};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use syn::{self, Expr, ExprCall, ExprPath, ItemUse, Type, UseTree, spanned::Spanned, visit::Visit};

/// Parse import bindings from a Rust source file.
pub fn parse_import_bindings(source: &str) -> anyhow::Result<Vec<ImportBinding>> {
    let file = syn::parse_file(source)?;
    let mut bindings = Vec::new();

    for item in &file.items {
        if let syn::Item::Use(use_item) = item {
            extract_use_bindings(use_item, &mut bindings);
        }
    }

    Ok(bindings)
}

fn extract_use_bindings(use_item: &ItemUse, bindings: &mut Vec<ImportBinding>) {
    collect_use_bindings(&use_item.tree, &[], bindings);
}

/// Recursively collect all import bindings from a use tree.
/// For `use crate::models::{User, Order}`, this generates:
///   bindings: "User" → ("crate::models", "User"), "Order" → ("crate::models", "Order")
fn collect_use_bindings(tree: &UseTree, prefix: &[String], bindings: &mut Vec<ImportBinding>) {
    match tree {
        UseTree::Path(path) => {
            let mut new_prefix = prefix.to_vec();
            new_prefix.push(path.ident.to_string());
            collect_use_bindings(&path.tree, &new_prefix, bindings);
        }
        UseTree::Name(name) => {
            let local_name = name.ident.to_string();
            if local_name == "self" {
                return; // `self` in use groups like `{self, ...}` - skip
            }
            let mut full_path = prefix.to_vec();
            full_path.push(local_name.clone());
            let source_module = full_path[..full_path.len() - 1].join("::");
            let exported_name = local_name.clone();

            let is_local = is_crate_local(&source_module);

            if is_local {
                bindings.push(ImportBinding {
                    local_name,
                    source_module,
                    exported_name,
                });
            }
        }
        UseTree::Rename(rename) => {
            let local_name = rename.rename.to_string();
            let mut full_path = prefix.to_vec();
            full_path.push(rename.ident.to_string());
            let source_module = full_path[..full_path.len() - 1].join("::");
            let exported_name = rename.ident.to_string();

            let is_local = is_crate_local(&source_module);

            if is_local {
                bindings.push(ImportBinding {
                    local_name,
                    source_module,
                    exported_name,
                });
            }
        }
        UseTree::Glob(_) => {
            // Skip glob imports for now
        }
        UseTree::Group(group) => {
            for item in &group.items {
                collect_use_bindings(item, prefix, bindings);
            }
        }
    }
}

/// Resolve a Rust module path to an actual file path.
fn resolve_module_path(
    module_path: &str,
    current_file: &Path,
    project_root: &Path,
) -> Option<PathBuf> {
    // Handle `crate` (root module without "::")
    if module_path == "crate" {
        // The crate root is either src/lib.rs or src/main.rs
        let lib_rs = project_root.join("src").join("lib.rs");
        if lib_rs.exists() {
            return Some(lib_rs);
        }
        let main_rs = project_root.join("src").join("main.rs");
        if main_rs.exists() {
            return Some(main_rs);
        }
        return None;
    }

    // Handle `super::` and `crate::` paths relative to current file
    let dir = current_file.parent()?;

    // Remove `crate::` prefix
    let relative = if let Some(rest) = module_path.strip_prefix("crate::") {
        rest
    } else if let Some(rest) = module_path.strip_prefix("self::") {
        rest
    } else if module_path.starts_with("super::") {
        // Go up one directory for each `super::`
        let count = module_path.matches("super::").count();
        let rest_path = module_path.trim_start_matches("super::");
        let mut dir = dir.to_path_buf();
        for _ in 0..count {
            dir = dir.parent()?.to_path_buf();
        }
        // Now resolve `rest_path` relative to `dir`
        let candidate = dir.join(rest_path.replace("::", "/"));
        let with_ext = candidate.with_extension("rs");
        if with_ext.exists() {
            return Some(with_ext);
        }
        // Try as directory with mod.rs
        let mod_rs = candidate.join("mod.rs");
        if mod_rs.exists() {
            return Some(mod_rs);
        }
        return None;
    } else {
        // External crate or absolute path - skip
        return None;
    };

    let candidate = dir.join(relative.replace("::", "/"));
    let with_ext = candidate.with_extension("rs");
    if with_ext.exists() {
        return Some(with_ext);
    }
    // Try as directory with mod.rs
    let mod_rs = candidate.join("mod.rs");
    if mod_rs.exists() {
        return Some(mod_rs);
    }

    None
}

/// Check if a module path is local to the crate (not an external dependency).
fn is_crate_local(module_path: &str) -> bool {
    module_path.starts_with("crate")
        || module_path.starts_with("self")
        || module_path.starts_with("super")
}

/// Build import map: file_path -> (local_name -> (target_file, exported_name))
fn build_import_map(
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
            let module = relative.to_string_lossy().replace('\\', "/");

            let bindings = match parse_import_bindings(&source) {
                Ok(b) => b,
                Err(e) => return (module, Err(e)),
            };
            let mut file_bindings: FileBindings = HashMap::new();

            for binding in bindings {
                if let Some(target_path) = resolve_module_path(&binding.source_module, file, root) {
                    let target_relative = target_path
                        .strip_prefix(root)
                        .unwrap_or(&target_path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    file_bindings
                        .insert(binding.local_name, (target_relative, binding.exported_name));
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

/// Per-file reference extraction result.
struct FileRefs {
    forward: ForwardIndex,
    reverse: ReverseIndex,
}

/// Extract references from a single file.
fn extract_file_refs(
    file: &Path,
    root: &Path,
    import_map: &HashMap<String, FileBindings>,
) -> Option<FileRefs> {
    let source = fs::read_to_string(file).ok()?;

    let relative = file.strip_prefix(root).unwrap_or(file);
    let module = relative.to_string_lossy().replace('\\', "/");

    let file_bindings = import_map.get(&module)?;
    if file_bindings.is_empty() {
        return None;
    }

    let syntax_tree = syn::parse_file(&source).ok()?;

    let mut visitor = ReferenceVisitor {
        file_bindings,
        module: &module,
        forward: HashMap::new(),
        reverse: HashMap::new(),
    };

    visitor.visit_file(&syntax_tree);

    Some(FileRefs {
        forward: visitor.forward,
        reverse: visitor.reverse,
    })
}

/// A visitor that collects references to imported symbols.
struct ReferenceVisitor<'a> {
    file_bindings: &'a FileBindings,
    module: &'a str,
    forward: ForwardIndex,
    reverse: ReverseIndex,
}

impl<'ast> Visit<'ast> for ReferenceVisitor<'ast> {
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if let Expr::Path(expr_path) = &*node.func
            && let Some(local_name) = expr_path.path.segments.first().map(|s| s.ident.to_string())
        {
            self.record_reference(&local_name, node.span(), ReferenceKind::Call);
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_type(&mut self, node: &'ast Type) {
        if let Type::Path(type_path) = node
            && let Some(local_name) = type_path.path.segments.first().map(|s| s.ident.to_string())
        {
            self.record_reference(&local_name, node.span(), ReferenceKind::TypeReference);
        }
        syn::visit::visit_type(self, node);
    }

    fn visit_expr_path(&mut self, node: &'ast ExprPath) {
        if let Some(local_name) = node.path.segments.first().map(|s| s.ident.to_string()) {
            self.record_reference(&local_name, node.span(), ReferenceKind::Usage);
        }
        syn::visit::visit_expr_path(self, node);
    }
}

impl<'a> ReferenceVisitor<'a> {
    fn record_reference(&mut self, local_name: &str, span: proc_macro2::Span, kind: ReferenceKind) {
        if let Some((target_file, exported_name)) = self.file_bindings.get(local_name) {
            let target_symbol_id = format!("{}:{}", target_file, exported_name);
            let source_symbol_id = format!("{}:{}", self.module, local_name);

            let start = span.start();
            let line = start.line;
            let column = start.column;

            let reference_entry = SymbolReference {
                symbol: target_symbol_id.clone(),
                location: Location {
                    file: self.module.to_string(),
                    line,
                    column,
                },
                kind: kind.clone(),
            };

            self.forward
                .entry(source_symbol_id.clone())
                .or_default()
                .push(reference_entry);

            self.reverse
                .entry(target_symbol_id)
                .or_default()
                .push(SymbolReference {
                    symbol: source_symbol_id,
                    location: Location {
                        file: self.module.to_string(),
                        line,
                        column,
                    },
                    kind,
                });
        }
    }
}

/// Extract cross-file references from Rust files.
pub fn extract_all(root: &Path, files: &[PathBuf]) -> anyhow::Result<(ForwardIndex, ReverseIndex)> {
    let import_map = build_import_map(root, files)?;

    let per_file: Vec<FileRefs> = files
        .par_iter()
        .filter_map(|file| extract_file_refs(file, root, &import_map))
        .collect();

    let mut forward: ForwardIndex = HashMap::new();
    let mut reverse: ReverseIndex = HashMap::new();
    for fr in per_file {
        for (k, v) in fr.forward {
            forward.entry(k).or_default().extend(v);
        }
        for (k, v) in fr.reverse {
            reverse.entry(k).or_default().extend(v);
        }
    }

    Ok((forward, reverse))
}
