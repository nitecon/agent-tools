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
    sanitize_name_impl(raw)
}

/// Derive a short, gateway-friendly project identifier from a canonical
/// project ident (normalized git URL or filesystem path).
///
/// Takes the final path component, strips a trailing `.git`, then routes
/// the result through [`sanitize_name`]. Falls back to sanitizing the full
/// input if the last segment is empty.
///
/// # Examples
/// ```
/// use agent_comms::sanitize::short_project_ident;
/// assert_eq!(short_project_ident("github.com/nitecon/eventic.git"), "eventic");
/// assert_eq!(short_project_ident("git@github.com:nitecon/agent-tools.git"), "agent-tools");
/// assert_eq!(short_project_ident("/Users/me/Projects/Cool Repo"), "cool-repo");
/// ```
pub fn short_project_ident(canonical: &str) -> String {
    let last = canonical
        .rsplit(['/', '\\', ':'])
        .find(|seg| !seg.is_empty())
        .unwrap_or(canonical);
    let stripped = last.strip_suffix(".git").unwrap_or(last);
    let short = sanitize_name_impl(stripped);
    if short.is_empty() {
        sanitize_name_impl(canonical)
    } else {
        short
    }
}

/// Validate that `key` is safe to embed in an HTTP header value.
///
/// HTTP header values disallow CR, LF, NUL, and (for our purposes) any
/// non-printable or non-ASCII byte. Returns a human-readable error string
/// describing the offending byte and its position when invalid — callers
/// wrap this into their own error type.
///
/// This exists to replace reqwest's opaque "builder error: failed to parse
/// header value" with actionable guidance when an API key has been
/// copy/pasted with a stray newline or non-breaking space.
pub fn validate_api_key(key: &str) -> Result<(), String> {
    if key.is_empty() {
        return Err("gateway API key is empty".to_string());
    }
    for (i, b) in key.bytes().enumerate() {
        // Allow HTAB and printable ASCII (0x20..=0x7E); reject control chars
        // and non-ASCII so header construction never surprises the caller.
        if b != 0x09 && !(0x20..=0x7E).contains(&b) {
            return Err(format!(
                "gateway API key contains invalid character (byte 0x{b:02x} at position {i}); \
                 check your gateway.conf or re-run `agent-tools setup gateway`"
            ));
        }
    }
    Ok(())
}

fn sanitize_name_impl(raw: &str) -> String {
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

    #[test]
    fn short_ident_from_https_git_url() {
        assert_eq!(
            short_project_ident("github.com/nitecon/eventic.git"),
            "eventic"
        );
    }

    #[test]
    fn short_ident_from_ssh_shorthand() {
        assert_eq!(
            short_project_ident("git@github.com:nitecon/agent-tools.git"),
            "agent-tools"
        );
    }

    #[test]
    fn short_ident_strips_trailing_slash() {
        assert_eq!(
            short_project_ident("github.com/nitecon/eventic/"),
            "eventic"
        );
    }

    #[test]
    fn short_ident_from_filesystem_path() {
        assert_eq!(
            short_project_ident("/Users/me/Projects/Cool Repo"),
            "cool-repo"
        );
    }

    #[test]
    fn short_ident_from_windows_path() {
        assert_eq!(
            short_project_ident(r"C:\Users\me\Projects\my-repo"),
            "my-repo"
        );
    }

    #[test]
    fn short_ident_handles_empty_last_segment() {
        // Fallback sanitizes the whole input when no path component survives.
        assert_eq!(short_project_ident("///"), "");
    }

    #[test]
    fn validate_api_key_accepts_printable_ascii() {
        assert!(validate_api_key("sk-abc123_XYZ.!@#").is_ok());
    }

    #[test]
    fn validate_api_key_rejects_empty() {
        assert!(validate_api_key("").is_err());
    }

    #[test]
    fn validate_api_key_rejects_newline() {
        let err = validate_api_key("secret\nkey").unwrap_err();
        assert!(
            err.contains("0x0a"),
            "error should name the bad byte: {err}"
        );
    }

    #[test]
    fn validate_api_key_rejects_non_ascii() {
        // U+00A0 NON-BREAKING SPACE — a very common copy/paste hazard.
        assert!(validate_api_key("secret\u{00a0}key").is_err());
    }
}
