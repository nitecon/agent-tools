//! Shared glue for bare `agent-tools setup` (interactive checklist) and
//! `agent-tools setup all` (non-interactive sweep).
//!
//! The components are intentionally executed in a fixed order:
//!   1. gateway  — supplies the URL/key that rules+comms+tasks all depend on
//!   2. rules    — injects the mandatory-protocols block into rule files
//!   3. skill    — installs the Claude Code skill (independent of gateway)
//!   4. perms    — denies native Task* so agents are forced onto the board
//!
//! Putting rules before skill keeps humans reading a freshly-updated
//! CLAUDE.md able to see the new rules ahead of the model discovering the
//! skill. Perms is intentionally last so a partial failure still leaves the
//! positive instructions (rules+skill) in place without the blocking denies.

use crate::{cmd_setup_perms, cmd_setup_rules, cmd_setup_skill};
use agent_comms::config::{load_config, user_gateway_conf_path};
use anyhow::{Context, Result};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Component {
    Gateway,
    Rules,
    Skill,
    Perms,
}

impl Component {
    pub const ALL: [Component; 4] = [
        Component::Gateway,
        Component::Rules,
        Component::Skill,
        Component::Perms,
    ];

    fn label(self) -> &'static str {
        match self {
            Component::Gateway => "Gateway",
            Component::Rules => "Rules",
            Component::Skill => "Skill",
            Component::Perms => "Perms",
        }
    }
}

/// Entry point for `agent-tools setup` with no subcommand — shows a checklist
/// of the four components, their current install state, and lets the user
/// pick which ones to run.
pub fn run_interactive() -> Result<()> {
    let states: Vec<(Component, ComponentState)> =
        Component::ALL.iter().map(|c| (*c, probe(*c))).collect();

    println!("agent-tools setup — choose components to install:");
    println!();
    for (i, (comp, state)) in states.iter().enumerate() {
        let check = if state.installed { "x" } else { " " };
        println!("  {}) [{check}] {} — {}", i + 1, comp.label(), state.detail);
    }
    println!();
    println!("Select components to (re)install:");
    println!("  a     = all");
    println!("  1,3   = specific (comma-separated indices)");
    println!("  c     = cancel");
    print!("> ");
    io::stdout().flush().context("flush stdout")?;

    let mut input = String::new();
    io::stdin()
        .lock()
        .read_line(&mut input)
        .context("read selection")?;
    let chosen = parse_selection(input.trim(), &Component::ALL)?;

    if chosen.is_empty() {
        println!("Cancelled — nothing changed.");
        return Ok(());
    }

    run_components(&chosen)
}

/// Entry point for `agent-tools setup all`. `assume_yes` suppresses the
/// confirmation prompt; useful for scripted installs.
pub fn run_all(assume_yes: bool) -> Result<()> {
    if !assume_yes {
        println!("Will install: gateway, rules, skill, perms.");
        print!("Proceed? [y/N]: ");
        io::stdout().flush().context("flush stdout")?;
        let mut input = String::new();
        io::stdin()
            .lock()
            .read_line(&mut input)
            .context("read confirmation")?;
        if !matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            println!("Cancelled.");
            return Ok(());
        }
    }
    run_components(&Component::ALL)
}

// -- internals ---------------------------------------------------------------

struct ComponentState {
    installed: bool,
    detail: String,
}

fn probe(c: Component) -> ComponentState {
    match c {
        Component::Gateway => probe_gateway(),
        Component::Rules => probe_rules(),
        Component::Skill => probe_skill(),
        Component::Perms => probe_perms(),
    }
}

fn probe_gateway() -> ComponentState {
    let cfg = load_config();
    match (cfg.gateway.url.as_ref(), cfg.gateway.api_key.as_ref()) {
        (Some(url), Some(_)) => ComponentState {
            installed: true,
            detail: format!("configured at {url}"),
        },
        _ => ComponentState {
            installed: false,
            detail: format!(
                "not configured ({} missing)",
                user_gateway_conf_path().display()
            ),
        },
    }
}

fn probe_rules() -> ComponentState {
    let targets = detect_rule_files();
    if targets.is_empty() {
        return ComponentState {
            installed: false,
            detail: "no agent rule files detected".into(),
        };
    }
    let any_with_block = targets.iter().any(|p| file_has_rules_block(p));
    if any_with_block {
        let with_block: Vec<String> = targets
            .iter()
            .filter(|p| file_has_rules_block(p))
            .map(|p| p.display().to_string())
            .collect();
        ComponentState {
            installed: true,
            detail: format!("injected in {}", with_block.join(", ")),
        }
    } else {
        ComponentState {
            installed: false,
            detail: format!(
                "not injected in {}",
                targets
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

fn probe_skill() -> ComponentState {
    if cmd_setup_skill::is_installed() {
        ComponentState {
            installed: true,
            detail: format!("installed at {}", cmd_setup_skill::skill_path().display()),
        }
    } else {
        ComponentState {
            installed: false,
            detail: format!(
                "not installed ({} missing)",
                cmd_setup_skill::skill_path().display()
            ),
        }
    }
}

fn probe_perms() -> ComponentState {
    if cmd_setup_perms::is_fully_installed() {
        ComponentState {
            installed: true,
            detail: format!(
                "Task* denies present in {}",
                cmd_setup_perms::settings_path().display()
            ),
        }
    } else {
        ComponentState {
            installed: false,
            detail: format!(
                "Task* denies missing from {}",
                cmd_setup_perms::settings_path().display()
            ),
        }
    }
}

fn detect_rule_files() -> Vec<PathBuf> {
    let home = agent_comms::config::home_dir();
    [
        home.join(".claude").join("CLAUDE.md"),
        home.join(".gemini").join("GEMINI.md"),
        home.join(".codex").join("AGENTS.md"),
        home.join(".config").join("codex").join("AGENTS.md"),
    ]
    .into_iter()
    .filter(|p| p.exists())
    .collect()
}

fn file_has_rules_block(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|s| s.contains("<agent-tools-rules>") && s.contains("</agent-tools-rules>"))
        .unwrap_or(false)
}

fn run_components(components: &[Component]) -> Result<()> {
    let mut any_failed = false;
    for c in components {
        println!();
        println!("=== {} ===", c.label());
        let result = match c {
            Component::Gateway => agent_comms::config::run_setup_gateway(),
            Component::Rules => cmd_setup_rules::run(None, true, false, false),
            Component::Skill => cmd_setup_skill::run(false, false),
            Component::Perms => cmd_setup_perms::run(false, false, false),
        };
        if let Err(e) = result {
            eprintln!("{} failed: {e:#}", c.label());
            any_failed = true;
        }
    }
    if any_failed {
        anyhow::bail!("one or more components failed");
    }
    println!();
    println!("Done.");
    Ok(())
}

/// Parse the freeform selection string. Supports `a`/`all`, `c`/`cancel`, or
/// a comma-separated list of 1-based indices. Out-of-range indices or
/// unparseable tokens bail with a clear error.
fn parse_selection(input: &str, all: &[Component]) -> Result<Vec<Component>> {
    let s = input.trim().to_ascii_lowercase();
    if s.is_empty() || s == "c" || s == "cancel" {
        return Ok(vec![]);
    }
    if s == "a" || s == "all" {
        return Ok(all.to_vec());
    }
    let mut picked = Vec::new();
    for tok in s.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        let idx: usize = tok
            .parse()
            .with_context(|| format!("invalid selection token: {tok:?}"))?;
        if idx < 1 || idx > all.len() {
            anyhow::bail!("selection out of range: {idx}");
        }
        let comp = all[idx - 1];
        if !picked.contains(&comp) {
            picked.push(comp);
        }
    }
    Ok(picked)
}

// -- tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_selection_cancel_variants() {
        assert_eq!(parse_selection("", &Component::ALL).unwrap(), vec![]);
        assert_eq!(parse_selection("c", &Component::ALL).unwrap(), vec![]);
        assert_eq!(parse_selection("cancel", &Component::ALL).unwrap(), vec![]);
    }

    #[test]
    fn parse_selection_all() {
        assert_eq!(
            parse_selection("a", &Component::ALL).unwrap(),
            Component::ALL.to_vec()
        );
        assert_eq!(
            parse_selection("ALL", &Component::ALL).unwrap(),
            Component::ALL.to_vec()
        );
    }

    #[test]
    fn parse_selection_specific_indices() {
        let r = parse_selection("1,3", &Component::ALL).unwrap();
        assert_eq!(r, vec![Component::Gateway, Component::Skill]);
    }

    #[test]
    fn parse_selection_dedupes() {
        let r = parse_selection("2,2,4,2", &Component::ALL).unwrap();
        assert_eq!(r, vec![Component::Rules, Component::Perms]);
    }

    #[test]
    fn parse_selection_handles_whitespace() {
        let r = parse_selection("  1 , 4  ", &Component::ALL).unwrap();
        assert_eq!(r, vec![Component::Gateway, Component::Perms]);
    }

    #[test]
    fn parse_selection_rejects_out_of_range() {
        assert!(parse_selection("5", &Component::ALL).is_err());
        assert!(parse_selection("0", &Component::ALL).is_err());
    }

    #[test]
    fn parse_selection_rejects_garbage() {
        assert!(parse_selection("foo", &Component::ALL).is_err());
        assert!(parse_selection("1,bar", &Component::ALL).is_err());
    }

    #[test]
    fn component_all_order_matches_execution_contract() {
        // Pin the order so future contributors don't accidentally reorder and
        // end up installing perms before the rules that explain them.
        assert_eq!(
            Component::ALL,
            [
                Component::Gateway,
                Component::Rules,
                Component::Skill,
                Component::Perms
            ]
        );
    }
}
