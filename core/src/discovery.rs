use crate::project::SourceLanguage;
use crate::util;
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

/// Normalize `.` and `..` segments without touching the filesystem.
pub(crate) fn normalize_path_components(path: &Path) -> PathBuf {
    util::normalize_path(path)
}

/// Built-in framework boilerplate glob patterns.
/// Activated by `--skip-framework`.
/// Only excludes framework-specific renderer packages and UI component library boilerplate.
const FRAMEWORK_PATTERNS: &[&str] = &[
    // Framework-specific renderer packages (monorepo pattern)
    "packages/vue/**",
    "packages/svelte/**",
    "packages/solid/**",
    "packages/react/**",
    "packages/next/**",
    "packages/ink/**",
    "packages/remotion/**",
    "packages/image/**",
    "packages/react-email/**",
    "packages/react-native/**",
    "packages/react-pdf/**",
    "packages/react-three-fiber/**",
    "packages/react-state/**",
    // Framework devtools adapter packages
    "packages/devtools-react/**",
    "packages/devtools-solid/**",
    "packages/devtools-svelte/**",
    "packages/devtools-vue/**",
    // Component library boilerplate (shadcn)
    "packages/shadcn/**",
    "packages/shadcn-svelte/**",
    "packages/ui/**",
    // Generic shadcn UI pattern across any project
    "**/components/ui/*",
];

pub struct FileDiscovery {
    pub source_language: SourceLanguage,
    pub ignore_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
    pub skip_framework: bool,
}

impl FileDiscovery {
    pub fn new(
        source_language: SourceLanguage,
        ignore_patterns: Vec<String>,
        exclude_patterns: Vec<String>,
        skip_framework: bool,
    ) -> Self {
        Self {
            source_language,
            ignore_patterns,
            exclude_patterns,
            skip_framework,
        }
    }

    pub fn discover(&self, root: &Path) -> Vec<PathBuf> {
        let files: Vec<PathBuf> = WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| self.should_traverse(e, root))
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| self.should_include(e.path(), root))
            .map(|e| e.path().to_path_buf())
            .collect();
        // Normalize `.`/`..` segments that WalkDir may emit when traversing
        // Windows junctions. This ensures module names in reports are clean.
        files
            .iter()
            .filter_map(|f| {
                let rel = f.strip_prefix(root).unwrap_or(f);
                let cleaned = normalize_path_components(rel);
                // Skip files that would escape the root via `..`
                if cleaned
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    return None;
                }
                Some(root.join(cleaned))
            })
            .collect()
    }

    fn should_traverse(&self, entry: &DirEntry, root: &Path) -> bool {
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap_or(path);

        let default_skipped = ["node_modules", "target", ".git", "dist", "build"];
        for part in relative.components() {
            if let Some(name) = part.as_os_str().to_str() {
                if default_skipped.contains(&name) {
                    return false;
                }
                // Skip migration output directories created by a previous run
                if name.ends_with("-migration") {
                    return false;
                }
            }
        }

        true
    }

    fn should_include(&self, path: &Path, root: &Path) -> bool {
        let extension = path.extension().and_then(|e| e.to_str());

        let matches_language = match self.source_language {
            SourceLanguage::TypeScript => matches!(extension, Some("ts") | Some("tsx")),
            SourceLanguage::Rust => matches!(extension, Some("rs")),
        };

        if !matches_language {
            return false;
        }

        let relative = path.strip_prefix(root).unwrap_or(path);

        // Check config ignore patterns
        for pattern in &self.ignore_patterns {
            if glob::Pattern::new(pattern)
                .ok()
                .map(|p| p.matches_path(relative))
                .unwrap_or(false)
            {
                return false;
            }
        }

        // Check CLI --exclude patterns
        for pattern in &self.exclude_patterns {
            if glob::Pattern::new(pattern)
                .ok()
                .map(|p| p.matches_path(relative))
                .unwrap_or(false)
            {
                return false;
            }
        }

        // Check built-in framework patterns
        if self.skip_framework {
            for pattern in FRAMEWORK_PATTERNS {
                if glob::Pattern::new(pattern)
                    .ok()
                    .map(|p| p.matches_path(relative))
                    .unwrap_or(false)
                {
                    return false;
                }
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_ts_project() -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/index.ts"), "export const x = 1;").unwrap();
        fs::write(dir.path().join("src/app.tsx"), "export const App = () => null;").unwrap();
        fs::write(dir.path().join("README.md"), "docs").unwrap();
        fs::create_dir_all(dir.path().join("node_modules/pkg")).unwrap();
        fs::write(dir.path().join("node_modules/pkg/index.ts"), "").unwrap();
        dir
    }

    fn setup_rust_project() -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(dir.path().join("src/lib.rs"), "pub fn foo() {}").unwrap();
        fs::create_dir_all(dir.path().join("target/debug")).unwrap();
        fs::write(dir.path().join("target/debug/test.rs"), "").unwrap();
        dir
    }

    #[test]
    fn test_discover_ts_files() {
        let dir = setup_ts_project();
        let discovery = FileDiscovery::new(SourceLanguage::TypeScript, vec![], vec![], false);
        let files = discovery.discover(dir.path());
        let names: Vec<String> = files.iter().map(|f| f.to_string_lossy().to_string()).collect();
        assert!(names.iter().any(|n| n.replace('\\', "/").contains("index.ts")));
        assert!(names.iter().any(|n| n.replace('\\', "/").contains("app.tsx")));
    }

    #[test]
    fn test_discover_ignores_node_modules() {
        let dir = setup_ts_project();
        let discovery = FileDiscovery::new(SourceLanguage::TypeScript, vec![], vec![], false);
        let files = discovery.discover(dir.path());
        assert!(!files.iter().any(|f| f.to_string_lossy().replace('\\', "/").contains("node_modules")));
    }

    #[test]
    fn test_discover_ignores_non_source() {
        let dir = setup_ts_project();
        let discovery = FileDiscovery::new(SourceLanguage::TypeScript, vec![], vec![], false);
        let files = discovery.discover(dir.path());
        assert!(!files.iter().any(|f| f.to_string_lossy().replace('\\', "/").contains("README")));
    }

    #[test]
    fn test_discover_rust_files() {
        let dir = setup_rust_project();
        let discovery = FileDiscovery::new(SourceLanguage::Rust, vec![], vec![], false);
        let files = discovery.discover(dir.path());
        assert!(files.iter().any(|f| f.to_string_lossy().replace('\\', "/").contains("main.rs")));
        assert!(files.iter().any(|f| f.to_string_lossy().replace('\\', "/").contains("lib.rs")));
    }

    #[test]
    fn test_discover_ignores_target() {
        let dir = setup_rust_project();
        let discovery = FileDiscovery::new(SourceLanguage::Rust, vec![], vec![], false);
        let files = discovery.discover(dir.path());
        assert!(!files.iter().any(|f| f.to_string_lossy().replace('\\', "/").contains("target")));
    }

    #[test]
    fn test_discover_ignore_pattern() {
        let dir = setup_ts_project();
        let discovery = FileDiscovery::new(SourceLanguage::TypeScript, vec!["src/app*".into()], vec![], false);
        let files = discovery.discover(dir.path());
        assert!(!files.iter().any(|f| f.to_string_lossy().replace('\\', "/").contains("app.tsx")));
        assert!(files.iter().any(|f| f.to_string_lossy().replace('\\', "/").contains("index.ts")));
    }

    #[test]
    fn test_discover_exclude_pattern() {
        let dir = setup_ts_project();
        let discovery = FileDiscovery::new(SourceLanguage::TypeScript, vec![], vec!["**/index.ts".into()], false);
        let files = discovery.discover(dir.path());
        assert!(!files.iter().any(|f| f.to_string_lossy().replace('\\', "/").contains("index.ts")));
    }

    #[test]
    fn test_discover_skip_framework() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("p");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(root.join("packages/react")).unwrap();
        fs::write(root.join("packages/react/index.ts"), "").unwrap();
        fs::create_dir_all(root.join("components/ui")).unwrap();
        fs::write(root.join("components/ui/button.ts"), "").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/index.ts"), "export const x = 1;").unwrap();

        let discovery = FileDiscovery::new(SourceLanguage::TypeScript, vec![], vec![], true);
        let files = discovery.discover(&root);
        let paths: Vec<String> = files.iter().map(|f| f.to_string_lossy().replace('\\', "/")).collect();
        assert!(!paths.iter().any(|p| p.contains("packages/react")), "should exclude framework: got {paths:?}");
        assert!(!paths.iter().any(|p| p.contains("components/ui")), "should exclude shadcn: got {paths:?}");
        assert!(paths.iter().any(|p| p.contains("src/index.ts")), "should include src/index.ts: got {paths:?}");
    }

    #[test]
    fn test_discover_empty_directory() {
        let dir = TempDir::new().unwrap();
        let discovery = FileDiscovery::new(SourceLanguage::TypeScript, vec![], vec![], false);
        let files = discovery.discover(dir.path());
        assert!(files.is_empty());
    }

    #[test]
    fn test_discover_skips_migration_dirs() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("some-migration")).unwrap();
        fs::write(dir.path().join("some-migration/foo.ts"), "").unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/index.ts"), "export const x = 1;").unwrap();

        let discovery = FileDiscovery::new(SourceLanguage::TypeScript, vec![], vec![], false);
        let files = discovery.discover(dir.path());
        assert!(!files.iter().any(|f| f.to_string_lossy().contains("some-migration")));
    }

    #[test]
    fn test_should_traverse_rejects_git() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".git/config"), "").unwrap();
        let discovery = FileDiscovery::new(SourceLanguage::TypeScript, vec![], vec![], false);
        let files = discovery.discover(dir.path());
        assert!(!files.iter().any(|f| f.to_string_lossy().contains(".git")));
    }
}
