use super::{
    FileBindings, ForwardIndex, ImportBinding, Location, ReferenceKind, ReverseIndex,
    SymbolReference,
};
use crate::util;
use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_ast::ast::{self, Function, Statement};
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::GetSpan;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Detected path alias mapping: import prefix → target directory prefix.
#[derive(Debug, Clone)]
pub struct PathAlias {
    pub import_prefix: String,
    pub target_prefix: String,
}

/// Auto-detects path aliases from package.json#imports and tsconfig.json#paths.
#[derive(Debug, Clone)]
pub struct PathAliasResolver {
    aliases: Vec<PathAlias>,
}

impl PathAliasResolver {
    pub fn empty() -> Self {
        Self {
            aliases: Vec::new(),
        }
    }

    pub fn with_aliases(aliases: Vec<PathAlias>) -> Self {
        Self { aliases }
    }

    /// Detect aliases by walking up from `project_root` and scanning for package.json files.
    pub fn detect(project_root: &Path) -> Self {
        let mut aliases = Vec::new();

        // Phase 1: walk up the directory tree
        let mut dir = Some(project_root);
        while let Some(d) = dir {
            Self::scan_dir(d, &mut aliases);
            dir = d.parent();
        }

        // Phase 2: walk down to find package.json files with imports (limit depth to avoid huge trees)
        Self::scan_down(project_root, &mut aliases, 3);

        if !aliases.is_empty() {
            eprintln!("  [aliases] detected {} path alias(es):", aliases.len());
            for a in &aliases {
                eprintln!("    {} → {}", a.import_prefix, a.target_prefix);
            }
        }

        Self { aliases }
    }

    fn scan_dir(dir: &Path, aliases: &mut Vec<PathAlias>) {
        let pkg_json = dir.join("package.json");
        if pkg_json.exists()
            && let Ok(content) = fs::read_to_string(&pkg_json)
            && let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content)
        {
            Self::extract_package_imports(&pkg, dir, aliases);
        }
        let tsconfig = dir.join("tsconfig.json");
        if tsconfig.exists()
            && let Ok(content) = fs::read_to_string(&tsconfig)
            && let Ok(cfg) = serde_json::from_str::<serde_json::Value>(&content)
        {
            Self::extract_tsconfig_paths(&cfg, dir, aliases);
        }
    }

    fn scan_down(dir: &Path, aliases: &mut Vec<PathAlias>, depth: usize) {
        if depth == 0 {
            return;
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                // Skip common non-source directories
                if name == "node_modules"
                    || name == ".git"
                    || name == "dist"
                    || name == ".next"
                    || name.starts_with('.')
                {
                    continue;
                }
                Self::scan_dir(&path, aliases);
                Self::scan_down(&path, aliases, depth - 1);
            }
        }
    }

    fn extract_package_imports(pkg: &serde_json::Value, pkg_dir: &Path, out: &mut Vec<PathAlias>) {
        let Some(imports) = pkg.get("imports").and_then(|v| v.as_object()) else {
            return;
        };

        for (pattern, target) in imports {
            if !pattern.starts_with('#') {
                continue;
            }
            let target_str = match target {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Object(obj) => {
                    // Prefer source/dev conditions over default (which may point to dist/)
                    if let Some(v) = obj
                        .get("eve-source")
                        .or_else(|| obj.get("development"))
                        .or_else(|| obj.get("source"))
                        .or_else(|| obj.get("default"))
                    {
                        if let Some(s) = v.as_str() {
                            s.replacen("./dist/", "./", 1)
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };

            if let Some(wildcard_pos) = pattern.find('*') {
                let prefix = &pattern[..wildcard_pos];
                let target_prefix = if let Some(tw) = target_str.find('*') {
                    &target_str[..tw]
                } else {
                    &target_str
                };
                let joined = pkg_dir.join(target_prefix);
                let normalized = util::normalize_path(&joined);
                let target_str = path_to_forward_slash(&normalized);
                out.push(PathAlias {
                    import_prefix: prefix.to_string(),
                    target_prefix: target_str,
                });
            }
        }
    }

    fn extract_tsconfig_paths(
        cfg: &serde_json::Value,
        tsconfig_dir: &Path,
        out: &mut Vec<PathAlias>,
    ) {
        let Some(paths) = cfg
            .get("compilerOptions")
            .and_then(|c| c.get("paths"))
            .and_then(|v| v.as_object())
        else {
            return;
        };

        for (pattern, targets) in paths {
            let Some(arr) = targets.as_array() else {
                continue;
            };
            let Some(first) = arr.first().and_then(|v| v.as_str()) else {
                continue;
            };

            if let Some(wildcard_pos) = pattern.find('*') {
                let import_prefix = &pattern[..wildcard_pos];
                let target_dir = if let Some(tw) = first.find('*') {
                    &first[..tw]
                } else {
                    first
                };
                let joined = tsconfig_dir.join(target_dir);
                let normalized = util::normalize_path(&joined);
                let target_str = path_to_forward_slash(&normalized);
                out.push(PathAlias {
                    import_prefix: import_prefix.to_string(),
                    target_prefix: target_str,
                });
            }
        }
    }

    /// Strip trailing `.js` extension from an import specifier since TS source uses `.js` in imports.
    fn strip_js_ext(s: &str) -> String {
        if s.ends_with(".js") && !s.ends_with(".json") {
            s[..s.len() - 3].to_string()
        } else {
            s.to_string()
        }
    }

    /// Try to resolve an aliased import to a filesystem path.
    /// Returns an absolute path that probe_path can check.
    pub fn resolve_alias(&self, source_module: &str, _project_root: &Path) -> Option<PathBuf> {
        for alias in &self.aliases {
            if let Some(rest) = source_module.strip_prefix(&alias.import_prefix) {
                let rest = Self::strip_js_ext(rest);
                let base = Path::new(&alias.target_prefix);
                let full = base.join(rest);
                return Some(util::normalize_path(&full));
            }
        }
        None
    }

    pub fn is_empty(&self) -> bool {
        self.aliases.is_empty()
    }
}

/// Parse import binding information from a TypeScript file.
pub fn parse_import_bindings(
    source: &str,
    file_path: Option<&Path>,
) -> anyhow::Result<Vec<ImportBinding>> {
    let source_type = util::detect_source_type(file_path);
    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, source, source_type).parse();
    let mut bindings = Vec::new();

    for stmt in &ret.program.body {
        match stmt {
            Statement::ImportDeclaration(import) => {
                let source_module = import.source.value.to_string();
                // Keep all imports (relative and aliased). Filtering happens at resolve time.
                let is_relative = source_module.starts_with('.') || source_module.starts_with('/');
                let is_aliased = source_module.starts_with('#')
                    || self::non_relative_might_be_alias(&source_module);
                if !is_relative && !is_aliased {
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
                    let is_relative =
                        source_module.starts_with('.') || source_module.starts_with('/');
                    let is_aliased = source_module.starts_with('#')
                        || self::non_relative_might_be_alias(&source_module);
                    if !is_relative && !is_aliased {
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

fn non_relative_might_be_alias(source: &str) -> bool {
    if source.starts_with('#') && source.len() > 1 {
        return true;
    }
    if source.contains('/') && !source.starts_with('.') {
        return true;
    }
    false
}

/// Get the name from a ModuleExportName.
fn imported_name(name: &ast::ModuleExportName) -> String {
    match name {
        ast::ModuleExportName::IdentifierName(id) => id.name.to_string(),
        ast::ModuleExportName::IdentifierReference(id) => id.name.to_string(),
        ast::ModuleExportName::StringLiteral(s) => s.value.to_string(),
    }
}

/// Resolve a relative or aliased import path to an actual file path relative to project root.
fn resolve_import_with_resolver(
    imp: &str,
    current_file: &Path,
    project_root: &Path,
    resolver: &PathAliasResolver,
) -> Option<PathBuf> {
    // Handle relative paths
    let dir = current_file.parent()?;
    let resolved = dir.join(imp);
    let normalized = util::normalize_path(&resolved);

    if let Some(path) = probe_path(&normalized, project_root) {
        return Some(to_relative(&path, project_root));
    }

    // Try alias resolution
    if !resolver.is_empty()
        && let Some(alias_resolved) = resolver.resolve_alias(imp, project_root)
        && let Some(path) = probe_path(&alias_resolved, project_root)
    {
        return Some(to_relative(&path, project_root));
    }

    None
}

/// Convert an absolute path to be relative to project_root, using forward slashes.
fn to_relative(abs: &Path, project_root: &Path) -> PathBuf {
    let stripped = abs.strip_prefix(project_root).unwrap_or(abs);
    PathBuf::from(path_to_forward_slash(stripped))
}

fn probe_path(path: &Path, project_root: &Path) -> Option<PathBuf> {
    if path.extension().and_then(|e| e.to_str()).is_some() {
        if path.exists() {
            return Some(project_root.join(trim_to_relative(path, project_root)));
        }
    } else {
        for ext in &["ts", "tsx"] {
            let with_ext = path.with_extension(ext);
            if with_ext.exists() {
                return Some(project_root.join(trim_to_relative(&with_ext, project_root)));
            }
        }
        for ext in &["ts", "tsx"] {
            let index = path.join(format!("index.{}", ext));
            if index.exists() {
                return Some(project_root.join(trim_to_relative(&index, project_root)));
            }
        }
    }
    None
}

/// Convert path to forward slashes (used for cross-platform report keys).
fn path_to_forward_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
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
) -> anyhow::Result<HashMap<String, FileBindings>> {
    let resolver = PathAliasResolver::detect(root);
    let resolver_arc = std::sync::Arc::new(resolver);

    let entries: Vec<(String, anyhow::Result<FileBindings>)> = files
        .par_iter()
        .map(|file| {
            let resolver = resolver_arc.clone();
            let source = match fs::read_to_string(file) {
                Ok(s) => s,
                Err(e) => return (String::new(), Err(e.into())),
            };
            let relative = file.strip_prefix(root).unwrap_or(file);
            let module = path_to_forward_slash(relative);

            let bindings = match parse_import_bindings(&source, Some(file)) {
                Ok(b) => b,
                Err(e) => return (module, Err(e)),
            };
            let mut file_bindings: FileBindings = HashMap::new();

            for binding in bindings {
                if let Some(target_relative) =
                    resolve_import_with_resolver(&binding.source_module, file, root, &resolver)
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
                        if impl_span.start <= ref_span.start && ref_span.end <= impl_span.end {
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
            | AstKind::Function(Function { id: Some(_), .. })
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


