//! CI-only provider process fixture with genuine role argv and native ancestry.
use std::process::{exit, Command};

fn main() {
    // A rejected provider boundary beneath a live executor must stop resolution,
    // rather than allowing the worker to fall back to that older executor.
    let mut command =
        if let Some(boundary) = std::env::var_os("AGENT_TOOLS_RUNTIME_FIXTURE_BOUNDARY") {
            let mut command = Command::new(boundary);
            command.arg(std::env::var_os("AGENT_TOOLS_RUNTIME_FIXTURE_BOUNDARY_ARG").unwrap());
            command.env_remove("AGENT_TOOLS_RUNTIME_FIXTURE_BOUNDARY");
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
