use std::fmt;

/// Source location for diagnostics.
#[derive(Clone, Debug)]
pub enum Location {
    Unknown,
    FileLineCol { file: String, line: u32, col: u32 },
    Fused { locations: Vec<Location> },
}

impl Location {
    pub fn unknown() -> Self {
        Location::Unknown
    }

    pub fn file_line_col(file: impl Into<String>, line: u32, col: u32) -> Self {
        Location::FileLineCol {
            file: file.into(),
            line,
            col,
        }
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Location::Unknown => write!(f, "<unknown>"),
            Location::FileLineCol { file, line, col } => {
                write!(f, "{}:{}:{}", file, line, col)
            }
            Location::Fused { locations } => {
                write!(f, "fused[")?;
                for (i, loc) in locations.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", loc)?;
                }
                write!(f, "]")
            }
        }
    }
}

impl Default for Location {
    fn default() -> Self {
        Location::Unknown
    }
}
