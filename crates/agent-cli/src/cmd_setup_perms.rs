//! Retired task permission setup. Existing settings, including amux denies,
//! are preserved. Native task settings are now managed by amux.
use anyhow::Result;

pub fn run(_remove: bool, _dry_run: bool, _print: bool) -> Result<()> {
    anyhow::bail!(agent_comms::tasks::TASK_MIGRATION)
}
