//! CI-only provider process fixture with genuine role argv and native ancestry.
use std::process::{exit, Command};

fn main() {
    if let Some(hook) = std::env::var_os("AGENT_TOOLS_RUNTIME_FIXTURE_HOOK") {
        let status = Command::new(hook)
            .args(["hook", "user-prompt-submit", "--agent"])
            .arg(std::env::var_os("AGENT_TOOLS_RUNTIME_FIXTURE_PROVIDER").unwrap())
            .stdin(std::process::Stdio::inherit())
            .status()
            .unwrap();
        exit(status.code().unwrap_or(1));
    }
    // A rejected provider boundary beneath a live executor must stop resolution,
    // rather than allowing the worker to fall back to that older executor.
    let mut command =
        if let Some(boundary) = std::env::var_os("AGENT_TOOLS_RUNTIME_FIXTURE_BOUNDARY") {
            let mut command = Command::new(boundary);
            let argument = std::env::var_os("AGENT_TOOLS_RUNTIME_FIXTURE_BOUNDARY_ARG").unwrap();
            if !argument.is_empty() {
                command.arg(argument);
            }
            command.env_remove("AGENT_TOOLS_RUNTIME_FIXTURE_BOUNDARY");
            command
        } else if let Some(shell) = std::env::var_os("AGENT_TOOLS_RUNTIME_FIXTURE_SHELL") {
            let mut command = Command::new(shell);
            command.args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "& $env:AGENT_TOOLS_RUNTIME_FIXTURE_WORKER --exact actor_runtime::tests::runtime_child_fixture --nocapture; exit $LASTEXITCODE",
            ]);
            command
        } else {
            let mut command =
                Command::new(std::env::var_os("AGENT_TOOLS_RUNTIME_FIXTURE_WORKER").unwrap());
            command.args([
                "--exact",
                "actor_runtime::tests::runtime_child_fixture",
                "--nocapture",
            ]);
            command
        };
    exit(command.status().unwrap().code().unwrap_or(1));
}
