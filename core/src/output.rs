use std::fs;
use std::path::Path;

pub struct OutputWriter;

impl OutputWriter {
    pub fn init(output_dir: &Path) -> anyhow::Result<Self> {
        if output_dir.exists() {
            fs::remove_dir_all(output_dir)?;
        }
        fs::create_dir_all(output_dir)?;
        Ok(Self)
    }

    pub fn write_json<T: serde::Serialize>(
        &self,
        output_dir: &Path,
        relative_path: &str,
        data: &T,
    ) -> anyhow::Result<()> {
        let path = output_dir.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(data)?;
        fs::write(path, content)?;
        Ok(())
    }
}
