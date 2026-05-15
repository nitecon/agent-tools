use anyhow::{bail, Context, Result};
use std::path::Path;

pub(crate) fn run(file: &Path, lines: Option<&str>) -> Result<()> {
    let text = std::fs::read_to_string(file)
        .with_context(|| format!("failed to read UTF-8 file {}", file.display()))?;
    match lines {
        Some(raw) => print!("{}", select_lines(&text, parse_lines(raw)?)),
        None => print!("{text}"),
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LineRange {
    start: usize,
    end: Option<usize>,
}

fn parse_lines(raw: &str) -> Result<LineRange> {
    let raw = raw.trim();
    if raw.is_empty() {
        bail!("--lines must be N, START:END, START:, :END, or START,END");
    }

    let (start_raw, end_raw) = if let Some((start, end)) = raw.split_once(':') {
        (start, Some(end))
    } else if let Some((start, end)) = raw.split_once(',') {
        (start, Some(end))
    } else {
        (raw, Some(raw))
    };

    let start = if start_raw.is_empty() {
        1
    } else {
        parse_one_based_line(start_raw, "--lines start")?
    };
    let end = match end_raw {
        Some("$") | Some("") => None,
        Some(value) => Some(parse_one_based_line(value, "--lines end")?),
        None => None,
    };

    if let Some(end) = end {
        if end < start {
            bail!("--lines end is before start");
        }
    }

    Ok(LineRange { start, end })
}

fn parse_one_based_line(raw: &str, label: &str) -> Result<usize> {
    match raw.parse::<usize>() {
        Ok(0) => bail!("{label} must be one-based"),
        Ok(value) => Ok(value),
        Err(_) => bail!("{label} must be a non-negative integer"),
    }
}

fn select_lines(text: &str, range: LineRange) -> String {
    text.split_inclusive('\n')
        .enumerate()
        .filter_map(|(idx, line)| {
            let line_no = idx + 1;
            if line_no < range.start {
                return None;
            }
            if range.end.is_some_and(|end| line_no > end) {
                return None;
            }
            Some(line)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_parser_accepts_single_ranges_and_open_ends() {
        assert_eq!(
            parse_lines("3").unwrap(),
            LineRange {
                start: 3,
                end: Some(3),
            }
        );
        assert_eq!(
            parse_lines("2:4").unwrap(),
            LineRange {
                start: 2,
                end: Some(4),
            }
        );
        assert_eq!(
            parse_lines("2,4").unwrap(),
            LineRange {
                start: 2,
                end: Some(4),
            }
        );
        assert_eq!(
            parse_lines(":2").unwrap(),
            LineRange {
                start: 1,
                end: Some(2),
            }
        );
        assert_eq!(
            parse_lines("2:").unwrap(),
            LineRange {
                start: 2,
                end: None,
            }
        );
        assert!(parse_lines("0").is_err());
        assert!(parse_lines("4:2").is_err());
    }

    #[test]
    fn line_selection_preserves_existing_line_endings() {
        let text = "one\r\ntwo\nthree";
        assert_eq!(
            select_lines(
                text,
                LineRange {
                    start: 1,
                    end: Some(2),
                }
            ),
            "one\r\ntwo\n"
        );
        assert_eq!(
            select_lines(
                text,
                LineRange {
                    start: 3,
                    end: Some(3),
                }
            ),
            "three"
        );
    }
}
