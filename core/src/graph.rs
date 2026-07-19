use crate::discovery::normalize_path_components;
use crate::parser::parse_file_references;
use crate::project::SourceLanguage;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyGraph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub in_degree: usize,
    pub out_degree: usize,
    pub top_dir: String,
    pub dir_path: String,
    pub in_cycle: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cycle {
    pub nodes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleDetectionResult {
    pub has_cycles: bool,
    pub cycles: Vec<Cycle>,
    pub self_loops: Vec<String>,
}

pub struct GraphBuilder;

impl GraphBuilder {
    pub fn build(
        root: &Path,
        files: &[PathBuf],
        source_language: SourceLanguage,
    ) -> anyhow::Result<DependencyGraph> {
        let mut nodes = HashSet::new();
        let mut edges = Vec::new();

        for file in files {
            let relative = file.strip_prefix(root).unwrap_or(file);
            let from = normalize_path_components(relative)
                .to_string_lossy()
                .replace('\\', "/");
            nodes.insert(from.clone());

            let source = fs::read_to_string(file)?;
            let references = parse_file_references(file, &source);

            match references {
                Ok(refs) => {
                    for import in refs.relative_imports {
                        if let Some(to) =
                            resolve_relative_import(file, &import, root, source_language)
                        {
                            nodes.insert(to.clone());
                            edges.push(Edge {
                                from: from.clone(),
                                to,
                            });
                        }
                    }
                }
                Err(e) => eprintln!("Warning: failed to parse {}: {}", file.display(), e),
            }
        }

        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut out_degree: HashMap<String, usize> = HashMap::new();
        for edge in &edges {
            *out_degree.entry(edge.from.clone()).or_default() += 1;
            *in_degree.entry(edge.to.clone()).or_default() += 1;
        }

        let mut node_list: Vec<Node> = nodes
            .into_iter()
            .map(|id| {
                let in_degree = in_degree.get(&id).copied().unwrap_or(0);
                let out_degree = out_degree.get(&id).copied().unwrap_or(0);
                let top_dir = id.split('/').next().unwrap_or("_root").to_string();
                let dir_path = id
                    .rsplit_once('/')
                    .map(|(dir, _)| dir)
                    .unwrap_or("")
                    .to_string();
                Node {
                    id,
                    in_degree,
                    out_degree,
                    top_dir,
                    dir_path,
                    in_cycle: false,
                }
            })
            .collect();
        node_list.sort_by(|a, b| a.id.cmp(&b.id));

        Ok(DependencyGraph {
            nodes: node_list,
            edges,
        })
    }
}

impl DependencyGraph {
    pub fn detect_cycles(&mut self) -> CycleDetectionResult {
        let mut adjacency: HashMap<&String, Vec<&String>> = HashMap::new();
        for edge in &self.edges {
            adjacency.entry(&edge.from).or_default().push(&edge.to);
        }

        let mut self_loops = Vec::new();
        for edge in &self.edges {
            if edge.from == edge.to && !self_loops.contains(&edge.from) {
                self_loops.push(edge.from.clone());
            }
        }

        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut stack = Vec::new();
        let mut on_stack = HashSet::new();

        for node in &self.nodes {
            if !visited.contains(&node.id) {
                self.dfs_cycles(
                    &node.id,
                    &adjacency,
                    &mut visited,
                    &mut stack,
                    &mut on_stack,
                    &mut cycles,
                );
            }
        }

        let mut cycle_nodes: HashSet<String> = HashSet::new();
        for cycle in &cycles {
            for node in &cycle.nodes {
                cycle_nodes.insert(node.clone());
            }
        }
        for node in &self_loops {
            cycle_nodes.insert(node.clone());
        }

        for node in &mut self.nodes {
            node.in_cycle = cycle_nodes.contains(&node.id);
        }

        CycleDetectionResult {
            has_cycles: !cycles.is_empty() || !self_loops.is_empty(),
            cycles,
            self_loops,
        }
    }

    fn dfs_cycles(
        &self,
        node: &String,
        adjacency: &HashMap<&String, Vec<&String>>,
        visited: &mut HashSet<String>,
        stack: &mut Vec<String>,
        on_stack: &mut HashSet<String>,
        cycles: &mut Vec<Cycle>,
    ) {
        visited.insert(node.clone());
        stack.push(node.clone());
        on_stack.insert(node.clone());

        if let Some(neighbors) = adjacency.get(node) {
            for neighbor in neighbors {
                if neighbor == &node {
                    // Self-loop handled separately.
                    continue;
                }
                if !visited.contains(*neighbor) {
                    self.dfs_cycles(neighbor, adjacency, visited, stack, on_stack, cycles);
                } else if on_stack.contains(*neighbor) {
                    // Found a cycle. Extract the cycle portion from the stack.
                    if let Some(pos) = stack.iter().position(|n| n == *neighbor) {
                        let cycle_nodes: Vec<String> = stack[pos..]
                            .iter()
                            .cloned()
                            .chain(std::iter::once((*neighbor).clone()))
                            .collect();
                        if !cycles.iter().any(|c| c.nodes == cycle_nodes) {
                            cycles.push(Cycle { nodes: cycle_nodes });
                        }
                    }
                }
            }
        }

        stack.pop();
        on_stack.remove(node);
    }
}

fn resolve_relative_import(
    file: &Path,
    import: &str,
    root: &Path,
    source_language: SourceLanguage,
) -> Option<String> {
    match source_language {
        SourceLanguage::TypeScript => resolve_typescript_import(file, import, root),
        SourceLanguage::Rust => resolve_rust_import(file, import, root),
    }
}

fn resolve_typescript_import(file: &Path, import: &str, root: &Path) -> Option<String> {
    let parent = file.parent()?;
    let resolved = parent.join(import);

    let candidates = [
        resolved.clone(),
        resolved.with_extension("ts"),
        resolved.with_extension("tsx"),
        resolved.with_extension("js"),
        resolved.join("index.ts"),
        resolved.join("index.tsx"),
        resolved.join("index.js"),
    ];

    for candidate in &candidates {
        if candidate.exists() {
            let relative = candidate.strip_prefix(root).unwrap_or(candidate);
            let module = relative.to_string_lossy().replace('\\', "/");
            return Some(
                normalize_path_components(Path::new(&module))
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }

    None
}

fn resolve_rust_import(file: &Path, import: &str, root: &Path) -> Option<String> {
    // Rust imports use module paths: crate::foo::bar, self::baz, super::qux.
    // Map them to files under the `src` directory.
    let src_root = root.join("src");

    if let Some(module_path) = import.strip_prefix("crate::") {
        return resolve_rust_module_path(&src_root, module_path);
    }

    if let Some(module_path) = import.strip_prefix("self::") {
        let parent = file.parent()?;
        return resolve_rust_module_path(parent, module_path);
    }

    if let Some(module_path) = import.strip_prefix("super::") {
        let parent = file.parent()?;
        let grandparent = parent.parent()?;
        return resolve_rust_module_path(grandparent, module_path);
    }

    // Bare module path without prefix (e.g., from `use foo::bar;` where `foo` is a crate).
    // Treat as external; do not resolve to a local file here.
    None
}

fn resolve_rust_module_path(base: &Path, module_path: &str) -> Option<String> {
    if module_path.is_empty() {
        return None;
    }

    let parts: Vec<&str> = module_path.split("::").collect();
    let mut path = base.to_path_buf();
    for part in &parts[..parts.len() - 1] {
        path = path.join(part);
    }

    let last = parts.last()?;
    let module_file = path.join(format!("{}.rs", last));
    let module_dir = path.join(last).join("mod.rs");

    let candidate = if module_file.exists() {
        module_file
    } else if module_dir.exists() {
        module_dir
    } else {
        return None;
    };

    let root = base
        .ancestors()
        .find(|p| p.join("Cargo.toml").exists())
        .unwrap_or(base);
    let relative = candidate.strip_prefix(root).unwrap_or(&candidate);
    Some(relative.to_string_lossy().replace('\\', "/"))
}
