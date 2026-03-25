use crate::source::{SourceFile, Span};
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    pub note: String,
    pub span: Span,
    pub fix_it: Option<String>,
}

impl Diagnostic {
    pub fn error(
        code: &'static str,
        message: impl Into<String>,
        note: impl Into<String>,
        span: Span,
    ) -> Self {
        Self {
            code,
            severity: Severity::Error,
            message: message.into(),
            note: note.into(),
            span,
            fix_it: None,
        }
    }

    pub fn with_fix_it(mut self, fix_it: impl Into<String>) -> Self {
        self.fix_it = Some(fix_it.into());
        self
    }

    pub fn render(&self, source: &SourceFile) -> String {
        let line = source
            .text()
            .lines()
            .nth(self.span.line.saturating_sub(1))
            .unwrap_or_default();
        let caret_width = self.span.end_column.saturating_sub(self.span.column).max(1);
        let padding = " ".repeat(self.span.column.saturating_sub(1));
        let marker = "^".repeat(caret_width);
        let fix_it = self
            .fix_it
            .as_ref()
            .map(|value| format!("\nfix-it: {value}"))
            .unwrap_or_default();

        format!(
            "{}:{}:{}: {}: {}\n{}\n{}{}\nnote: {}{}",
            source.path().display(),
            self.span.line,
            self.span.column,
            self.code,
            self.message,
            line,
            padding,
            marker,
            self.note,
            fix_it
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diagnostics(pub Vec<Diagnostic>);

impl Diagnostics {
    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.0.push(diagnostic);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn codes(&self) -> Vec<&'static str> {
        self.0.iter().map(|diagnostic| diagnostic.code).collect()
    }

    pub fn render(&self, source: &SourceFile) -> String {
        self.0
            .iter()
            .map(|diagnostic| diagnostic.render(source))
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

impl From<Vec<Diagnostic>> for Diagnostics {
    fn from(value: Vec<Diagnostic>) -> Self {
        Self(value)
    }
}

impl Display for Diagnostics {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for (index, diagnostic) in self.0.iter().enumerate() {
            if index > 0 {
                writeln!(f)?;
            }
            write!(f, "{}: {}", diagnostic.code, diagnostic.message)?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostics {}
