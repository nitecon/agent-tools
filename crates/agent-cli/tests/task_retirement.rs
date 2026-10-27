use std::process::Command;

const MIGRATION: &str =
    "All task capabilities have migrated to amux; see https://github.com/nitecon/amux";

fn workspace_temp() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap()
}

#[test]
fn retired_cli_paths_fail_without_creating_local_state_or_changing_permissions() {
    let root = workspace_temp();
    let home = root.path().join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    let settings = home.join(".claude/settings.json");
    let original = r#"{"permissions":{"deny":["TaskCreate","UserOwnedDeny"]},"custom":true}"#;
    std::fs::write(&settings, original).unwrap();
    let cases: &[&[&str]] = &[
        &["tasks", "list"],
        &["tasks", "get", "id"],
        &["tasks", "add", "--title", "Title"],
        &[
            "tasks",
            "add-delegated",
            "--target-project",
            "other",
            "--title",
            "Title",
            "--description",
            "d",
            "--specification",
            "s",
        ],
        &["tasks", "claim", "id"],
        &["tasks", "release", "id"],
        &["tasks", "done", "id"],
        &["tasks", "comment", "id", "body"],
        &["tasks", "rank", "id", "1"],
        &["tasks", "builds"],
        &["setup", "perms"],
        &["setup", "perms", "--remove"],
        &["specs", "generate-tasks", "id", "--confirmed"],
        &["specs", "link-task", "id", "item", "task"],
    ];
    for args in cases {
        let output = Command::new(env!("CARGO_BIN_EXE_agent-tools"))
            .args(*args)
            .current_dir(root.path())
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("CODEX_HOME", home.join(".codex"))
            .output()
            .unwrap();
        assert!(!output.status.success(), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains(MIGRATION), "{args:?}: {stderr}");
        assert!(!stderr.contains("Tracking work?"));
    }
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), original);
    assert!(!home.join(".agentic").exists());
    assert!(!home.join(".codex").exists());
    assert!(!settings.with_extension("json.bak").exists());
}

#[test]
fn regenerated_provider_rules_preserve_other_blocks_and_are_idempotent() {
    let root = workspace_temp();
    let manual = "# Manual instructions\nKeep this.\n";
    let amux = "<!-- amux:tasks:start -->\nAmux owns tasks.\n<!-- amux:tasks:end -->\n";
    let memory = "<memory-rules>\nKeep memory guidance.\n</memory-rules>\n";
    for file in [
        "CLAUDE.md",
        "AGENTS.md",
        "GEMINI.md",
        "copilot-instructions.md",
    ] {
        let path = root.path().join(file);
        std::fs::write(&path, format!("{manual}<agent-tools-rules>\n### Tasks\nagent-tools tasks list\n</agent-tools-rules>\n{amux}{memory}")).unwrap();
        let run = || {
            Command::new(env!("CARGO_BIN_EXE_agent-tools"))
                .args(["setup", "rules", "--target"])
                .arg(&path)
                .output()
                .unwrap()
        };
        assert!(run().status.success());
        let once = std::fs::read_to_string(&path).unwrap();
        assert!(once.starts_with(manual));
        assert!(once.ends_with(&format!("{amux}{memory}")));
        assert!(once.contains("agent-tools symbols"));
        assert!(once.contains("agent-tools doc outline"));
        assert!(!once.contains("agent-tools tasks"));
        assert!(!once.contains("tasks done"));
        assert!(run().status.success());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), once);
    }
}
