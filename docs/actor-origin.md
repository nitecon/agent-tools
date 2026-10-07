# Provider actor origin

`agent-tools-actor-v1` identifies the actual provider invocation independently
of CMUX terminal membership. It is staged on `main`, pending integrated release.

## UUID inputs

`instance_id` is a random UUIDv4 published atomically once at
`~/.agentic/agent-tools/actor-instance-id`. Concurrent first callers read the
same complete value. Corrupt or unreadable identities reject attribution.
This namespace is separate from the display agent ID and CMUX transport ID.

`session_id` is RFC UUIDv5 with that instance UUID as namespace and the UTF-8
bytes of this compact JSON array as its name:

```json
["agent-tools-actor-v1", "os", "provider", "native_session_id", ["executor_generation"]]
```

The displayed spaces above are explanatory; hashed JSON has no insignificant
whitespace or trailing newline. Fields retain exactly this order. UUID strings
are lowercase canonical hyphenated text. Ordinary JSON string escaping applies.

| Field | Value |
| --- | --- |
| OS | `linux`, `macos`, or `windows` |
| Provider | `codex` or `claude` |
| Native session | 1–256 printable ASCII bytes with no whitespace; UUID IDs normalize to canonical lowercase, other IDs remain byte-for-byte |
| Linux executor | `["linux-proc-v1", boot_id, pid, start_ticks]`, raw `/proc/PID/stat` field 22 |
| Windows executor | `["windows-process-v1", pid, creation_FILETIME]`, raw `GetProcessTimes` u64 |
| macOS executor | `["macos-proc-v1", pid, start_microseconds]`, `proc_bsdinfo` seconds × 1,000,000 + microseconds |

Numeric generation fields are decimal strings without leading zeroes. Linux
boot ID is the canonical UUID from `/proc/sys/kernel/random/boot_id`. Repository,
cwd, terminal, socket and CMUX instance never enter the actor hash. The
[reference vectors](actor-origin-vectors.json) include exact names and UTF-8 bytes.

## Runtime verification

Codex direct tools use their own `CODEX_THREAD_ID` and/or `CODEX_SESSION_ID`;
both must agree when present. Claude uses `CLAUDE_CODE_SESSION_ID`. Hooks use
the installed provider and payload `session_id`, checked against any present
same-provider environment. Other-provider environment cannot select an executor.

The CLI inspects only its own bounded ancestry (64 processes, two seconds),
recognizes native provider executables or Claude's official Node entrypoint,
and rechecks executable, command, parent and precise creation generation.
Codex requires positive `app-server`, `exec`/`e`, `review`, bare `--no-daemon`,
or `resume`/`fork` with `--no-daemon` execution evidence. Other subcommands
reject even with `--no-daemon`; help/version and missing option values reject
too. App-server schema/proxy/daemon subcommands are rejected. A recognized daemon-connected frontend or official
Codex Node launcher stops verification; the CLI cannot cross that boundary to
select an older ancestor after backend exit. Normal shared-daemon launches
resolve the actual `app-server` automatically; no user flags are required.
Claude native versions under `.local/share/claude/versions` are recognized too.
No ancestor supplies a thread ID or surface ID. Dead, replaced, reparented or
unverifiable runtime context fails closed when native identity is present. No
native provider context retains legacy machine attribution.

Same native conversation and live executor retain one actor across terminal
reattachment. Replacing the executor or starting another native conversation
changes the actor. Supported paths are direct provider CLI and hook subprocesses;
remote, detached or long-lived MCP paths without per-invocation native context
are not assigned guessed identity. Platform adapters require platform CI.

## Optional CMUX membership

Gateway mutations keep exactly the four origin fields `session_id`,
`instance_id`, `provider`, and `os`. CMUX RPC absence or rejection never changes
a correctly derived actor. CMUX owns peer validation and terminal membership.

`gateway.session.announce` accepts `{version:1, origin, provider_session_id,
executor_generation, repository?, enrollment_token?}`. A call without a token
requests registration/capability only. `gateway.session.resolve` uses the same
fields without a token. Success echoes origin, native ID and generation with
`binding_state` (`unbound` or `bound`); bound context adds `surface_id`,
`workspace_id`, and `recipient_session_id`. Mismatched echoes reject membership.
CMUX enables bootstrap only after observing an actual `agent-tools hook`
SessionStart/UserPromptSubmit process through its kernel peer. An ordinary CLI
announcement cannot prove installed, enabled hooks or grant enrollment readiness.
UserPromptSubmit also makes this private announcement when its first prompt is a
gateway or harness notification, then returns without any context output.

The existing UserPromptSubmit hook consumes only a whole dedicated prompt:

```text
<cmux-session-enrollment>{"version":1,"enrollment_token":"64 lowercase hex characters"}</cmux-session-enrollment>
```

It derives identity from its actual hook session, announces before notification
filtering, and exits successfully with a blocking decision even if enrollment
fails. Normal user prompts remain fail-soft. Claude additionally suppresses the
original prompt in its block message. Provider transcript/history may retain the
prompt; the client never logs, caches, or includes its bearer token in context.
CMUX observes binding success and owns safe-idle retries and readiness gating.

Linux discovery uses an absolute, private current-user `XDG_RUNTIME_DIR`,
otherwise `/run/user/<realuid>`, followed by `cmux/cmux.sock` and an owner-only
bounded `cmux/last-socket-path` marker. Inherited `CMUX_SOCKET`/`CMUX_SOCKET_PATH`
are endpoint hints only. Windows uses `\\.\pipe\cmux-<current-user-SID>-control`.
This Linux CMUX release advertises no macOS bootstrap endpoint. Native Unix and
Windows transport have a two-second overall deadline and 64 KiB response bound.
