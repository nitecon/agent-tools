//! Provider-native invocation identity, independent of terminal membership.
//! Only the current command's bounded ancestry is inspected; no provider is run.

use agent_comms::actor::{self, Actor};
use anyhow::{bail, ensure, Context, Result};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Process {
    pid: u32,
    parent: u32,
    executable: PathBuf,
    command: Vec<String>,
    generation: Vec<String>,
}

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

    fn any(&self) -> bool {
        self.codex_thread.is_some() || self.codex_session.is_some() || self.claude_session.is_some()
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
}

pub(crate) fn current() -> Result<Option<Actor>> {
    let ids = NativeIds::from_env()?;
    if !ids.any() {
        return Ok(None);
    }
    let (provider, generation) = executor()?;
    let native = ids
        .for_provider(provider)?
        .context("verified executor lacks its own native session ID")?;
    Ok(Some(actor::derive(
        actor::instance_id()?,
        std::env::consts::OS,
        provider,
        &native,
        generation,
    )?))
}

pub(crate) fn from_hook(provider: &str, native_id: &str) -> Result<Actor> {
    let native = actor::normalize_native_id(native_id)?;
    let ids = NativeIds::from_env()?;
    if let Some(inherited) = ids.for_provider(provider)? {
        ensure!(
            native == inherited,
            "hook native ID conflicts with invocation context"
        );
    }
    let (actual_provider, generation) = executor()?;
    ensure!(
        actual_provider == provider,
        "hook provider does not match its executor"
    );
    actor::derive(
        actor::instance_id()?,
        std::env::consts::OS,
        provider,
        &native,
        generation,
    )
}

fn provider(process: &Process) -> Option<&'static str> {
    let executable = process
        .executable
        .file_name()?
        .to_str()?
        .to_ascii_lowercase();
    let stem = executable.strip_suffix(".exe").unwrap_or(&executable);
    let args = &process.command;
    match stem {
        "codex" | "codex-x86_64-unknown-linux-musl" | "codex-aarch64-unknown-linux-musl" => {
            // Helpers are not agent executors. Continue to the actual parent.
            if args.get(1).is_some_and(|arg| {
                matches!(
                    arg.as_str(),
                    "exec-server" | "mcp-server" | "sandbox" | "debug" | "login" | "logout"
                )
            }) {
                None
            } else {
                Some("codex")
            }
        }
        "claude" => Some("claude"),
        "node" | "nodejs" | "bun" => {
            let entry = args.get(1)?.replace('\\', "/");
            if entry.ends_with("/@anthropic-ai/claude-code/cli.js") {
                Some("claude")
            } else {
                // The official Codex JS launcher is a wrapper: its native child
                // is the executor, so never bind to the Node launcher instead.
                None
            }
        }
        _ => None,
    }
}

fn executor() -> Result<(&'static str, Vec<String>)> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut pid = std::process::id();
    let mut lineage: Vec<Process> = Vec::new();
    for _ in 0..64 {
        ensure!(
            Instant::now() < deadline,
            "provider runtime verification timed out"
        );
        ensure!(
            pid != 0 && !lineage.iter().any(|p| p.pid == pid),
            "unverified provider ancestry"
        );
        let process = snapshot(pid)?;
        if let Some(child) = lineage.last() {
            // Windows can retain an exited parent's PID. Reject a reused PID
            // whose process creation is newer than its purported child.
            let child_start: u64 = child.generation.last().unwrap().parse()?;
            let parent_start: u64 = process.generation.last().unwrap().parse()?;
            ensure!(parent_start <= child_start, "replaced provider ancestor");
        }
        let found = provider(&process);
        pid = process.parent;
        lineage.push(process);
        if let Some(provider) = found {
            for expected in &lineage {
                ensure!(
                    Instant::now() < deadline,
                    "provider runtime verification timed out"
                );
                ensure!(
                    snapshot(expected.pid)? == *expected,
                    "provider ancestry changed during identity verification"
                );
            }
            return Ok((provider, lineage.last().unwrap().generation.clone()));
        }
    }
    bail!("no verified provider executor in invocation ancestry")
}

fn snapshot(pid: u32) -> Result<Process> {
    let observed_generation = generation(pid)?;
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[Pid::from_u32(pid)]),
        true,
        ProcessRefreshKind::nothing()
            .without_tasks()
            .with_exe(UpdateKind::Always)
            .with_cmd(UpdateKind::Always),
    );
    let process = system
        .process(Pid::from_u32(pid))
        .context("provider ancestor disappeared")?;
    let executable = process
        .exe()
        .context("provider ancestor executable unavailable")?
        .to_path_buf();
    let command: Vec<String> = process
        .cmd()
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    ensure!(
        !command.is_empty(),
        "provider ancestor invocation unavailable"
    );
    let parent = process.parent().map(|p| p.as_u32()).unwrap_or(0);
    ensure!(
        generation(pid)? == observed_generation,
        "provider process generation changed"
    );
    Ok(Process {
        pid,
        parent,
        executable,
        command,
        generation: observed_generation,
    })
}

#[cfg(target_os = "linux")]
fn generation(pid: u32) -> Result<Vec<String>> {
    let boot = std::fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let end = stat.rfind(')').context("invalid process stat")?;
    let fields: Vec<_> = stat[end + 1..].split_whitespace().collect();
    ensure!(
        fields
            .first()
            .is_some_and(|state| !matches!(*state, "Z" | "X")),
        "provider process exited"
    );
    let ticks: u64 = fields
        .get(19)
        .context("missing process start ticks")?
        .parse()?;
    Ok(vec![
        "linux-proc-v1".into(),
        boot.trim().to_ascii_lowercase(),
        pid.to_string(),
        ticks.to_string(),
    ])
}

#[cfg(target_os = "macos")]
fn generation(pid: u32) -> Result<Vec<String>> {
    // SAFETY: proc_bsdinfo is a C POD and the buffer/size match Apple's API.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of_val(&info) as i32;
    let read = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        )
    };
    ensure!(
        read == size && info.pbi_pid == pid && info.pbi_status != 5,
        "unverified macOS process generation"
    );
    let start = info
        .pbi_start_tvsec
        .checked_mul(1_000_000)
        .and_then(|sec| sec.checked_add(info.pbi_start_tvusec))
        .context("invalid process creation time")?;
    Ok(vec![
        "macos-proc-v1".into(),
        pid.to_string(),
        start.to_string(),
    ])
}

#[cfg(windows)]
fn generation(pid: u32) -> Result<Vec<String>> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, FILETIME},
        System::Threading::{
            GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        },
    };
    // SAFETY: the handle is checked, output buffers are valid, and it is closed
    // before any result is propagated. No supplied process handle is trusted.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    ensure!(!handle.is_null(), "provider process handle unavailable");
    let mut creation = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut exit = creation;
    let mut kernel = creation;
    let mut user = creation;
    let mut status = 0;
    let valid = unsafe {
        GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) != 0
            && GetExitCodeProcess(handle, &mut status) != 0
            && status == 259
    };
    unsafe { CloseHandle(handle) };
    ensure!(valid, "unverified Windows process generation");
    let start = (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime);
    Ok(vec![
        "windows-process-v1".into(),
        pid.to_string(),
        start.to_string(),
    ])
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn generation(_: u32) -> Result<Vec<String>> {
    bail!("provider runtime verification unsupported on this OS")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Separate processes exercise real native ancestry and hook/tool parity.
    // The copied test executable models a shared provider executor; no provider
    // installation, authentication, or production test bypass is required.
    #[test]
    fn runtime_child_fixture() {
        let Ok(output) = std::env::var("AGENT_TOOLS_RUNTIME_FIXTURE_OUTPUT") else {
            return;
        };
        if std::env::var("AGENT_TOOLS_RUNTIME_FIXTURE_MODE").as_deref() == Ok("worker") {
            let actor = current().unwrap().unwrap();
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            assert_eq!(
                rt.block_on(crate::cmd_session::mutation_origin())
                    .unwrap()
                    .unwrap(),
                actor.origin
            );
            assert_eq!(
                actor,
                from_hook("codex", &actor.provider_session_id).unwrap()
            );
            assert!(from_hook("codex", "another-thread").is_err());
            assert!(from_hook("claude", &actor.provider_session_id).is_err());
            std::fs::write(output, serde_json::to_vec(&actor).unwrap()).unwrap();
            // This child runs exactly this one test; its environment changes
            // cannot race other tests or alter the parent executor's context.
            std::env::set_var("CODEX_SESSION_ID", "conflicting-native-id");
            assert!(rt.block_on(crate::cmd_session::mutation_origin()).is_err());
            std::env::remove_var("CODEX_THREAD_ID");
            std::env::remove_var("CODEX_SESSION_ID");
            assert!(rt
                .block_on(crate::cmd_session::mutation_origin())
                .unwrap()
                .is_none());
            return;
        }
        let executable = std::env::var_os("AGENT_TOOLS_RUNTIME_FIXTURE_WORKER").unwrap();
        let mut actors = Vec::new();
        for native in ["fixture-thread-a", "fixture-thread-b"] {
            let child_output = format!("{output}.{native}");
            let status = std::process::Command::new(&executable)
                .args([
                    "--exact",
                    "actor_runtime::tests::runtime_child_fixture",
                    "--nocapture",
                ])
                .env("AGENT_TOOLS_RUNTIME_FIXTURE_MODE", "worker")
                .env("AGENT_TOOLS_RUNTIME_FIXTURE_OUTPUT", &child_output)
                .env("CODEX_THREAD_ID", native)
                .env("CODEX_SESSION_ID", native)
                .env_remove("CLAUDE_CODE_SESSION_ID")
                .env(
                    "CMUX_SURFACE_ID",
                    "inherited-daemon-surface-must-not-select-actor",
                )
                .env("CMUX_SOCKET", "/missing/legacy/socket")
                .status()
                .unwrap();
            assert!(status.success());
            let actor: Actor =
                serde_json::from_slice(&std::fs::read(child_output).unwrap()).unwrap();
            actors.push(actor);
        }
        std::fs::write(output, serde_json::to_vec(&actors).unwrap()).unwrap();
    }

    #[test]
    fn shared_executor_threads_are_isolated_and_replacement_changes_actor() {
        let directory = tempfile::tempdir().unwrap();
        let worker = std::env::current_exe().unwrap();
        let executor = directory
            .path()
            .join(if cfg!(windows) { "codex.exe" } else { "codex" });
        std::fs::copy(&worker, &executor).unwrap();
        let mut runs = Vec::new();
        for run in 0..2 {
            let output = directory.path().join(format!("actors-{run}.json"));
            let status = std::process::Command::new(&executor)
                .args([
                    "--exact",
                    "actor_runtime::tests::runtime_child_fixture",
                    "--nocapture",
                ])
                .env("AGENT_TOOLS_RUNTIME_FIXTURE_MODE", "executor")
                .env("AGENT_TOOLS_RUNTIME_FIXTURE_OUTPUT", &output)
                .env("AGENT_TOOLS_RUNTIME_FIXTURE_WORKER", &worker)
                .status()
                .unwrap();
            assert!(status.success());
            let actors: Vec<Actor> =
                serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
            assert_eq!(actors[0].executor_generation, actors[1].executor_generation);
            assert_eq!(actors[0].origin.instance_id, actors[1].origin.instance_id);
            assert_ne!(actors[0].origin.session_id, actors[1].origin.session_id);
            runs.push(actors);
        }
        assert_eq!(
            runs[0][0].provider_session_id,
            runs[1][0].provider_session_id
        );
        assert_eq!(runs[0][0].origin.instance_id, runs[1][0].origin.instance_id);
        assert_ne!(
            runs[0][0].executor_generation,
            runs[1][0].executor_generation
        );
        assert_ne!(runs[0][0].origin.session_id, runs[1][0].origin.session_id);
    }

    #[test]
    fn native_context_conflicts_fail_and_other_provider_context_does_not_select_actor() {
        let ids = NativeIds {
            codex_thread: Some("thread-a".into()),
            codex_session: Some("thread-b".into()),
            claude_session: Some("claude-c".into()),
        };
        assert!(ids.for_provider("codex").is_err());
        assert_eq!(ids.for_provider("claude").unwrap().unwrap(), "claude-c");
    }

    #[test]
    fn helper_and_launcher_are_not_the_executor() {
        let mut process = Process {
            pid: 1,
            parent: 0,
            executable: "/package/bin/codex".into(),
            command: vec!["codex".into(), "app-server".into()],
            generation: vec![],
        };
        assert_eq!(provider(&process), Some("codex"));
        process.command[1] = "exec-server".into();
        assert_eq!(provider(&process), None);
        process.executable = "node".into();
        process.command[1] = "/pkg/@openai/codex/bin/codex.js".into();
        assert_eq!(provider(&process), None);
        process.command[1] = "/pkg/@anthropic-ai/claude-code/cli.js".into();
        assert_eq!(provider(&process), Some("claude"));
    }

    #[test]
    fn native_process_snapshot_is_precise_and_stable() {
        let a = snapshot(std::process::id()).unwrap();
        let b = snapshot(std::process::id()).unwrap();
        assert_eq!(a, b);
        assert_ne!(a.parent, a.pid);
        assert!(a.generation.last().unwrap().parse::<u64>().unwrap() > 0);
    }
}
