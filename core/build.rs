use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let data_dir = Path::new("compatibility_data");
    println!("cargo:rerun-if-changed=compatibility_data/");

    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir);

    for lang_dir in &["ts_libraries", "rust_libraries"] {
        let dir = data_dir.join(lang_dir);
        if !dir.is_dir() {
            continue;
        }
        let merged = merge_toml_dir(&dir);
        fs::write(out_path.join(format!("{lang_dir}.toml")), &merged).unwrap();
    }
}

fn merge_toml_dir(dir: &Path) -> String {
    let mut merged = String::new();
    let mut entries: Vec<_> = fs::read_dir(dir).unwrap().filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "toml") {
            let content = fs::read_to_string(&path).unwrap();
            merged.push_str(&format!(
                "# Source: {}\n",
                path.file_name().unwrap().to_string_lossy()
            ));
            merged.push_str(&content);
            merged.push('\n');
        }
    }
    merged
}
