use crate::ir::location::Location;
use std::fmt;

/// Severity level of a diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
    Remark,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
            Severity::Note => write!(f, "note"),
            Severity::Remark => write!(f, "remark"),
        }
    }
}

/// A diagnostic message with location and severity.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub severity: Severity,
    pub location: Location,
    pub message: String,
    pub notes: Vec<Diagnostic>,
}

impl Diagnostic {
    pub fn error(location: Location, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            location,
            message: message.into(),
            notes: Vec::new(),
        }
    }

    pub fn warning(location: Location, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            location,
            message: message.into(),
            notes: Vec::new(),
        }
    }

    pub fn note(location: Location, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Note,
            location,
            message: message.into(),
            notes: Vec::new(),
        }
    }

    pub fn with_note(mut self, note: Diagnostic) -> Self {
        self.notes.push(note);
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}: {}", self.location, self.severity, self.message)?;
        for note in &self.notes {
            write!(f, "\n  {}", note)?;
        }
        Ok(())
    }
}

/// Error type for MLIF operations that can fail with diagnostics.
#[derive(Debug)]
pub struct DiagnosticError {
    pub diagnostics: Vec<Diagnostic>,
}

impl DiagnosticError {
    pub fn new(diagnostic: Diagnostic) -> Self {
        Self {
            diagnostics: vec![diagnostic],
        }
    }

    pub fn single(location: Location, message: impl Into<String>) -> Self {
        Self::new(Diagnostic::error(location, message))
    }
}

impl fmt::Display for DiagnosticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, d) in self.diagnostics.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{}", d)?;
        }
        Ok(())
    }
}

impl std::error::Error for DiagnosticError {}
