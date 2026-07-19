pub const MANIFEST: &str = "manifest.json";
pub const PROJECT: &str = "project.json";
pub const OVERVIEW: &str = "overview.json";
pub const SCORES: &str = "scores.json";
pub const ERRORS: &str = "errors.json";
pub const INDEX_HTML: &str = "index.html";

pub mod external {
    pub const PACKAGES: &str = "external/packages.json";
    pub const COMPATIBILITY: &str = "external/compatibility.json";
}

pub mod graph {
    pub const NODES: &str = "graph/nodes.json";
    pub const EDGES: &str = "graph/edges.json";
    pub const CYCLES: &str = "graph/cycles.json";
}

pub mod symbols {
    pub fn for_module(module: &str) -> String {
        format!("symbols/{}/symbols.json", module)
    }
    pub fn api_for_module(module: &str) -> String {
        format!("symbols/{}/api.json", module)
    }
}

pub mod references {
    pub fn forward_for(file: &str) -> String {
        format!("references/forward/{}.json", file)
    }
    pub fn reverse_for(file: &str) -> String {
        format!("references/reverse/{}.json", file)
    }
}

pub mod boundaries {
    pub const LAYERS: &str = "boundaries/layers.json";
    pub const UNCUT_SURFACES: &str = "boundaries/uncut-surfaces.json";
}

pub mod diffs {
    pub fn dated(name: &str) -> String {
        format!("diffs/{}", name)
    }
    pub const LATEST: &str = "diffs/latest.json";
}

pub mod updates {
    pub const DIFF_OVERVIEW: &str = "updates/diff_overview.json";
    pub const CHANGED_FILES: &str = "updates/changed_files.json";
    pub const DEP_CHANGES: &str = "updates/dep_changes.json";
}
