use std::path::{Path, PathBuf};

/// Normalize a path to use forward slashes consistently.
/// On Windows, converts backslashes to forward slashes.
/// Also canonicalizes `.` and `..` components where possible.
pub fn normalize_path(path: &Path) -> PathBuf {
    // Try to canonicalize, fall back to the original path
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    // On Windows, strip the \\?\ prefix that canonicalize adds
    let path_str = resolved.to_string_lossy();
    let cleaned = path_str.strip_prefix(r"\\?\").unwrap_or(&path_str);

    // Convert backslashes to forward slashes
    PathBuf::from(cleaned.replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_forward_slashes() {
        let path = Path::new("src/main.rs");
        let normalized = normalize_path(path);
        let s = normalized.to_string_lossy();
        assert!(!s.contains('\\'), "Should not contain backslashes: {s}");
    }
}
