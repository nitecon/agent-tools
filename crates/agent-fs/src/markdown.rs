use anyhow::{Context, Result};
use serde::Serialize;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct DocHeading {
    pub level: usize,
    pub text: String,
    pub line: usize,
}

/// Extract every markdown heading (`#`-prefixed lines) from a file.
///
/// Lines inside fenced code blocks are skipped so example code containing
/// `#` comments is not mistaken for headings. Fence tracking follows the
/// CommonMark rule: a fence is opened by 3+ backticks or 3+ tildes and is
/// closed only by the same character with at least the same count, so
/// nested fences (e.g. a ```` ```bash ```` block inside a ` ```` markdown ` block)
/// are handled correctly.
pub fn extract_headings(path: &Path) -> Result<Vec<DocHeading>> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let reader = BufReader::new(file);

    let mut headings = Vec::new();
    let mut fence: Option<Fence> = None;

    for (idx, line) in reader.lines().enumerate() {
        let line_num = idx + 1;
        let line =
            line.with_context(|| format!("reading line {line_num} of {}", path.display()))?;
        let trimmed = line.trim_start();

        if let Some(f) = detect_fence(trimmed) {
            update_fence(&mut fence, f);
            continue;
        }
        if fence.is_some() {
            continue;
        }

        let level = heading_level(trimmed);
        if level > 0 {
            headings.push(DocHeading {
                level,
                text: heading_text(trimmed, level),
                line: line_num,
            });
        }
    }

    Ok(headings)
}

/// Extract the body of the section whose heading text matches `section`
/// (case-insensitive). The returned string contains the heading line itself
/// and continues until the next heading at the same or higher level
/// (i.e. equal or smaller `#` count), or end-of-file.
pub fn extract_section(path: &Path, section: &str) -> Result<String> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let reader = BufReader::new(file);

    let mut output = String::new();
    let mut fence: Option<Fence> = None;
    let mut capturing = false;
    let mut capture_level = 0usize;

    for line in reader.lines() {
        let line = line.with_context(|| format!("reading {}", path.display()))?;
        let trimmed = line.trim_start();

        if let Some(f) = detect_fence(trimmed) {
            update_fence(&mut fence, f);
            if capturing {
                output.push_str(&line);
                output.push('\n');
            }
            continue;
        }

        if fence.is_none() {
            let level = heading_level(trimmed);
            if level > 0 {
                let text = heading_text(trimmed, level);

                if capturing && level <= capture_level {
                    break;
                }

                if !capturing && text.eq_ignore_ascii_case(section) {
                    capturing = true;
                    capture_level = level;
                }
            }
        }

        if capturing {
            output.push_str(&line);
            output.push('\n');
        }
    }

    if output.is_empty() {
        anyhow::bail!("section '{}' not found in {}", section, path.display());
    }

    Ok(output)
}

/// Render an outline as `<indent><hashes> Text (Lline)` per heading.
pub fn render_outline_text(headings: &[DocHeading]) -> String {
    let mut out = String::new();
    for h in headings {
        let indent = "  ".repeat(h.level.saturating_sub(1));
        let hashes = "#".repeat(h.level);
        out.push_str(&format!("{indent}{hashes} {} (L{})\n", h.text, h.line));
    }
    out
}

#[derive(Debug, Clone, Copy)]
struct Fence {
    ch: u8,
    len: usize,
}

fn detect_fence(trimmed: &str) -> Option<Fence> {
    let bytes = trimmed.as_bytes();
    let first = *bytes.first()?;
    if first != b'`' && first != b'~' {
        return None;
    }
    let len = bytes.iter().take_while(|&&b| b == first).count();
    if len >= 3 {
        Some(Fence { ch: first, len })
    } else {
        None
    }
}

/// Apply CommonMark-ish fence transitions:
/// - no current fence + any fence line  → open it
/// - current fence + same char and len ≥ open length → close it
/// - otherwise (nested fence of different shape) → ignore, stay open
fn update_fence(state: &mut Option<Fence>, seen: Fence) {
    match *state {
        None => *state = Some(seen),
        Some(open) if seen.ch == open.ch && seen.len >= open.len => *state = None,
        _ => {}
    }
}

fn heading_level(trimmed: &str) -> usize {
    let mut level = 0usize;
    for ch in trimmed.chars() {
        if ch == '#' {
            level += 1;
        } else {
            break;
        }
    }
    let bytes = trimmed.as_bytes();
    if level > 0 && level <= 6 && level < bytes.len() && bytes[level] == b' ' {
        level
    } else {
        0
    }
}

fn heading_text(trimmed: &str, level: usize) -> String {
    if level == 0 || level + 1 > trimmed.len() {
        return String::new();
    }
    trimmed[level + 1..].trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_md(dir: &TempDir, name: &str, body: &str) -> std::path::PathBuf {
        let path = dir.path().join(name);
        fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn outline_returns_levels_and_lines() {
        let dir = TempDir::new().unwrap();
        let p = write_md(
            &dir,
            "doc.md",
            "# Top\n\ntext\n\n## Sub A\n\nstuff\n\n## Sub B\n\n### Deep\n",
        );
        let headings = extract_headings(&p).unwrap();
        assert_eq!(headings.len(), 4);
        assert_eq!(headings[0].level, 1);
        assert_eq!(headings[0].text, "Top");
        assert_eq!(headings[0].line, 1);
        assert_eq!(headings[1].level, 2);
        assert_eq!(headings[1].text, "Sub A");
        assert_eq!(headings[3].level, 3);
        assert_eq!(headings[3].text, "Deep");
    }

    #[test]
    fn outline_skips_code_block_hashes() {
        let dir = TempDir::new().unwrap();
        let p = write_md(
            &dir,
            "doc.md",
            "# Real\n\n```bash\n# not a heading\n## also not\n```\n\n## Real Sub\n",
        );
        let headings = extract_headings(&p).unwrap();
        assert_eq!(headings.len(), 2);
        assert_eq!(headings[0].text, "Real");
        assert_eq!(headings[1].text, "Real Sub");
    }

    #[test]
    fn section_extracts_until_same_or_higher_heading() {
        let dir = TempDir::new().unwrap();
        let p = write_md(
            &dir,
            "doc.md",
            "# Top\n\nintro\n\n## Sub A\n\nA body\n\n### Deeper\n\nmore\n\n## Sub B\n\nB body\n",
        );
        let body = extract_section(&p, "Sub A").unwrap();
        assert!(body.contains("A body"));
        assert!(body.contains("### Deeper"));
        assert!(body.contains("more"));
        assert!(!body.contains("Sub B"));
        assert!(!body.contains("B body"));
    }

    #[test]
    fn section_match_is_case_insensitive() {
        let dir = TempDir::new().unwrap();
        let p = write_md(&dir, "doc.md", "## Installation\n\nrun this\n");
        let body = extract_section(&p, "installation").unwrap();
        assert!(body.contains("run this"));
    }

    #[test]
    fn section_missing_errors() {
        let dir = TempDir::new().unwrap();
        let p = write_md(&dir, "doc.md", "## Only\n\nx\n");
        let err = extract_section(&p, "Other").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn section_preserves_fenced_blocks() {
        let dir = TempDir::new().unwrap();
        let p = write_md(
            &dir,
            "doc.md",
            "## Usage\n\n```bash\n# step one\n## step two\n```\n\n## After\n",
        );
        let body = extract_section(&p, "Usage").unwrap();
        assert!(body.contains("# step one"));
        assert!(body.contains("## step two"));
        assert!(!body.contains("After"));
    }

    #[test]
    fn outline_handles_nested_fences() {
        // 4-backtick outer fence containing a 3-backtick inner block — the
        // inner ``` must NOT close the outer ````. Mirrors the README pattern
        // where docs embed example markdown that itself contains code blocks.
        let dir = TempDir::new().unwrap();
        let p = write_md(
            &dir,
            "doc.md",
            "# Real\n\
             \n\
             ````markdown\n\
             ## fake heading inside outer fence\n\
             \n\
             ```bash\n\
             # not a heading\n\
             ## also not\n\
             ```\n\
             \n\
             ## still inside outer fence\n\
             ````\n\
             \n\
             ## Real Sub\n",
        );
        let headings = extract_headings(&p).unwrap();
        let texts: Vec<&str> = headings.iter().map(|h| h.text.as_str()).collect();
        assert_eq!(texts, vec!["Real", "Real Sub"]);
    }

    #[test]
    fn render_outline_indents_by_level() {
        let headings = vec![
            DocHeading {
                level: 1,
                text: "Top".into(),
                line: 1,
            },
            DocHeading {
                level: 3,
                text: "Deep".into(),
                line: 5,
            },
        ];
        let text = render_outline_text(&headings);
        assert!(text.contains("# Top (L1)"));
        assert!(text.contains("    ### Deep (L5)"));
    }
}
