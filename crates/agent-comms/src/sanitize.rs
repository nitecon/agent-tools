/// Sanitise a raw string into a safe project identifier.
///
/// The output is lower-cased, non-alphanumeric characters (except `-` and `_`)
/// are replaced with hyphens, consecutive hyphens are collapsed, leading and
/// trailing hyphens are stripped, and the result is truncated to 100 characters.
///
/// # Examples
/// ```
/// use agent_comms::sanitize::sanitize_name;
/// assert_eq!(sanitize_name("My Cool  Project!!"), "my-cool-project");
/// assert_eq!(sanitize_name("---edge---"), "edge");
/// ```
pub fn sanitize_name(raw: &str) -> String {
    let lower = raw.to_lowercase();
    let replaced: String = lower
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();

    let mut collapsed = String::new();
    let mut prev_hyphen = false;
    for c in replaced.chars() {
        if c == '-' {
            if !prev_hyphen {
                collapsed.push(c);
            }
            prev_hyphen = true;
        } else {
            collapsed.push(c);
            prev_hyphen = false;
        }
    }

    let stripped = collapsed.trim_matches('-').to_string();
    if stripped.len() > 100 {
        stripped[..100].trim_matches('-').to_string()
    } else {
        stripped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_sanitize() {
        assert_eq!(sanitize_name("Hello World"), "hello-world");
    }

    #[test]
    fn collapses_hyphens() {
        assert_eq!(sanitize_name("a---b"), "a-b");
    }

    #[test]
    fn strips_leading_trailing() {
        assert_eq!(sanitize_name("---edge---"), "edge");
    }

    #[test]
    fn preserves_underscores() {
        assert_eq!(sanitize_name("my_project"), "my_project");
    }

    #[test]
    fn truncates_long_names() {
        let long = "a".repeat(200);
        assert!(sanitize_name(&long).len() <= 100);
    }

    #[test]
    fn special_chars_replaced() {
        assert_eq!(sanitize_name("My Cool  Project!!"), "my-cool-project");
    }
}
