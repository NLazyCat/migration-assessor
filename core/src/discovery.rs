use crate::project::SourceLanguage;
use std::path::{Component, Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

/// Normalize `.` and `..` segments without touching the filesystem.
pub(crate) fn normalize_path_components(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    result.push("..");
                }
            }
            other => result.push(other),
        }
    }
    result
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
            if let Some(name) = part.as_os_str().to_str()
                && default_skipped.contains(&name)
            {
                return false;
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
