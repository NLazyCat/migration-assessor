use super::{ApiContract, ApiExport, Param, Symbol, SymbolIndex, SymbolParam};
use super::Visibility as SymbolVisibility;
use proc_macro2::Span;
use quote::quote;
use syn::spanned::Spanned;
use syn::{
    File, FnArg, GenericParam, ItemConst, ItemEnum, ItemFn, ItemImpl, ItemStatic, ItemStruct,
    ItemTrait, ItemType, Pat, ReturnType, Signature, Type, Visibility, visit::Visit,
};

struct SymbolVisitor {
    module: String,
    symbols: Vec<Symbol>,
    exports: Vec<ApiExport>,
}

impl SymbolVisitor {
    fn new(module: String) -> Self {
        Self {
            module,
            symbols: Vec::new(),
            exports: Vec::new(),
        }
    }

    fn symbol_id(&self, name: &str) -> String {
        format!("{}:{}", self.module, name)
    }

    fn line_range(&self, span: Span) -> [usize; 2] {
        let start = span.start();
        let end = span.end();
        [start.line, end.line]
    }
}

impl<'ast> Visit<'ast> for SymbolVisitor {
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        if !is_public(&node.vis) {
            syn::visit::visit_item_fn(self, node);
            return;
        }

        let name = node.sig.ident.to_string();
        let id = self.symbol_id(&name);
        let line_range = self.line_range(node.sig.span());

        let params = extract_params(&node.sig);
        let return_type = extract_return_type(&node.sig.output);
        let generics = extract_generics(&node.sig.generics.params);
        let signature = format_function_signature(&node.sig);

        self.symbols.push(Symbol {
            id: id.clone(),
            name: name.clone(),
            kind: "function".to_string(),
            line_range,
            children: Vec::new(),
            partial_analysis: false,
            partial_reason: None,
            visibility: Some(SymbolVisibility::Public),
            value: None,
            signature: Some(signature.clone()),
            doc_comment: extract_doc_comment(&node.attrs),
            attributes: extract_attributes(&node.attrs),
            is_async: Some(node.sig.asyncness.is_some()),
            return_type: extract_return_type(&node.sig.output),
            params: Some(extract_symbol_params(&node.sig)),
        });

        self.exports.push(ApiExport {
            name,
            kind: "function".to_string(),
            generics,
            signature,
            params,
            return_type,
            description: extract_doc_comment(&node.attrs),
            line_range,
            partial_analysis: false,
        });

        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        if !is_public(&node.vis) {
            syn::visit::visit_item_struct(self, node);
            return;
        }

        let name = node.ident.to_string();
        let id = self.symbol_id(&name);
        let line_range = self.line_range(node.ident.span());
        let generics = extract_generics(&node.generics.params);

        let mut children = Vec::new();
        for field in &node.fields {
            if let Some(ident) = &field.ident {
                children.push(Symbol {
                    id: format!("{}.{}", id, ident),
                    name: ident.to_string(),
                    kind: "field".to_string(),
                    line_range: self.line_range(field.span()),
                    children: Vec::new(),
                    partial_analysis: false,
                    partial_reason: None,
                    visibility: Some(SymbolVisibility::Public),
                    value: None,
                    signature: None,
                    doc_comment: None,
                    attributes: Vec::new(),
                    is_async: None,
                    return_type: extract_type_from_field(field),
                    params: None,
                });
            }
        }

        self.symbols.push(Symbol {
            id: id.clone(),
            name: name.clone(),
            kind: "struct".to_string(),
            line_range,
            children,
            partial_analysis: false,
            partial_reason: None,
            visibility: Some(SymbolVisibility::Public),
            value: None,
            signature: None,
            doc_comment: extract_doc_comment(&node.attrs),
            attributes: extract_attributes(&node.attrs),
            is_async: None,
            return_type: None,
            params: None,
        });

        self.exports.push(ApiExport {
            name,
            kind: "struct".to_string(),
            generics,
            signature: format_struct_signature(node),
            params: Vec::new(),
            return_type: None,
            description: extract_doc_comment(&node.attrs),
            line_range,
            partial_analysis: false,
        });

        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
        if !is_public(&node.vis) {
            syn::visit::visit_item_enum(self, node);
            return;
        }

        let name = node.ident.to_string();
        let id = self.symbol_id(&name);
        let line_range = self.line_range(node.ident.span());
        let generics = extract_generics(&node.generics.params);

        let mut children = Vec::new();
        for variant in &node.variants {
            children.push(Symbol {
            id: format!("{}::{}", id, variant.ident),
            name: variant.ident.to_string(),
            kind: "variant".to_string(),
            line_range: self.line_range(variant.span()),
            children: Vec::new(),
            partial_analysis: false,
            partial_reason: None,
            visibility: Some(SymbolVisibility::Public),
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
            id: id.clone(),
            name: name.clone(),
            kind: "enum".to_string(),
            line_range,
            children,
            partial_analysis: false,
            partial_reason: None,
            visibility: Some(SymbolVisibility::Public),
            value: None,
            signature: None,
            doc_comment: extract_doc_comment(&node.attrs),
            attributes: extract_attributes(&node.attrs),
            is_async: None,
            return_type: None,
            params: None,
        });

        self.exports.push(ApiExport {
            name,
            kind: "enum".to_string(),
            generics,
            signature: format_enum_signature(node),
            params: Vec::new(),
            return_type: None,
            description: extract_doc_comment(&node.attrs),
            line_range,
            partial_analysis: false,
        });

        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_trait(&mut self, node: &'ast ItemTrait) {
        if !is_public(&node.vis) {
            syn::visit::visit_item_trait(self, node);
            return;
        }

        let name = node.ident.to_string();
        let id = self.symbol_id(&name);
        let line_range = self.line_range(node.ident.span());
        let generics = extract_generics(&node.generics.params);

        self.symbols.push(Symbol {
            id: id.clone(),
            name: name.clone(),
            kind: "trait".to_string(),
            line_range,
            children: Vec::new(),
            partial_analysis: false,
            partial_reason: None,
            visibility: Some(SymbolVisibility::Public),
            value: None,
            signature: None,
            doc_comment: extract_doc_comment(&node.attrs),
            attributes: extract_attributes(&node.attrs),
            is_async: None,
            return_type: None,
            params: None,
        });

        self.exports.push(ApiExport {
            name,
            kind: "trait".to_string(),
            generics,
            signature: format_trait_signature(node),
            params: Vec::new(),
            return_type: None,
            description: extract_doc_comment(&node.attrs),
            line_range,
            partial_analysis: false,
        });

        syn::visit::visit_item_trait(self, node);
    }

    fn visit_item_type(&mut self, node: &'ast ItemType) {
        if !is_public(&node.vis) {
            syn::visit::visit_item_type(self, node);
            return;
        }

        let name = node.ident.to_string();
        let id = self.symbol_id(&name);
        let line_range = self.line_range(node.ident.span());
        let generics = extract_generics(&node.generics.params);

        self.symbols.push(Symbol {
            id: id.clone(),
            name: name.clone(),
            kind: "type_alias".to_string(),
            line_range,
            children: Vec::new(),
            partial_analysis: false,
            partial_reason: None,
            visibility: Some(SymbolVisibility::Public),
            value: None,
            signature: None,
            doc_comment: extract_doc_comment(&node.attrs),
            attributes: extract_attributes(&node.attrs),
            is_async: None,
            return_type: None,
            params: None,
        });

        self.exports.push(ApiExport {
            name,
            kind: "type_alias".to_string(),
            generics,
            signature: format_type_alias_signature(node),
            params: Vec::new(),
            return_type: None,
            description: extract_doc_comment(&node.attrs),
            line_range,
            partial_analysis: false,
        });

        syn::visit::visit_item_type(self, node);
    }

    fn visit_item_const(&mut self, node: &'ast ItemConst) {
        if !is_public(&node.vis) {
            syn::visit::visit_item_const(self, node);
            return;
        }

        let name = node.ident.to_string();
        let id = self.symbol_id(&name);
        let line_range = self.line_range(node.ident.span());

        self.symbols.push(Symbol {
            id: id.clone(),
            name: name.clone(),
            kind: "const".to_string(),
            line_range,
            children: Vec::new(),
            partial_analysis: false,
            partial_reason: None,
            visibility: Some(SymbolVisibility::Public),
            value: None,
            signature: Some(format!("pub const {}: {}", node.ident, type_to_string(&node.ty))),
            doc_comment: extract_doc_comment(&node.attrs),
            attributes: extract_attributes(&node.attrs),
            is_async: None,
            return_type: Some(type_to_string(&node.ty)),
            params: None,
        });

        self.exports.push(ApiExport {
            name,
            kind: "const".to_string(),
            generics: Vec::new(),
            signature: format!("pub const {}: {}", node.ident, type_to_string(&node.ty)),
            params: Vec::new(),
            return_type: Some(type_to_string(&node.ty)),
            description: extract_doc_comment(&node.attrs),
            line_range,
            partial_analysis: false,
        });

        syn::visit::visit_item_const(self, node);
    }

    fn visit_item_static(&mut self, node: &'ast ItemStatic) {
        if !is_public(&node.vis) {
            syn::visit::visit_item_static(self, node);
            return;
        }

        let name = node.ident.to_string();
        let id = self.symbol_id(&name);
        let line_range = self.line_range(node.ident.span());

        self.symbols.push(Symbol {
            id: id.clone(),
            name: name.clone(),
            kind: "static".to_string(),
            line_range,
            children: Vec::new(),
            partial_analysis: false,
            partial_reason: None,
            visibility: Some(SymbolVisibility::Public),
            value: None,
            signature: Some(format!("pub static {}: {}", node.ident, type_to_string(&node.ty))),
            doc_comment: extract_doc_comment(&node.attrs),
            attributes: extract_attributes(&node.attrs),
            is_async: None,
            return_type: Some(type_to_string(&node.ty)),
            params: None,
        });

        self.exports.push(ApiExport {
            name,
            kind: "static".to_string(),
            generics: Vec::new(),
            signature: format!("pub static {}: {}", node.ident, type_to_string(&node.ty)),
            params: Vec::new(),
            return_type: Some(type_to_string(&node.ty)),
            description: extract_doc_comment(&node.attrs),
            line_range,
            partial_analysis: false,
        });

        syn::visit::visit_item_static(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        // Impl blocks themselves are not exported as top-level symbols, but their
        // public associated methods are interesting for the API contract.
        for item in &node.items {
            if let syn::ImplItem::Fn(method) = item {
                if !is_public(&method.vis) {
                    continue;
                }
                let self_ty = type_to_string(&node.self_ty);
                let name = method.sig.ident.to_string();
                let id = format!("{}::impl:{}", self_ty, name);
                let line_range = self.line_range(method.sig.span());
                let params = extract_params(&method.sig);
                let return_type = extract_return_type(&method.sig.output);
                let generics = extract_generics(&method.sig.generics.params);
                let signature = format_function_signature(&method.sig);

                self.symbols.push(Symbol {
                    id: id.clone(),
                    name: name.clone(),
                    kind: "method".to_string(),
                    line_range,
                    children: Vec::new(),
                    partial_analysis: false,
                    partial_reason: None,
                    visibility: Some(SymbolVisibility::Public),
                    value: None,
                    signature: Some(signature.clone()),
                    doc_comment: extract_doc_comment(&method.attrs),
                    attributes: extract_attributes(&method.attrs),
                    is_async: Some(method.sig.asyncness.is_some()),
                    return_type: extract_return_type(&method.sig.output),
                    params: Some(extract_symbol_params(&method.sig)),
                });

                self.exports.push(ApiExport {
                    name: format!("{}::{}", self_ty, name),
                    kind: "method".to_string(),
                    generics,
                    signature,
                    params,
                    return_type,
                    description: extract_doc_comment(&method.attrs),
                    line_range,
                    partial_analysis: false,
                });
            }
        }

        syn::visit::visit_item_impl(self, node);
    }
}

pub fn extract(module: &str, source: &str) -> anyhow::Result<(SymbolIndex, ApiContract)> {
    let file: File = syn::parse_file(source)?;

    let mut visitor = SymbolVisitor::new(module.to_string());
    visitor.visit_file(&file);

    Ok((
        SymbolIndex {
            module: module.to_string(),
            symbols: visitor.symbols,
        },
        ApiContract {
            module: module.to_string(),
            exports: visitor.exports,
        },
    ))
}

fn is_public(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

fn extract_doc_comment(attrs: &[syn::Attribute]) -> Option<String> {
    let mut lines = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("doc")
            && let syn::Meta::NameValue(meta) = &attr.meta
            && let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(lit_str),
                ..
            }) = &meta.value
        {
            lines.push(lit_str.value().trim().to_string());
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join(" "))
    }
}

pub(crate) fn extract_params(sig: &Signature) -> Vec<Param> {
    sig.inputs
        .iter()
        .map(|arg| match arg {
            FnArg::Receiver(_) => Param {
                name: "self".to_string(),
                ty: "Self".to_string(),
                optional: false,
            },
            FnArg::Typed(pat_type) => {
                let name = match &*pat_type.pat {
                    Pat::Ident(ident) => ident.ident.to_string(),
                    _ => "_".to_string(),
                };
                Param {
                    name,
                    ty: type_to_string(&pat_type.ty),
                    optional: false,
                }
            }
        })
        .collect()
}

pub(crate) fn extract_return_type(return_type: &ReturnType) -> Option<String> {
    match return_type {
        ReturnType::Default => None,
        ReturnType::Type(_, ty) => Some(type_to_string(ty)),
    }
}

fn extract_generics(
    params: &syn::punctuated::Punctuated<GenericParam, syn::Token![,]>,
) -> Vec<String> {
    params.iter().map(|p| quote!(#p).to_string()).collect()
}

fn type_to_string(ty: &Type) -> String {
    quote!(#ty).to_string()
}

pub(crate) fn format_function_signature(sig: &Signature) -> String {
    let name = &sig.ident;
    let generics = if sig.generics.params.is_empty() {
        String::new()
    } else {
        let params = sig
            .generics
            .params
            .iter()
            .map(|p| quote!(#p).to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!("<{}>", params)
    };
    let args = sig
        .inputs
        .iter()
        .map(|arg| match arg {
            FnArg::Receiver(_) => "self".to_string(),
            FnArg::Typed(pat_type) => {
                let name = match &*pat_type.pat {
                    Pat::Ident(ident) => ident.ident.to_string(),
                    _ => "_".to_string(),
                };
                format!("{}: {}", name, type_to_string(&pat_type.ty))
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let ret = match &sig.output {
        ReturnType::Default => String::new(),
        ReturnType::Type(_, ty) => format!(" -> {}", type_to_string(ty)),
    };

    format!("pub fn {}{}({}){}", name, generics, args, ret)
}

fn format_struct_signature(node: &ItemStruct) -> String {
    let name = &node.ident;
    let generics = if node.generics.params.is_empty() {
        String::new()
    } else {
        let params = node
            .generics
            .params
            .iter()
            .map(|p| quote!(#p).to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!("<{}>", params)
    };
    format!("pub struct {}{} {{ ... }}", name, generics)
}

fn format_enum_signature(node: &ItemEnum) -> String {
    let name = &node.ident;
    let generics = if node.generics.params.is_empty() {
        String::new()
    } else {
        let params = node
            .generics
            .params
            .iter()
            .map(|p| quote!(#p).to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!("<{}>", params)
    };
    format!("pub enum {}{} {{ ... }}", name, generics)
}

fn format_trait_signature(node: &ItemTrait) -> String {
    let name = &node.ident;
    let generics = if node.generics.params.is_empty() {
        String::new()
    } else {
        let params = node
            .generics
            .params
            .iter()
            .map(|p| quote!(#p).to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!("<{}>", params)
    };
    format!("pub trait {}{} {{ ... }}", name, generics)
}

fn format_type_alias_signature(node: &ItemType) -> String {
    let name = &node.ident;
    let generics = if node.generics.params.is_empty() {
        String::new()
    } else {
        let params = node
            .generics
            .params
            .iter()
            .map(|p| quote!(#p).to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!("<{}>", params)
    };
    format!(
        "pub type {}{} = {}",
        name,
        generics,
        type_to_string(&node.ty)
    )
}

fn extract_attributes(attrs: &[syn::Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("doc"))
        .map(|attr| quote!(#attr).to_string())
        .collect()
}

fn extract_symbol_params(sig: &Signature) -> Vec<SymbolParam> {
    sig.inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Receiver(_) => None,
            FnArg::Typed(pat_type) => {
                let name = match &*pat_type.pat {
                    Pat::Ident(ident) => ident.ident.to_string(),
                    _ => "_".to_string(),
                };
                Some(SymbolParam {
                    name,
                    ty: type_to_string(&pat_type.ty),
                    optional: false,
                    default_value: None,
                })
            }
        })
        .collect()
}

fn extract_type_from_field(field: &syn::Field) -> Option<String> {
    Some(type_to_string(&field.ty))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_pub_fn() {
        let source = "pub fn add(x: i32, y: i32) -> i32 { x + y }";
        let (index, contract) = extract("lib.rs", source).unwrap();
        assert_eq!(index.symbols.len(), 1);
        assert_eq!(index.symbols[0].name, "add");
        assert_eq!(index.symbols[0].kind, "function");
        assert_eq!(contract.exports.len(), 1);
    }

    #[test]
    fn test_extract_private_fn_skipped() {
        let source = "fn hidden() -> u32 { 42 }";
        let (index, contract) = extract("lib.rs", source).unwrap();
        assert!(index.symbols.is_empty());
        assert!(contract.exports.is_empty());
    }

    #[test]
    fn test_extract_pub_struct() {
        let source = "pub struct User { pub name: String, pub age: u32 }";
        let (index, contract) = extract("lib.rs", source).unwrap();
        assert_eq!(index.symbols.len(), 1);
        assert_eq!(index.symbols[0].name, "User");
        assert_eq!(index.symbols[0].kind, "struct");
        assert_eq!(contract.exports.len(), 1);
    }

    #[test]
    fn test_extract_pub_enum() {
        let source = "pub enum Color { Red, Green, Blue }";
        let (index, contract) = extract("lib.rs", source).unwrap();
        assert_eq!(index.symbols.len(), 1);
        assert_eq!(index.symbols[0].name, "Color");
        assert_eq!(index.symbols[0].kind, "enum");
        assert_eq!(contract.exports.len(), 1);
    }

    #[test]
    fn test_extract_pub_trait() {
        let source = "pub trait Runnable { fn run(&self); }";
        let (index, _contract) = extract("lib.rs", source).unwrap();
        assert_eq!(index.symbols.len(), 1);
        assert_eq!(index.symbols[0].name, "Runnable");
        assert_eq!(index.symbols[0].kind, "trait");
    }

    #[test]
    fn test_extract_pub_type_alias() {
        let source = "pub type Callback = Box<dyn Fn()>;";
        let (index, _contract) = extract("lib.rs", source).unwrap();
        assert_eq!(index.symbols.len(), 1);
        assert_eq!(index.symbols[0].name, "Callback");
        assert_eq!(index.symbols[0].kind, "type_alias");
    }

    #[test]
    fn test_extract_pub_const() {
        let source = "pub const MAX_SIZE: usize = 1024;";
        let (index, _contract) = extract("lib.rs", source).unwrap();
        assert_eq!(index.symbols.len(), 1);
        assert_eq!(index.symbols[0].name, "MAX_SIZE");
        assert_eq!(index.symbols[0].kind, "const");
    }

    #[test]
    fn test_extract_pub_static() {
        let source = "pub static GREETING: &str = \"hello\";";
        let (index, _contract) = extract("lib.rs", source).unwrap();
        assert_eq!(index.symbols.len(), 1);
        assert_eq!(index.symbols[0].name, "GREETING");
        assert_eq!(index.symbols[0].kind, "static");
    }

    #[test]
    fn test_extract_empty_source() {
        let source = "";
        let (index, contract) = extract("lib.rs", source).unwrap();
        assert!(index.symbols.is_empty());
        assert!(contract.exports.is_empty());
    }

    #[test]
    fn test_extract_multiple_items() {
        let source = "pub fn a() {} pub struct B {} pub enum C { X }";
        let (index, contract) = extract("lib.rs", source).unwrap();
        assert_eq!(index.symbols.len(), 3);
        assert_eq!(contract.exports.len(), 3);
    }

    #[test]
    fn test_extract_impl_method() {
        let source = "pub struct Counter { count: i32 }
impl Counter {
    pub fn increment(&mut self) { self.count += 1; }
    fn private_helper(&self) -> i32 { self.count }
}";
        let (index, contract) = extract("lib.rs", source).unwrap();
        assert_eq!(index.symbols.len(), 2);
        let method = &index.symbols[1];
        assert_eq!(method.name, "increment");
        assert_eq!(method.kind, "method");
        assert_eq!(contract.exports.len(), 2);
    }

    #[test]
    fn test_is_public_true() {
        let vis: syn::Visibility = syn::parse2(quote::quote! { pub }).unwrap();
        assert!(is_public(&vis));
    }

    #[test]
    fn test_is_public_false() {
        let vis: syn::Visibility = syn::Visibility::Inherited;
        assert!(!is_public(&vis));
    }
}
