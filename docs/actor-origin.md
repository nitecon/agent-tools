# Provider actor origin

`agent-tools-actor-v2` registers the calling native conversation locally once
and reuses that registration for ordinary task calls and provider hooks.
Task identity does not require provider process ancestry, executable paths,
launch flags, or CMUX membership. Terminal delivery generation is CMUX-owned.

## UUID inputs

The existing persisted random UUIDv4 at
`~/.agentic/agent-tools/actor-instance-id` is the machine-local namespace/salt.
It remains separate from the display agent ID and CMUX transport identity.
Concurrent first callers atomically publish one complete value. No new user
configuration or manual identity input is required.

`base_id` is RFC UUIDv5 with that instance UUID as namespace and UTF-8 compact
JSON array name bytes, in exactly this order:

```json
["agent-tools-actor-v2","base","os","canonical_project_path","git_identity","provider"]
```

The project path is the canonical Git top-level directory, or canonical current
directory outside Git. Linux/macOS preserve its exact UTF-8 path. Windows
replaces backslashes with `/`, removes a leading `//?/` extended path prefix,
and folds ASCII letters to lowercase. Git identity is the normalized `origin`
repository URL: remove HTTP(S)/SSH scheme and leading user, normalize SSH
shorthand colon to slash, remove trailing slash and a final `.git` suffix.
An absent Git remote contributes the empty string. Provider is `codex` or
`claude`; OS is `linux`, `windows` or `macos`.

`origin.session_id` is RFC UUIDv5 with `base_id` as namespace and UTF-8 compact
JSON array name bytes:

```json
["agent-tools-actor-v2","normalized_native_conversation_id"]
```

JSON has no insignificant whitespace, BOM or trailing newline. UUIDs are
lowercase canonical hyphenated strings. Native IDs contain 1–256 printable
ASCII bytes without whitespace; UUID IDs normalize to lowercase canonical
form, other IDs remain byte-for-byte. Ordinary JSON string escaping applies.
The [reference vectors](actor-origin-vectors.json) include both exact JSON names.

The gateway origin retains exactly `session_id`, `instance_id`, `provider` and
`os`. A different project, Git identity, provider, OS or machine namespace
changes the base. A different native conversation changes its session UUID.
Executor restart/reconnection, terminal reattachment and slot renumbering do
not change a conversation UUID. V2 changes identities once from generation-based
v1; release or finish old-origin task claims legitimately before changing the
installed client. There is no fallback that impersonates an older owner.

## Automatic registration

Codex direct tools use their own `CODEX_THREAD_ID` and/or `CODEX_SESSION_ID`;
both must agree when present. Claude uses `CLAUDE_CODE_SESSION_ID`. Direct
calls with both providers' native contexts reject as ambiguous. Provider hooks
use their installed provider and payload `session_id`, checked against present
same-provider native IDs. No project/provider lookup selects a peer conversation.
Calls without native conversation metadata retain existing plain-shell behavior.

The local registry is under the native OS temporary directory at
`agent-tools-actors/<instance_id>/<base_id>/`. Each positive numeric slot file
contains its conversation session UUID. Atomic no-clobber publication assigns
the first available slot, including concurrent first callers; later calls with
the same UUID reuse it. Slots are limited to 1–65535. The readable local identity
is `<base_id>-<session_slot>`; it is display/registration metadata, not a gateway
credential. Deleting temporary files may renumber slots but cannot make another
native conversation reuse an earlier gateway UUID. Corrupt registration files
reject rather than selecting a peer. No daemon, service or assignment policy
is involved in local registration.

SessionStart and UserPromptSubmit hooks register/announce automatically.
Ordinary calls reuse the same registration and announce it when CMUX is
available. CMUX absence, an older API or unbound membership cannot revoke task
identity. Native platform CI covers registration concurrency, separate same-
provider conversations, reconnect, temp reset, hooks and task origin headers;
live Windows hardware acceptance remains separate.

## Optional CMUX membership

`gateway.session.announce` accepts `{version:2, origin, provider_session_id,
base_id, session_slot, repository?, enrollment_token?}`.
`gateway.session.resolve` uses the same fields without an enrollment token.
There is no executor generation in v2 SDK registration. Success echoes origin,
native conversation ID, base and slot with `binding_state` (`unbound` or `bound`).
Bound context adds `surface_id`, `workspace_id` and `recipient_session_id`.
Mismatched echoes reject membership without changing gateway provenance.
CMUX must associate the exact native conversation with a unique live terminal;
project/provider names alone never establish binding. CMUX independently retains
its process/TUI generation fences for stale terminal delivery.

Existing provider hooks consume only a whole dedicated enrollment prompt:

```text
<cmux-session-enrollment>{"version":1,"enrollment_token":"64 lowercase hex characters"}</cmux-session-enrollment>
```

This envelope version belongs to enrollment, separately from actor registration.
The hook uses its own registration, announces before notification filtering,
and exits with a blocking decision even if enrollment fails. Normal user prompts
remain fail-soft. Claude suppresses the original prompt in its block message.
The client never logs, caches or puts a bearer token in model context. Registration
and ordinary task calls require no enrollment token.

Linux discovery uses an absolute private current-user `XDG_RUNTIME_DIR`, otherwise
`/run/user/<realuid>`, followed by `cmux/cmux.sock`, with a bounded owner-only
`cmux/last-socket-path` marker. Inherited `CMUX_SOCKET`/`CMUX_SOCKET_PATH` are
endpoint hints only. Windows uses `\\.\pipe\cmux-<current-user-SID>-control`.
Native transports have a two-second overall deadline and 64 KiB response bound.
Matching CMUX v2 integration must be validated and distributable before the SDK
public rollout; old clients can use existing CMUX v1 support during that rollout.
