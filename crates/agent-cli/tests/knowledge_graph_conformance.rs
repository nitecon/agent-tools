use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const REQUIRED_ROWS: [&str; 12] = [
    "KG-A001", "KG-A002", "KG-A003", "KG-A004", "KG-A005", "KG-A006", "KG-A007", "KG-A008",
    "KG-A009", "KG-A010", "KG-A011", "KG-A012",
];

const REQUIRED_COLUMNS: [&str; 9] = [
    "Row ID",
    "Owner",
    "Operation",
    "Fixture",
    "Expected status",
    "Database / graph expectation",
    "Output / response expectation",
    "Compare",
    "Platform",
];

const REQUIRED_FIXTURES: [&str; 13] = [
    "knowledge_graph/code/",
    "knowledge_graph/code/rust/lib.rs",
    "knowledge_graph/markdown/architecture.md",
    "knowledge_graph/okf/v02/",
    "knowledge_graph/okf/v01/legacy.md",
    "knowledge_graph/migration/files-v0.sql",
    "knowledge_graph/migration/symbols-v0.sql",
    "knowledge_graph/gateways/default.json",
    "knowledge_graph/gateways/additional.json",
    "knowledge_graph/gateways/failure.json",
    "knowledge_graph/hostile/invalid-frontmatter.md",
    "knowledge_graph/hostile/attested-computation.md",
    "knowledge_graph/hostile/limits.md",
];

#[test]
fn knowledge_graph_matrix_has_stable_complete_rows() {
    let document = fs::read_to_string(conformance_document()).expect("read conformance matrix");
    let header = find_table_row(&document, "| Row ID |").expect("required matrix header");
    let columns = parse_cells(header);
    assert_eq!(columns, REQUIRED_COLUMNS, "required matrix columns drifted");

    let rows: Vec<Vec<String>> = document
        .lines()
        .filter(|line| line.starts_with("| `KG-A"))
        .map(parse_cells)
        .collect();
    let ids: Vec<&str> = rows.iter().map(|row| row[0].trim_matches('`')).collect();
    assert_eq!(ids, REQUIRED_ROWS, "scenario IDs or ordering drifted");

    for row in &rows {
        assert_eq!(row.len(), REQUIRED_COLUMNS.len(), "malformed row: {row:?}");
        assert!(
            row.iter().all(|cell| !cell.trim().is_empty()),
            "empty cell: {row:?}"
        );
        assert!(matches!(row[4].as_str(), "0" | "1" | "2" | "3"));
        assert!(matches!(
            row[7].as_str(),
            "byte-exact" | "graph-exact" | "normalized"
        ));
        assert!(matches!(row[8].as_str(), "all" | "platform"));
    }
}

#[test]
fn knowledge_graph_fixture_inventory_is_present_and_unique() {
    let document = fs::read_to_string(conformance_document()).expect("read conformance matrix");
    let fixture_root = fixture_root();
    let mut seen = BTreeSet::new();

    for fixture in REQUIRED_FIXTURES {
        assert!(
            seen.insert(fixture),
            "duplicate required fixture: {fixture}"
        );
        let path = fixture_root.join(fixture);
        assert!(
            path.exists(),
            "missing fixture declared by matrix: {}",
            path.display()
        );
        assert!(
            document.contains(&format!("`{fixture}`")),
            "fixture is not inventoried in conformance matrix: {fixture}"
        );
    }
}

#[test]
fn every_required_scenario_has_executable_ownership() {
    let document = fs::read_to_string(conformance_document()).expect("read conformance matrix");
    let ownership = document
        .split("## Executable Ownership")
        .nth(1)
        .expect("executable ownership section");

    for row in REQUIRED_ROWS {
        assert!(ownership.contains(row), "{row} has no executable owner");
    }
    assert!(ownership.contains("cargo test -p agent-cli --test knowledge_graph_conformance"));
}

#[test]
fn graph_cli_resolves_cross_file_imports_and_preserves_deterministic_locations() {
    let state = isolated_state_dir("resolved");
    let project = fixture_root().join("knowledge_graph/code");
    let indexed = run_agent_tools(&project, &state, &["index"]);
    assert!(indexed.status.success(), "{}", stderr(&indexed));
    assert!(stdout(&indexed).contains("14 resolved"));

    let graph = run_agent_tools(
        &project,
        &state,
        &[
            "graph",
            "app.py",
            "--relation",
            "imports",
            "--direction",
            "out",
        ],
    );
    assert!(graph.status.success(), "{}", stderr(&graph));
    let output = stdout(&graph);
    assert!(output.contains("out imports"));
    assert!(output.contains("python/worker.py [resolved] python/app.py:1"));
    assert!(!output.contains(" -> ?"));

    fs::remove_dir_all(state).expect("remove isolated state");
}

#[test]
fn graph_cli_rejects_ambiguous_resource_names_without_guessing() {
    let state = isolated_state_dir("ambiguous");
    let project = fixture_root().join("knowledge_graph/code");
    assert!(run_agent_tools(&project, &state, &["index"])
        .status
        .success());

    let graph = run_agent_tools(&project, &state, &["graph", "worker"]);
    assert!(!graph.status.success());
    let error = stderr(&graph);
    assert!(error.contains("is ambiguous"));
    assert!(error.contains("python/worker.py"));
    assert!(error.contains("rust/worker.rs"));

    fs::remove_dir_all(state).expect("remove isolated state");
}

#[test]
fn okf_import_search_and_get_share_canonical_resource_metadata() {
    let state = isolated_state_dir("okf-retrieval");
    let project = fixture_root().join("knowledge_graph");
    let imported = run_agent_tools(&project, &state, &["okf", "import", "okf/v02"]);
    assert!(imported.status.success(), "{}", stderr(&imported));
    assert!(stdout(&imported).contains("Imported 3 concepts"));

    let search = run_agent_tools(
        &project,
        &state,
        &[
            "search",
            "checkout",
            "--type",
            "knowledge",
            "--namespace",
            "okf",
            "--status",
            "stable",
        ],
    );
    assert!(search.status.success(), "{}", stderr(&search));
    assert!(stdout(&search).contains("services/service.md [stable:repository]"));

    let get = run_agent_tools(&project, &state, &["get", "Checkout Service"]);
    assert!(get.status.success(), "{}", stderr(&get));
    let output = stdout(&get);
    assert!(output.contains("\"authority\": \"repository\""));
    assert!(output.contains("\"verification_count\": 1"));
    assert!(output.contains("\"stale_after\": \"2030-01-01\""));

    fs::remove_dir_all(state).expect("remove isolated state");
}

#[test]
fn okf_publish_dry_run_is_gateway_free_and_deterministic() {
    let state = isolated_state_dir("okf-publish-dry-run");
    let project = fixture_root().join("knowledge_graph");
    let first = run_agent_tools(
        &project,
        &state,
        &["okf", "publish", "okf/v02", "--dry-run"],
    );
    let second = run_agent_tools(
        &project,
        &state,
        &["okf", "publish", "okf/v02", "--dry-run"],
    );
    assert!(first.status.success(), "{}", stderr(&first));
    assert_eq!(first.stdout, second.stdout);
    let output = stdout(&first);
    assert_eq!(output.lines().count(), 3);
    assert!(output
        .lines()
        .all(|line| line.contains("\"action\":\"create\"")));

    fs::remove_dir_all(state).expect("remove isolated state");
}

#[test]
fn prompt_hook_injects_bounded_local_knowledge_without_a_gateway() {
    let state = isolated_state_dir("hook-knowledge");
    let project = fixture_root().join("knowledge_graph");
    assert!(
        run_agent_tools(&project, &state, &["okf", "import", "okf/v02"])
            .status
            .success()
    );
    let output = run_agent_tools_with_input(
        &project,
        &state,
        &["hook", "user-prompt-submit", "--agent", "claude"],
        br#"{"prompt":"checkout recovery runbook"}"#,
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let context = envelope["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(context.contains("Relevant knowledge"));
    assert!(context.contains("authority=repository"));
    assert!(context.contains("trust="));
    assert!(context.contains("agent-tools get"));
    assert!(context.len() <= 3_000);

    fs::remove_dir_all(state).expect("remove isolated state");
}

#[test]
fn hostile_okf_validation_uses_stable_failure_status_and_never_runs_computation() {
    let state = isolated_state_dir("hostile-validation");
    let invalid_dir = isolated_state_dir("invalid-bundle");
    let attested_dir = isolated_state_dir("attested-bundle");
    fs::copy(
        fixture_root().join("knowledge_graph/hostile/invalid-frontmatter.md"),
        invalid_dir.join("invalid.md"),
    )
    .unwrap();
    fs::copy(
        fixture_root().join("knowledge_graph/hostile/attested-computation.md"),
        attested_dir.join("attested.md"),
    )
    .unwrap();
    let project = fixture_root().join("knowledge_graph");
    let invalid = run_agent_tools(
        &project,
        &state,
        &["okf", "validate", invalid_dir.to_str().unwrap()],
    );
    assert_eq!(invalid.status.code(), Some(2));
    assert!(stderr(&invalid).contains("missing required type"));

    let attested = run_agent_tools(
        &project,
        &state,
        &["okf", "validate", attested_dir.to_str().unwrap()],
    );
    assert!(attested.status.success(), "{}", stderr(&attested));
    assert!(stdout(&attested).contains("1 concepts"));

    fs::remove_dir_all(state).unwrap();
    fs::remove_dir_all(invalid_dir).unwrap();
    fs::remove_dir_all(attested_dir).unwrap();
}

fn parse_cells(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_owned())
        .collect()
}

fn find_table_row<'a>(document: &'a str, prefix: &str) -> Option<&'a str> {
    document.lines().find(|line| line.starts_with(prefix))
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("agent-cli belongs to workspace/crates")
        .to_path_buf()
}

fn conformance_document() -> PathBuf {
    workspace_root().join("docs/knowledge-graph-okf-conformance.md")
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn isolated_state_dir(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "agent-tools-knowledge-graph-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("create isolated state");
    path
}

fn run_agent_tools(project: &Path, state: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_agent-tools"))
        .args(args)
        .current_dir(project)
        .env("AGENT_TOOLS_STATE_DIR", state)
        .env("AGENT_TOOLS_NUDGE", "off")
        .output()
        .expect("run agent-tools")
}

fn run_agent_tools_with_input(project: &Path, state: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_agent-tools"))
        .args(args)
        .current_dir(project)
        .env("AGENT_TOOLS_STATE_DIR", state)
        .env("AGENT_TOOLS_NUDGE", "off")
        .env("AGENT_TOOLS_HOOK_TIMEOUT_MS", "25")
        .env_remove("GATEWAY_URL")
        .env_remove("GATEWAY_API_KEY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn agent-tools");
    child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(input)
        .expect("write hook input");
    child.wait_with_output().expect("wait for agent-tools")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
