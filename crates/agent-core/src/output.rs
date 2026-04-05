use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Compact text for CLI usage (minimal tokens)
    Text,
    /// JSON for MCP server / programmatic usage
    Json,
}

pub struct OutputFormatter {
    format: OutputFormat,
}

impl OutputFormatter {
    pub fn new(format: OutputFormat) -> Self {
        Self { format }
    }

    pub fn text() -> Self {
        Self::new(OutputFormat::Text)
    }

    pub fn json() -> Self {
        Self::new(OutputFormat::Json)
    }

    pub fn format(&self) -> OutputFormat {
        self.format
    }

    /// Format a serializable value according to the output format.
    /// For Text format, uses the provided text representation.
    /// For Json format, serializes to JSON.
    pub fn output<T: Serialize>(&self, text: &str, json_value: &T) -> String {
        match self.format {
            OutputFormat::Text => text.to_string(),
            OutputFormat::Json => {
                serde_json::to_string(json_value).unwrap_or_else(|_| text.to_string())
            }
        }
    }
}
