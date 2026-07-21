use crate::parser::ModuleReferences;
use crate::symbols::{Symbol, SymbolParam, Visibility};
use crate::symbols::rust as rust_symbols;
use std::path::Path;
use syn::spanned::Spanned;

use super::AstOutput;

/// Parse a Rust file and extract symbols, API contracts, and imports
/// in a single AST walk.
pub fn parse(source: &str, file_path: &Path) -> anyhow::Result<AstOutput> {
    let module = file_path
        .to_str()
        .unwrap_or("unknown")
        .replace('\\', "/");
    let syntax_tree: syn::File = syn::parse_file(source)?;

    let mut visitor = UnifiedVisitor::new(module.clone());
    syn::visit::visit_file(&mut visitor, &syntax_tree);

    visitor.relative_imports.sort();
    visitor.relative_imports.dedup();
    visitor.external_imports.sort();
    visitor.external_imports.dedup();

    // Build api_contract from exported symbols
    use crate::symbols::{ApiContract, ApiExport};
    let mut api_exports = Vec::new();
    for sym in &visitor.symbols {
        if matches!(sym.visibility, Some(Visibility::Public)) {
            api_exports.push(ApiExport {
                name: sym.name.clone(),
                kind: sym.kind.clone(),
                generics: vec![],
                signature: sym.signature.clone().unwrap_or_default(),
                params: vec![],
                return_type: sym.return_type.clone(),
                description: sym.doc_comment.clone(),
                line_range: sym.line_range,
                partial_analysis: false,
            });
        }
    }

    Ok(AstOutput {
        file_path: module.clone(),
        source: source.to_string(),
        language: "rust".to_string(),
        symbols: visitor.symbols,
        exports: api_exports.clone(),
        api_contract: ApiContract {
            module,
            exports: api_exports,
        },
        imports: ModuleReferences {
            relative_imports: visitor.relative_imports,
            external_imports: visitor.external_imports,
        },
        diagnostics: vec![],
    })
}

struct UnifiedVisitor {
    module: String,
    symbols: Vec<Symbol>,
    relative_imports: Vec<String>,
    external_imports: Vec<String>,
}

impl UnifiedVisitor {
    fn new(module: String) -> Self {
        Self {
            module,
            symbols: Vec::new(),
            relative_imports: Vec::new(),
            external_imports: Vec::new(),
        }
    }

    fn symbol_id(&self, name: &str) -> String {
        format!("{}:{}", self.module, name)
    }

    fn line_range(&self, span: proc_macro2::Span) -> [usize; 2] {
        let start = span.start();
        let end = span.end();
        [start.line, end.line]
    }
}

impl<'ast> syn::visit::Visit<'ast> for UnifiedVisitor {
    // ── Import extraction ────────────────────────────────────────────

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        let mut path = use_tree_to_string(&node.tree);
        if node.leading_colon.is_some() {
            path = format!("::{}", path);
        }

        if path.starts_with("crate::") || path.starts_with("super::") || path.starts_with("self::")
        {
            self.relative_imports.push(path);
        } else {
            self.external_imports.push(path);
        }
        // Continue walking for nested items
        syn::visit::visit_item_use(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if node.content.is_none() {
            // `mod foo;` references a sibling file
            self.relative_imports.push(format!("self::{}", node.ident));
        }
        syn::visit::visit_item_mod(self, node);
    }

    // ── Symbol extraction ────────────────────────────────────────────

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if !is_public(&node.vis) {
            syn::visit::visit_item_fn(self, node);
            return;
        }

        let name = node.sig.ident.to_string();
        let id = self.symbol_id(&name);
        let line_range = self.line_range(node.sig.span());

        let params = rust_symbols::extract_params(&node.sig);
        let return_type = rust_symbols::extract_return_type(&node.sig.output);
        let signature = rust_symbols::format_function_signature(&node.sig);

        let symbol_params: Vec<SymbolParam> = params
            .iter()
            .map(|p| SymbolParam {
                name: p.name.clone(),
                ty: p.ty.clone(),
                optional: false,
                default_value: None,
            })
            .collect();

        self.symbols.push(Symbol {
            id,
            name,
            kind: "function".to_string(),
            line_range,
            children: Vec::new(),
            partial_analysis: false,
            partial_reason: None,
            visibility: Some(Visibility::Public),
            value: None,
            signature: Some(signature),
            doc_comment: None,
            attributes: Vec::new(),
            is_async: Some(node.sig.asyncness.is_some()),
            return_type,
            params: Some(symbol_params),
        });
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        if !is_public(&node.vis) {
            syn::visit::visit_item_struct(self, node);
            return;
        }

        let name = node.ident.to_string();
        let id = self.symbol_id(&name);
        let line_range = self.line_range(node.struct_token.span());

        let mut children = Vec::new();
        for field in &node.fields {
            let field_name = field
                .ident
                .as_ref()
                .map(|i| i.to_string())
                .unwrap_or_default();
            children.push(Symbol {
                id: format!("{}:{}", id, field_name),
                name: field_name,
                kind: "field".to_string(),
                line_range: self.line_range(field.span()),
                children: Vec::new(),
                partial_analysis: false,
                partial_reason: None,
                visibility: if is_public(&field.vis) {
                    Some(Visibility::Public)
                } else {
                    Some(Visibility::Private)
                },
                value: None,
                signature: None,
                doc_comment: None,
                attributes: Vec::new(),
                is_async: None,
                return_type: None,
                params: None,
            });
        }

        self.symbols.push(Symbol {
            id,
            name,
            kind: "struct".to_string(),
            line_range,
            children,
            partial_analysis: false,
            partial_reason: None,
            visibility: Some(Visibility::Public),
            value: None,
            signature: None,
            doc_comment: None,
            attributes: Vec::new(),
            is_async: None,
            return_type: None,
            params: None,
        });
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        if !is_public(&node.vis) {
            return;
        }
        let name = node.ident.to_string();
        let id = self.symbol_id(&name);
        let line_range = self.line_range(node.enum_token.span());
        self.symbols.push(Symbol {
            id,
            name,
            kind: "enum".to_string(),
            line_range,
            children: Vec::new(),
            partial_analysis: false,
            partial_reason: None,
            visibility: Some(Visibility::Public),
            value: None,
            signature: None,
            doc_comment: None,
            attributes: Vec::new(),
            is_async: None,
            return_type: None,
            params: None,
        });
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        if !is_public(&node.vis) {
            return;
        }
        let name = node.ident.to_string();
        let id = self.symbol_id(&name);
        let line_range = self.line_range(node.trait_token.span());
        self.symbols.push(Symbol {
            id,
            name,
            kind: "trait".to_string(),
            line_range,
            children: Vec::new(),
            partial_analysis: false,
            partial_reason: None,
            visibility: Some(Visibility::Public),
            value: None,
            signature: None,
            doc_comment: None,
            attributes: Vec::new(),
            is_async: None,
            return_type: None,
            params: None,
        });
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

fn is_public(vis: &syn::Visibility) -> bool {
    matches!(vis, syn::Visibility::Public(_))
}

fn use_tree_to_string(tree: &syn::UseTree) -> String {
    match tree {
        syn::UseTree::Path(syn::UsePath { ident, tree, .. }) => {
            format!("{}::{}", ident, use_tree_to_string(tree))
        }
        syn::UseTree::Name(syn::UseName { ident, .. }) => ident.to_string(),
        syn::UseTree::Rename(syn::UseRename { ident, rename, .. }) => {
            format!("{} as {}", ident, rename)
        }
        syn::UseTree::Glob(syn::UseGlob { .. }) => "*".to_string(),
        syn::UseTree::Group(group) => {
            let items: Vec<String> = group.items.iter().map(use_tree_to_string).collect();
            format!("{{{}}}", items.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_exported_function() {
        let source =
            "pub fn greet(name: &str) -> String { format!(\"hello {}\", name) }";
        let result = parse(source, Path::new("lib.rs")).unwrap();
        assert_eq!(result.symbols.len(), 1);
        assert_eq!(result.symbols[0].name, "greet");
        assert_eq!(result.symbols[0].kind, "function");
    }

    #[test]
    fn test_parse_with_imports() {
        let source = "use std::collections::HashMap;\nuse crate::helper::foo;\npub fn bar() {}";
        let result = parse(source, Path::new("lib.rs")).unwrap();
        assert!(result.imports.external_imports.iter().any(|i| i.contains("HashMap")));
        assert!(result.imports.relative_imports.iter().any(|i| i.contains("crate::helper")));
        assert_eq!(result.symbols.len(), 1);
    }

    #[test]
    fn test_parse_private_function() {
        let source = "fn hidden() -> i32 { 42 }";
        let result = parse(source, Path::new("lib.rs")).unwrap();
        // Private functions are not extracted
        assert_eq!(result.symbols.len(), 0);
    }

    #[test]
    fn test_parse_struct() {
        let source = "pub struct User { pub name: String, age: u32 }";
        let result = parse(source, Path::new("lib.rs")).unwrap();
        assert_eq!(result.symbols.len(), 1);
        assert_eq!(result.symbols[0].kind, "struct");
        assert_eq!(result.symbols[0].children.len(), 2);
    }

    #[test]
    fn test_parse_mod_decl() {
        let source = "mod foo;\npub fn bar() {}";
        let result = parse(source, Path::new("lib.rs")).unwrap();
        assert!(result.imports.relative_imports.contains(&"self::foo".to_string()));
    }
}
