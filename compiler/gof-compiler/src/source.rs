use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct SourceFile {
    path: PathBuf,
    text: String,
}

impl SourceFile {
    pub fn new(path: impl Into<PathBuf>, text: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            text: text.into(),
        }
    }

    pub fn from_path(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref();
        Ok(Self::new(
            path.to_path_buf(),
            std::fs::read_to_string(path)?,
        ))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Span {
    pub line: usize,
    pub column: usize,
    pub end_column: usize,
}

impl Span {
    pub const fn new(line: usize, column: usize, end_column: usize) -> Self {
        Self {
            line,
            column,
            end_column,
        }
    }
}
