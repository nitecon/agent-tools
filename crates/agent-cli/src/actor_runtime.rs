//! Register the invocation's native conversation independently of terminal membership.

use agent_comms::actor::{self, Actor};
use anyhow::{bail, ensure, Context, Result};
use std::path::Path;

#[derive(Default)]
struct NativeIds {
    codex_thread: Option<String>,
    codex_session: Option<String>,
    claude_session: Option<String>,
}

impl NativeIds {
    fn from_env() -> Result<Self> {
        fn read(name: &str) -> Result<Option<String>> {
            match std::env::var(name) {
                Ok(value) => Ok(Some(value)),
                Err(std::env::VarError::NotPresent) => Ok(None),
                Err(_) => bail!("non-Unicode provider-native identity"),
            }
        }
        Ok(Self {
            codex_thread: read("CODEX_THREAD_ID")?,
            codex_session: read("CODEX_SESSION_ID")?,
            claude_session: read("CLAUDE_CODE_SESSION_ID")?,
        })
    }

    fn for_provider(&self, provider: &str) -> Result<Option<String>> {
        match provider {
            "codex" => {
                let thread = self
                    .codex_thread
                    .as_deref()
                    .map(actor::normalize_native_id)
                    .transpose()?;
                let session = self
                    .codex_session
                    .as_deref()
                    .map(actor::normalize_native_id)
                    .transpose()?;
                if let (Some(thread), Some(session)) = (&thread, &session) {
                    ensure!(thread == session, "conflicting Codex native session IDs");
                }
                Ok(thread.or(session))
            }
            "claude" => self
                .claude_session
                .as_deref()
                .map(actor::normalize_native_id)
                .transpose(),
            _ => bail!("unsupported provider-native identity"),
        }
    }

    #[cfg(test)]
    fn conversation(&self) -> Result<Option<(&'static str, String)>> {
        self.conversation_with_ancestors(std::iter::empty())
    }

    fn conversation_with_ancestors(
        &self,
        ancestors: impl IntoIterator<Item = Result<Self>>,
    ) -> Result<Option<(&'static str, String)>> {
        match (self.for_provider("codex")?, self.for_provider("claude")?) {
            (Some(codex), Some(claude)) => {
                // Native tools can inherit an enclosing provider's ID. Select only
                // the ID introduced at a strictly nearer environment boundary.
                // A shared executor introducing both IDs remains ambiguous.
                for ancestor in ancestors {
                    let ancestor = ancestor?;
                    match (
                        ancestor.for_provider("codex")?.as_ref() == Some(&codex),
                        ancestor.for_provider("claude")?.as_ref() == Some(&claude),
                    ) {
                        (true, true) => continue,
                        (false, true) => return Ok(Some(("codex", codex))),
                        (true, false) => return Ok(Some(("claude", claude))),
                        (false, false) => break,
                    }
                }
                bail!("ambiguous provider-native conversation context")
            }
            (Some(native), None) => Ok(Some(("codex", native))),
            (None, Some(native)) => Ok(Some(("claude", native))),
            (None, None) => Ok(None),
        }
    }
}

#[cfg(target_os = "linux")]
fn native_ancestors() -> impl Iterator<Item = Result<NativeIds>> {
    let mut pid = std::process::id();
    (0..256).map_while(move |_| {
        if pid <= 1 {
            return None;
        }
        let result: Result<NativeIds> = (|| {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))?;
            // comm may contain spaces and parentheses; fields after its final ')'
            // start with state and ppid.
            pid = stat
                .rsplit_once(')')
                .and_then(|(_, fields)| fields.split_whitespace().nth(1))
                .context("native ancestor parent unavailable")?
                .parse()?;
            let environment = std::fs::read(format!("/proc/{pid}/environ"))?;
            let read = |name: &[u8]| -> Result<Option<String>> {
                environment
                    .split(|byte| *byte == 0)
                    .find_map(|entry| entry.strip_prefix(name))
                    .map(|value| {
                        String::from_utf8(value.to_vec())
                            .context("non-Unicode ancestor native identity")
                    })
                    .transpose()
            };
            Ok(NativeIds {
                codex_thread: read(b"CODEX_THREAD_ID=")?,
                codex_session: read(b"CODEX_SESSION_ID=")?,
                claude_session: read(b"CLAUDE_CODE_SESSION_ID=")?,
            })
        })();
        Some(result.context("ambiguous provider-native conversation context: read native ancestry"))
    })
}

#[cfg(not(target_os = "linux"))]
fn native_ancestors() -> impl Iterator<Item = Result<NativeIds>> {
    std::iter::empty()
}

fn git_value(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = std::str::from_utf8(&output.stdout).ok()?.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn register(provider: &str, native: &str) -> Result<Actor> {
    let cwd = std::env::current_dir()?;
    let root = git_value(&cwd, &["rev-parse", "--show-toplevel"])
        .map(std::path::PathBuf::from)
        .unwrap_or(cwd)
        .canonicalize()
        .context("canonical actor project path")?;
    let project = root
        .to_str()
        .context("non-Unicode actor project path")?
        .to_owned();
    #[cfg(windows)]
    let project = project.replace('\\', "/");
    #[cfg(windows)]
    let project = project
        .strip_prefix("//?/")
        .unwrap_or(&project)
        .to_ascii_lowercase();
    let remote = git_value(&root, &["remote", "get-url", "origin"])
        .map(|remote| {
            let normalized = agent_core::storage::normalize_git_url(&remote);
            let normalized = normalized.trim_end_matches('/');
            normalized
                .strip_suffix(".git")
                .unwrap_or(normalized)
                .to_owned()
        })
        .unwrap_or_default();
    let instance = actor::instance_id()?;
    let os = std::env::consts::OS;
    let base = actor::base_id(instance, os, provider, &project, &remote)?;
    actor::register(instance, os, provider, native, base)
}

pub(crate) fn current() -> Result<Option<Actor>> {
    NativeIds::from_env()?
        .conversation_with_ancestors(native_ancestors())?
        .map(|(provider, native)| register(provider, &native))
        .transpose()
}

pub(crate) fn from_hook(provider: &str, native_id: &str) -> Result<Actor> {
    let native = actor::normalize_native_id(native_id)?;
    if let Some(inherited) = NativeIds::from_env()?.for_provider(provider)? {
        ensure!(
            native == inherited,
            "hook native ID conflicts with invocation context"
        );
    }
    register(provider, &native)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_metadata_must_select_exactly_one_conversation() {
        let mut ids = NativeIds::default();
        assert!(ids.conversation().unwrap().is_none());
        ids.codex_thread = Some("thread-a".into());
        assert_eq!(
            ids.conversation().unwrap(),
            Some(("codex", "thread-a".into()))
        );
        ids.codex_session = Some("thread-b".into());
        assert!(ids.conversation().is_err());
        ids.codex_session = Some("thread-a".into());
        ids.claude_session = Some("claude-c".into());
        assert!(ids.conversation().is_err());
        assert_eq!(ids.for_provider("codex").unwrap(), Some("thread-a".into()));
        assert_eq!(ids.for_provider("claude").unwrap(), Some("claude-c".into()));
    }

    #[test]
    fn nested_native_context_requires_a_strictly_nearer_boundary() {
        let mixed = || NativeIds {
            codex_thread: Some("codex-parent".into()),
            claude_session: Some("claude-child".into()),
            ..Default::default()
        };
        let claude = mixed()
            .conversation_with_ancestors([
                Ok(mixed()), // tool shell
                Ok(NativeIds {
                    codex_thread: Some("codex-parent".into()),
                    ..Default::default()
                }), // Claude process inherited Codex, before Claude tool injection
            ])
            .unwrap();
        assert_eq!(claude, Some(("claude", "claude-child".into())));
        let codex = mixed()
            .conversation_with_ancestors([Ok(NativeIds {
                claude_session: Some("claude-child".into()),
                codex_thread: Some("older-codex".into()),
                ..Default::default()
            })])
            .unwrap();
        assert_eq!(codex, Some(("codex", "codex-parent".into())));

        // Both injected at the same boundary, no boundary, or inaccessible
        // evidence must never pick a provider based on a fixed preference.
        assert!(mixed()
            .conversation_with_ancestors([Ok(NativeIds::default())])
            .is_err());
        assert!(mixed().conversation_with_ancestors([Ok(mixed())]).is_err());
        assert!(mixed()
            .conversation_with_ancestors([Err(anyhow::anyhow!("unreadable"))])
            .is_err());
        let mut conflicting = mixed();
        conflicting.codex_session = Some("other-codex".into());
        assert!(conflicting
            .conversation_with_ancestors([Ok(NativeIds::default())])
            .is_err());
    }

    #[test]
    fn registration_child_fixture() {
        let Ok(output) = std::env::var("AGENT_TOOLS_REGISTRATION_OUTPUT") else {
            return;
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        if std::env::var_os("AGENT_TOOLS_REGISTRATION_REJECTED").is_some() {
            assert!(current().is_err());
            assert!(rt.block_on(crate::cmd_session::mutation_origin()).is_err());
            std::fs::write(output, "rejected").unwrap();
            return;
        }
        let actor = current().unwrap().unwrap();
        assert_eq!(current().unwrap().unwrap(), actor);
        assert_eq!(
            rt.block_on(crate::cmd_session::mutation_origin())
                .unwrap()
                .unwrap(),
            actor.origin
        );
        assert_eq!(
            from_hook(&actor.origin.provider, &actor.provider_session_id).unwrap(),
            actor
        );
        assert!(from_hook(&actor.origin.provider, "different-conversation").is_err());
        std::fs::write(output, serde_json::to_vec(&actor).unwrap()).unwrap();
    }

    #[test]
    fn ordinary_subprocesses_register_isolated_conversations_and_reuse_after_restart() {
        let directory = tempfile::tempdir().unwrap();
        let repository = directory.path().join("repo");
        let nested = repository.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        assert!(std::process::Command::new("git")
            .args(["init", "--quiet"])
            .arg(&repository)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .current_dir(&repository)
            .args(["remote", "add", "origin", "git@github.com:org/repo.git"])
            .status()
            .unwrap()
            .success());
        let worker = std::env::current_exe().unwrap();
        let mut runs = Vec::new();
        for (index, (provider, native)) in [
            ("codex", "thread-a"),
            ("codex", "thread-b"),
            ("codex", "thread-a"),
            ("claude", "thread-a"),
        ]
        .into_iter()
        .enumerate()
        {
            let output = directory.path().join(format!("actor-{index}.json"));
            let mut command = if cfg!(windows) {
                let mut command = std::process::Command::new("powershell.exe");
                command.args(["-NoProfile", "-NonInteractive", "-Command",
                    "& $env:AGENT_TOOLS_REGISTRATION_WORKER --exact actor_runtime::tests::registration_child_fixture --nocapture; exit $LASTEXITCODE"]);
                command
            } else {
                let mut command = std::process::Command::new(&worker);
                command.args([
                    "--exact",
                    "actor_runtime::tests::registration_child_fixture",
                    "--nocapture",
                ]);
                command
            };
            command
                .current_dir(if index == 2 { &nested } else { &repository })
                .env("AGENT_TOOLS_REGISTRATION_WORKER", &worker)
                .env("AGENT_TOOLS_REGISTRATION_OUTPUT", &output)
                .env("HOME", directory.path())
                .env("USERPROFILE", directory.path())
                .env_remove("CODEX_THREAD_ID")
                .env_remove("CODEX_SESSION_ID")
                .env_remove("CLAUDE_CODE_SESSION_ID")
                .env_remove("CMUX_SOCKET")
                .env_remove("CMUX_SOCKET_PATH")
                .env_remove("AMUX_CLI_PATH");
            // A rejecting amux CLI (absent/stale context) must not revoke provenance.
            #[cfg(unix)]
            if index == 1 {
                use std::os::unix::fs::PermissionsExt;
                let amux = directory.path().join("amux");
                std::fs::write(&amux, "#!/bin/sh\nexit 1\n").unwrap();
                std::fs::set_permissions(&amux, std::fs::Permissions::from_mode(0o700)).unwrap();
                command.env("AMUX_CLI_PATH", &amux);
            }
            command.env(
                if provider == "codex" {
                    "CODEX_THREAD_ID"
                } else {
                    "CLAUDE_CODE_SESSION_ID"
                },
                native,
            );
            assert!(command.status().unwrap().success());
            let actor: Actor = serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
            assert_eq!(actor.version, 2);
            runs.push(actor);
        }
        assert_eq!(runs[0], runs[2]);
        assert_eq!(runs[0].base_id, runs[1].base_id);
        assert_ne!(runs[0].session_slot, runs[1].session_slot);
        assert_ne!(runs[0].origin.session_id, runs[1].origin.session_id);
        assert_ne!(runs[0].base_id, runs[3].base_id);
        assert_ne!(runs[0].origin.session_id, runs[3].origin.session_id);
    }
}
