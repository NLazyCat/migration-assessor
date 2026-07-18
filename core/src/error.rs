use thiserror::Error;

#[allow(dead_code)]
#[derive(Error, Debug)]
pub enum AssessorError {
    #[error("project file not found: {0}")]
    ProjectNotFound(String),
    #[error("failed to parse source file: {path}")]
    SourceParseError {
        path: String,
        #[source]
        source: anyhow::Error,
    },
    #[error("failed to resolve module: {0}")]
    ModuleResolutionError(String),
    #[error("configuration error: {0}")]
    Config(String),
}
