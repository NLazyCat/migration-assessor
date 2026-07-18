use super::{ApiContract, ApiExport, Param, Symbol, SymbolIndex};
use proc_macro2::Span;
use quote::quote;
use syn::spanned::Spanned;
use syn::{
    visit::Visit, File, FnArg, GenericParam, ItemConst, ItemEnum, ItemFn, ItemImpl, ItemStatic,
    ItemStruct, ItemTrait, ItemType, Pat, ReturnType, Signature, Type, Visibility,
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
        if attr.path().is_ident("doc") {
            if let syn::Meta::NameValue(meta) = &attr.meta {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(lit_str),
                    ..
                }) = &meta.value
                {
                    lines.push(lit_str.value().trim().to_string());
                }
            }
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join(" "))
    }
}

fn extract_params(sig: &Signature) -> Vec<Param> {
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

fn extract_return_type(return_type: &ReturnType) -> Option<String> {
    match return_type {
        ReturnType::Default => None,
        ReturnType::Type(_, ty) => Some(type_to_string(ty)),
    }
}

fn extract_generics(params: &syn::punctuated::Punctuated<GenericParam, syn::Token![,]>) -> Vec<String> {
    params.iter().map(|p| quote!(#p).to_string()).collect()
}

fn type_to_string(ty: &Type) -> String {
    quote!(#ty).to_string()
}

fn format_function_signature(sig: &Signature) -> String {
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
