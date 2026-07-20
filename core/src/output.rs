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

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;
    use tempfile::TempDir;

    #[derive(Serialize)]
    struct TestData {
        name: String,
        value: i32,
    }

    #[test]
    fn test_output_format_from_cli_json() {
        assert_eq!(OutputFormat::from_cli("json").unwrap(), OutputFormat::Json);
        assert_eq!(OutputFormat::from_cli("JSON").unwrap(), OutputFormat::Json);
    }

    #[test]
    fn test_output_format_from_cli_ndjson() {
        assert_eq!(OutputFormat::from_cli("ndjson").unwrap(), OutputFormat::Ndjson);
    }

    #[test]
    fn test_output_format_from_cli_invalid() {
        let err = OutputFormat::from_cli("xml").unwrap_err();
        assert!(err.to_string().contains("unsupported"));
    }

    #[test]
    fn test_output_writer_init_creates_dir() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("nested/output");
        let writer = OutputWriter::init(&sub, OutputFormat::Json).unwrap();
        assert!(sub.exists());
        drop(writer);
    }

    #[test]
    fn test_output_writer_write_json() {
        let dir = TempDir::new().unwrap();
        let writer = OutputWriter::init(dir.path(), OutputFormat::Json).unwrap();
        let data = TestData {
            name: "test".into(),
            value: 42,
        };
        writer.write(dir.path(), "output.json", &data).unwrap();
        let content = std::fs::read_to_string(dir.path().join("output.json")).unwrap();
        assert!(content.contains("\"name\": \"test\""));
        assert!(content.contains("\"value\": 42"));
    }

    #[test]
    fn test_output_writer_write_ndjson() {
        let dir = TempDir::new().unwrap();
        let writer = OutputWriter::init(dir.path(), OutputFormat::Ndjson).unwrap();
        let data = vec![
            TestData {
                name: "a".into(),
                value: 1,
            },
            TestData {
                name: "b".into(),
                value: 2,
            },
        ];
        writer.write(dir.path(), "data.json", &data).unwrap();
        let content = std::fs::read_to_string(dir.path().join("data.ndjson")).unwrap();
        let lines: Vec<&str> = content.trim().lines().collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_output_writer_default_format() {
        let format = OutputFormat::default();
        assert_eq!(format, OutputFormat::Json);
    }
}
