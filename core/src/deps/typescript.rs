use super::ResolvedDependency;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Default)]
struct PackageJson {
    dependencies: HashMap<String, String>,
    dev_dependencies: HashMap<String, String>,
    workspaces: Vec<String>,
}

pub fn resolve(root: &Path) -> anyhow::Result<Vec<ResolvedDependency>> {
    let root_package = read_package_json(&root.join("package.json"))?;
    let mut all_packages = vec![root_package];

    let workspace_globs = collect_workspace_globs(&all_packages[0]);
    for workspace_package in discover_workspace_packages(root, &workspace_globs)? {
        all_packages.push(workspace_package);
    }

    // Try to enrich with lock file data
    let lock_data = try_parse_lock_file(root);

    let mut merged: HashMap<String, ResolvedDependency> = HashMap::new();
    for package in &all_packages {
        merge_dependencies(&mut merged, &package.dependencies, "prod");
        merge_dependencies(&mut merged, &package.dev_dependencies, "dev");
    }

    // Apply lock file resolution (resolved versions, tree structure)
    if let Some(ref lock) = lock_data {
        for dep in merged.values_mut() {
            if let Some(lock_entry) = lock.get(&dep.name) {
                dep.version.clone_from(&lock_entry.version);
                dep.resolved = lock_entry.resolved.clone();
                dep.children = lock_entry.children.clone();
            }
        }
    }

    let mut resolved: Vec<ResolvedDependency> = merged.into_values().collect();
    resolved.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(resolved)
}

/// Parse package-lock.json or npm-shrinkwrap.json to get resolved versions and tree.
fn try_parse_lock_file(root: &Path) -> Option<HashMap<String, LockEntry>> {
    let lock_paths = ["package-lock.json", "npm-shrinkwrap.json"];
    for name in &lock_paths {
        let path = root.join(name);
        if path.exists() {
            if let Ok(data) = parse_package_lock(&path) {
                return Some(data);
            }
        }
    }
    None
}

#[derive(Debug, Clone)]
struct LockEntry {
    version: String,
    resolved: Option<String>,
    children: Vec<ResolvedDependency>,
}

fn parse_package_lock(path: &Path) -> anyhow::Result<HashMap<String, LockEntry>> {
    let content = fs::read_to_string(path)?;
    let json: Value = serde_json::from_str(&content)?;

    let packages = match json.get("packages") {
        Some(Value::Object(m)) => m,
        _ => return Ok(HashMap::new()),
    };

    let mut entries: HashMap<String, LockEntry> = HashMap::new();
    // First pass: collect all packages
    for (pkg_path, info) in packages {
        let name = extract_package_name(pkg_path, info);
        if name.is_empty() {
            continue;
        }
        let version = info
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("0.0.0")
            .to_string();
        let resolved = info
            .get("resolved")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Collect direct dependency names from lock
        let dep_names: Vec<String> = info
            .get("dependencies")
            .and_then(|d| d.as_object())
            .map(|obj| obj.keys().cloned().collect())
            .unwrap_or_default();

        entries.insert(
            name.clone(),
            LockEntry {
                version,
                resolved,
                children: dep_names
                    .iter()
                    .map(|n| ResolvedDependency {
                        name: n.clone(),
                        version: String::new(),
                        resolved: None,
                        dependencies: vec![],
                        children: vec![],
                        dep_type: "prod".to_string(),
                    })
                    .collect(),
            },
        );
    }

    // Second pass: wire up children recursively
    let keys: Vec<String> = entries.keys().cloned().collect();
    for key in &keys {
        if let Some(entry) = entries.get(key) {
            let mut resolved_children = Vec::new();
            for child in &entry.children {
                if let Some(child_entry) = entries.get(&child.name) {
                    let mut rc = ResolvedDependency {
                        name: child.name.clone(),
                        version: child_entry.version.clone(),
                        resolved: child_entry.resolved.clone(),
                        dependencies: child_entry
                            .children
                            .iter()
                            .map(|c| c.name.clone())
                            .collect(),
                        children: vec![],
                        dep_type: "prod".to_string(),
                    };
                    // Recursively fill children
                    fill_children(&mut rc, &entries);
                    resolved_children.push(rc);
                } else {
                    resolved_children.push(child.clone());
                }
            }
            if let Some(entry) = entries.get_mut(key) {
                entry.children = resolved_children;
            }
        }
    }

    Ok(entries)
}

fn fill_children(dep: &mut ResolvedDependency, all_entries: &HashMap<String, LockEntry>) {
    let children_names: Vec<String> = dep.children.iter().map(|c| c.name.clone()).collect();
    let mut resolved = Vec::new();
    for name in &children_names {
        if let Some(entry) = all_entries.get(name) {
            let mut child = ResolvedDependency {
                name: name.clone(),
                version: entry.version.clone(),
                resolved: entry.resolved.clone(),
                dependencies: entry.children.iter().map(|c| c.name.clone()).collect(),
                children: vec![],
                dep_type: "prod".to_string(),
            };
            fill_children(&mut child, all_entries);
            resolved.push(child);
        }
    }
    dep.children = resolved;
}

fn extract_package_name(pkg_path: &str, info: &Value) -> String {
    // package-lock.json v3 uses "node_modules/pkg-name" paths
    if pkg_path.is_empty() || pkg_path == "" {
        return String::new();
    }
    // Try name field first (for the root package)
    if let Some(name) = info.get("name").and_then(|v| v.as_str()) {
        if !name.is_empty() && pkg_path.is_empty() {
            return name.to_string();
        }
    }
    // Extract from path: "node_modules/@scope/pkg" or "node_modules/pkg"
    let parts: Vec<&str> = pkg_path.split("node_modules/").collect();
    if let Some(last) = parts.last() {
        if !last.is_empty() {
            return last.to_string();
        }
    }
    String::new()
}

fn read_package_json(path: &Path) -> anyhow::Result<PackageJson> {
    let content = fs::read_to_string(path)?;
    let value: Value = serde_json::from_str(&content)?;

    Ok(PackageJson {
        dependencies: extract_dependency_map(value.get("dependencies")),
        dev_dependencies: extract_dependency_map(value.get("devDependencies")),
        workspaces: extract_workspaces(&value),
    })
}

fn extract_dependency_map(value: Option<&Value>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some(obj) = value.and_then(|v| v.as_object()) {
        for (name, version_value) in obj {
            if let Some(version) = version_value.as_str() {
                map.insert(name.clone(), version.to_string());
            }
        }
    }
    map
}

fn extract_workspaces(value: &Value) -> Vec<String> {
    if let Some(arr) = value.get("workspaces").and_then(|w| w.as_array()) {
        return arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
    }

    if let Some(arr) = value
        .get("workspaces")
        .and_then(|w| w.get("packages"))
        .and_then(|p| p.as_array())
    {
        return arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
    }

    vec![]
}

fn collect_workspace_globs(package: &PackageJson) -> Vec<String> {
    package.workspaces.clone()
}

fn discover_workspace_packages(root: &Path, globs: &[String]) -> anyhow::Result<Vec<PackageJson>> {
    let mut packages = Vec::new();

    for pattern in globs {
        let search_pattern = root.join(pattern).join("package.json");
        let pattern_str = search_pattern.to_string_lossy();

        for entry in glob::glob(&pattern_str)? {
            let path = entry?;
            if path == root.join("package.json") {
                continue;
            }
            match read_package_json(&path) {
                Ok(package) => packages.push(package),
                Err(e) => eprintln!("Warning: failed to read {}: {}", path.display(), e),
            }
        }
    }

    Ok(packages)
}

fn merge_dependencies(
    merged: &mut HashMap<String, ResolvedDependency>,
    dependencies: &HashMap<String, String>,
    dep_type: &str,
) {
    for (name, version) in dependencies {
        merged.insert(
            name.clone(),
            ResolvedDependency {
                name: name.clone(),
                version: version.clone(),
                resolved: None,
                dependencies: vec![],
                children: vec![],
                dep_type: dep_type.to_string(),
            },
        );
    }
}
