use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Json,
    Ndjson,
}

impl OutputFormat {
    pub fn from_cli(s: &str) -> anyhow::Result<Self> {
        match s.to_lowercase().as_str() {
            "json" => Ok(Self::Json),
            "ndjson" => Ok(Self::Ndjson),
            _ => anyhow::bail!("unsupported output format: {s}"),
        }
    }
}

pub struct OutputWriter {
    format: OutputFormat,
}

impl OutputWriter {
    pub fn init(output_dir: &Path, format: OutputFormat) -> anyhow::Result<Self> {
        fs::create_dir_all(output_dir)?;
        Ok(Self { format })
    }

    pub fn write<T: serde::Serialize>(
        &self,
        output_dir: &Path,
        relative_path: &str,
        data: &T,
    ) -> anyhow::Result<()> {
        let value = serde_json::to_value(data)?;
        let (content, extension) = if self.format == OutputFormat::Ndjson {
            if let Some(array) = value.as_array() {
                let mut lines = String::new();
                for item in array {
                    lines.push_str(&serde_json::to_string(item)?);
                    lines.push('\n');
                }
                (lines, "ndjson")
            } else {
                (serde_json::to_string_pretty(&value)?, "json")
            }
        } else {
            (serde_json::to_string_pretty(&value)?, "json")
        };

        let path = if self.format == OutputFormat::Ndjson {
            output_dir
                .join(relative_path.trim_end_matches(".json"))
                .with_extension(extension)
        } else {
            output_dir.join(relative_path)
        };

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
        Ok(())
    }
}
