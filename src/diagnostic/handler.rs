use super::diagnostic::{Diagnostic, Severity};

/// Collects diagnostics during IR operations.
pub struct DiagnosticHandler {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticHandler {
    pub fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
        }
    }

    pub fn emit(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    pub fn clear(&mut self) {
        self.diagnostics.clear();
    }

    /// Print all diagnostics to stderr.
    pub fn dump(&self) {
        for d in &self.diagnostics {
            eprintln!("{}", d);
        }
    }
}

impl Default for DiagnosticHandler {
    fn default() -> Self {
        Self::new()
    }
}
