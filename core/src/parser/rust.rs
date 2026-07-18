use super::ModuleReferences;
use syn::{
    visit::Visit, File, ItemUse, UseGlob, UseName, UsePath, UseRename,
};

struct ImportVisitor {
    relative_imports: Vec<String>,
    external_imports: Vec<String>,
}

impl ImportVisitor {
    fn new() -> Self {
        Self {
            relative_imports: Vec::new(),
            external_imports: Vec::new(),
        }
    }
}

impl<'ast> Visit<'ast> for ImportVisitor {
    fn visit_item_use(&mut self, node: &'ast ItemUse) {
        let mut path = use_tree_to_string(&node.tree);
        if node.leading_colon.is_some() {
            path = format!("::{}", path);
        }

        if path.starts_with("crate::")
            || path.starts_with("super::")
            || path.starts_with("self::")
        {
            self.relative_imports.push(path);
        } else {
            self.external_imports.push(path);
        }
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if node.content.is_none() {
            // `mod foo;` references a sibling file (foo.rs or foo/mod.rs).
            self.relative_imports
                .push(format!("self::{}", node.ident));
        }
        syn::visit::visit_item_mod(self, node);
    }
}

fn use_tree_to_string(tree: &syn::UseTree) -> String {
    match tree {
        syn::UseTree::Path(UsePath { ident, tree, .. }) => {
            format!("{}::{}", ident, use_tree_to_string(tree))
        }
        syn::UseTree::Name(UseName { ident, .. }) => ident.to_string(),
        syn::UseTree::Rename(UseRename { ident, rename, .. }) => {
            format!("{} as {}", ident, rename)
        }
        syn::UseTree::Glob(UseGlob { .. }) => "*".to_string(),
        syn::UseTree::Group(group) => {
            let items: Vec<String> = group.items.iter().map(use_tree_to_string).collect();
            format!("{{{}}}", items.join(", "))
        }
    }
}

pub fn parse_references(source: &str) -> anyhow::Result<ModuleReferences> {
    let syntax_tree: File = syn::parse_file(source)?;

    let mut visitor = ImportVisitor::new();
    visitor.visit_file(&syntax_tree);

    Ok(ModuleReferences {
        relative_imports: visitor.relative_imports,
        external_imports: visitor.external_imports,
    })
}
