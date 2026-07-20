use super::{javascript, typescript};
use crate::project::SourceLanguage;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Map each analyzed module (relative file path) to the set of external
/// dependency package names it imports (e.g. `express`, `lodash`, `@scope/pkg`).
///
/// This is the bridge that lets the scoring and recommendation modules reason
/// about compatibility at the module level instead of the whole project.
pub fn module_external_deps(
    root: &Path,
    files: &[PathBuf],
    source_language: SourceLanguage,
) -> HashMap<String, Vec<String>> {
    let entries: Vec<(String, Vec<String>)> = files
        .par_iter()
        .filter_map(|file| {
            let source = fs::read_to_string(file).ok()?;
            let relative = file.strip_prefix(root).unwrap_or(file);
            let module = relative.to_string_lossy().replace('\\', "/");
            let deps = extract_external_specifiers(&source, source_language);
            if deps.is_empty() {
                None
            } else {
                Some((module, deps))
            }
        })
        .collect();

    entries.into_iter().collect()
}

/// Extract external (non-relative, non-path-alias) import specifiers from a
/// single source file and normalize them to package names.
fn extract_external_specifiers(source: &str, lang: SourceLanguage) -> Vec<String> {
    match lang {
        SourceLanguage::TypeScript => typescript::extract_external_specifiers(source),
        SourceLanguage::JavaScript => javascript::extract_external_specifiers(source),
        SourceLanguage::Rust => extract_rust_external(source),
    }
}

fn extract_rust_external(source: &str) -> Vec<String> {
    let mut packages: Vec<String> = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("use ") {
            continue;
        }
        // `use crate::`, `use self::`, `use super::` are local.
        if trimmed.starts_with("use crate")
            || trimmed.starts_with("use self")
            || trimmed.starts_with("use super")
        {
            continue;
        }
        // Extract the path before `::` or `;`.
        let after_use = match trimmed.strip_prefix("use ") {
            Some(s) => s,
            None => continue,
        };
        // Strip `pub `, attributes, and the `as` / `{...}` parts.
        let path = after_use
            .split(" as ")
            .next()
            .unwrap_or(after_use)
            .split(';')
            .next()
            .unwrap_or(after_use)
            .trim();
        // Handle grouped `use { a, b::c }` minimally by taking first segment.
        let crate_name = path.split("::").next().unwrap_or(path).trim();
        if crate_name.is_empty()
            || crate_name == "crate"
            || crate_name == "self"
            || crate_name == "super"
        {
            continue;
        }
        if !packages.contains(&crate_name.to_string()) {
            packages.push(crate_name.to_string());
        }
    }
    packages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_ts_external_packages() {
        let src = r#"
import express from 'express';
import { foo } from 'lodash';
import { Bar } from '@scope/pkg/sub';
import './local';
import something from '../relative';
import x from '#alias/x';
"#;
        let mut pkgs = typescript::extract_external_specifiers(src);
        pkgs.sort();
        assert_eq!(pkgs, vec!["@scope/pkg", "express", "lodash"]);
    }

    #[test]
    fn extracts_rust_external_crates() {
        let src = r#"
use serde::Serialize;
use tokio::sync::Mutex;
use crate::local_mod;
use self::inner;
use super::parent;
use std::collections::HashMap;
"#;
        let mut pkgs = extract_rust_external(src);
        pkgs.sort();
        assert_eq!(pkgs, vec!["serde", "std", "tokio"]);
    }
}
