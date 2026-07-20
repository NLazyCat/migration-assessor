use super::{AstNode, DiffAnalyzer, Language, ParsedFile};
use crate::deps::rust as deps_rust;
use crate::symbols::rust as symbols_rust;
use std::path::Path;
use syn::visit::Visit;

pub struct RustLanguage;

impl Language for RustLanguage {
    fn name(&self) -> &str {
        "rust"
    }

    fn file_extensions(&self) -> &[&str] {
        &["rs"]
    }

    fn parse(&self, source: &str, file_path: &str) -> anyhow::Result<ParsedFile<'_>> {
        let file: syn::File = syn::parse_file(source)?;

        Ok(ParsedFile {
            source: source.to_string(),
            file_path: file_path.to_string(),
            language: "rust".to_string(),
            ast: AstNode::Rust(file),
            diagnostics: Vec::new(),
        })
    }

    fn extract_symbols(&self, parsed: &ParsedFile) -> anyhow::Result<(crate::symbols::SymbolIndex, crate::symbols::ApiContract)> {
        let relative = Path::new(&parsed.file_path);
        let module = relative.to_string_lossy().replace('\\', "/");
        let (index, contract) = symbols_rust::extract(&module, &parsed.source)?;
        Ok((index, contract))
    }

    fn extract_references(
        &self,
        parsed: &ParsedFile,
    ) -> anyhow::Result<(crate::references::ForwardIndex, crate::references::ReverseIndex)> {
        let mut forward: crate::references::ForwardIndex = Default::default();
        let mut reverse: crate::references::ReverseIndex = Default::default();

        if let AstNode::Rust(file) = &parsed.ast {
            let mut visitor = UseVisitor { imports: Vec::new() };
            visitor.visit_file(file);

            for import in &visitor.imports {
                let ref_id = format!("{}:use:{}", parsed.file_path, import);
                forward.insert(ref_id.clone(), vec![]);
                reverse.insert(import.clone(), vec![]);
            }
        }

        Ok((forward, reverse))
    }

    fn resolve_dependencies(&self, project_root: &Path) -> anyhow::Result<Vec<crate::deps::ResolvedDependency>> {
        deps_rust::resolve(project_root)
    }

    fn diff_analyzer(&self) -> &dyn DiffAnalyzer {
        &RustDiffAnalyzer
    }

    fn detect_project_type(&self, project_root: &Path) -> bool {
        project_root.join("Cargo.toml").exists()
    }
}

struct UseVisitor {
    imports: Vec<String>,
}

impl<'ast> Visit<'ast> for UseVisitor {
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        let path_str = use_tree_to_string(&node.tree);
        if !path_str.is_empty() {
            self.imports.push(path_str);
        }
    }
}

fn use_tree_to_string(tree: &syn::UseTree) -> String {
    match tree {
        syn::UseTree::Path(path) => {
            let prefix = path.ident.to_string();
            let rest = use_tree_to_string(&path.tree);
            if rest.is_empty() { prefix } else { format!("{}::{}", prefix, rest) }
        }
        syn::UseTree::Name(name) => name.ident.to_string(),
        syn::UseTree::Rename(rename) => rename.ident.to_string(),
        syn::UseTree::Glob(_) => "*".to_string(),
        syn::UseTree::Group(group) => {
            let items: Vec<String> = group.items.iter().map(use_tree_to_string).collect();
            items.join(", ")
        }
    }
}

pub struct RustDiffAnalyzer;

impl DiffAnalyzer for RustDiffAnalyzer {
    fn extract_imports(&self, parsed: &ParsedFile) -> Vec<String> {
        if let AstNode::Rust(file) = &parsed.ast {
            let mut visitor = UseVisitor { imports: Vec::new() };
            visitor.visit_file(file);
            visitor.imports.sort();
            visitor.imports.dedup();
            visitor.imports
        } else {
            Vec::new()
        }
    }

    fn extract_call_graph(&self, parsed: &ParsedFile) -> Vec<(String, String)> {
        if let AstNode::Rust(file) = &parsed.ast {
            let mut visitor = CallVisitor { calls: Vec::new(), current_fn: String::new() };
            visitor.visit_file(file);
            visitor.calls.sort();
            visitor.calls.dedup();
            visitor.calls
        } else {
            Vec::new()
        }
    }
}

struct CallVisitor {
    calls: Vec<(String, String)>,
    current_fn: String,
}

impl<'ast> Visit<'ast> for CallVisitor {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let old_fn = self.current_fn.clone();
        self.current_fn = node.sig.ident.to_string();
        syn::visit::visit_item_fn(self, node);
        self.current_fn = old_fn;
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        let callee = self.extract_callee_name(&node.func);
        if !callee.is_empty() {
            self.calls.push((self.current_fn.clone(), callee));
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        let method_name = node.method.to_string();
        self.calls.push((self.current_fn.clone(), method_name));
        syn::visit::visit_expr_method_call(self, node);
    }
}

impl CallVisitor {
    fn extract_callee_name(&self, expr: &syn::Expr) -> String {
        match expr {
            syn::Expr::Path(path) => {
                path.path.segments.last().map(|s| s.ident.to_string()).unwrap_or_default()
            }
            syn::Expr::Call(call) => self.extract_callee_name(&call.func),
            _ => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_language_name() {
        assert_eq!(RustLanguage.name(), "rust");
    }

    #[test]
    fn test_rust_file_extensions() {
        let exts = RustLanguage.file_extensions();
        assert_eq!(exts, &["rs"]);
    }

    #[test]
    fn test_rust_parse_valid_source() {
        let source = "fn main() { println!(\"hello\"); }";
        let parsed = RustLanguage.parse(source, "main.rs").unwrap();
        assert_eq!(parsed.source, source);
        assert_eq!(parsed.file_path, "main.rs");
        assert!(matches!(parsed.ast, AstNode::Rust(_)));
    }

    #[test]
    fn test_rust_parse_invalid_source() {
        let source = "fn main( { }";
        let result = RustLanguage.parse(source, "main.rs");
        assert!(result.is_err());
    }

    #[test]
    fn test_rust_detect_project_type() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname=\"test\"\n").unwrap();
        assert!(RustLanguage.detect_project_type(dir.path()));
    }

    #[test]
    fn test_rust_detect_project_type_no_match() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(!RustLanguage.detect_project_type(dir.path()));
    }

    #[test]
    fn test_rust_diff_analyzer_extract_imports() {
        let source = "use std::collections::HashMap;\nuse serde::{Serialize, Deserialize};\nmod foo;";
        let file: syn::File = syn::parse_file(source).unwrap();
        let parsed = ParsedFile {
            source: source.to_string(),
            file_path: "lib.rs".to_string(),
            language: "rust".to_string(),
            ast: AstNode::Rust(file),
            diagnostics: vec![],
        };
        let imports = RustDiffAnalyzer.extract_imports(&parsed);
        assert!(imports.iter().any(|i| i.contains("HashMap")));
        assert!(imports.iter().any(|i| i.contains("Serialize")));
    }

    #[test]
    fn test_rust_diff_analyzer_extract_call_graph() {
        let source = "fn foo() { bar(); baz::qux(); }";
        let file: syn::File = syn::parse_file(source).unwrap();
        let parsed = ParsedFile {
            source: source.to_string(),
            file_path: "lib.rs".to_string(),
            language: "rust".to_string(),
            ast: AstNode::Rust(file),
            diagnostics: vec![],
        };
        let calls = RustDiffAnalyzer.extract_call_graph(&parsed);
        assert!(calls.contains(&("foo".to_string(), "bar".to_string())));
    }

    #[test]
    fn test_rust_no_imports_when_not_rust_ast() {
        let parsed = ParsedFile {
            source: String::new(),
            file_path: "test.ts".to_string(),
            language: "typescript".to_string(),
            ast: AstNode::Other(serde_json::json!({})),
            diagnostics: vec![],
        };
        let imports = RustDiffAnalyzer.extract_imports(&parsed);
        assert!(imports.is_empty());
    }
}
