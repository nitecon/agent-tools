# Changelog

## v1.20.1

- Recognize the confirmed Windows Codex 0.161 bare native launch from the
  OpenAI installation path when deriving automatic actor identity. This fixes
  session and task mutations rejected before transport for that executor.
- Keep utility, Node launcher and arbitrary-path frontend rejection, precise
  process generation and bounded ancestry rechecks. Linux/macOS executor
  recognition and the actor UUID contract are unchanged.
- Cover PowerShell and standalone code-mode-host descendants with native
  Windows CI fixtures, including hook/tool parity, mutation identity, distinct
  conversations and executor replacement. Live Windows acceptance is separate.

## v1.20.0 (prepared; publication pending CMUX acceptance)

- Derive automatic task provenance from the calling Codex or Claude native
  session and verified executor generation, including distinct threads under
  one shared daemon. Provenance no longer depends on inherited CMUX context.
- Persist a separate, atomically published machine actor namespace. Preserve
  actor identity across directory changes and terminal reattachment; executor
  replacement changes it. Publish the exact UUID contract and reference vectors.
- Require positive Codex executor roles and stop at frontend, utility and Node
  launcher boundaries. Reject conflicting native IDs and unverifiable runtimes
  before sending task mutations; plain shells retain legacy attribution.
- Add optional, independently verified CMUX actor membership and hook-consumed
  enrollment. Gateway notifications privately announce enabled hook capability
  before returning without context output, including the first delegated prompt.
- Validate actor derivation, task headers, hook enrollment and notification-first
  announcement with Linux, macOS and Windows CI process fixtures.

CMUX Linux v0.6.5 supplies the coordinated terminal enrollment and delivery side;
this client release remains held until its integrated packages are accepted.
The older Windows CMUX preview lacks actor membership: valid provider task
mutations no longer require that API, but verified terminal binding and exact
actor self-echo suppression are unavailable there. No manual environment
workaround supplies membership. Windows runtime fixtures passed CI; live preview
acceptance is separate. This Linux CMUX release advertises no macOS bootstrap.
Updates remain manual, and no existing preview installation is changed by this
release preparation.

## v1.19.0

- Automatically attach exact CMUX session, instance, provider and OS provenance
  to task mutations, while retaining the existing machine identity.
- Add `agent-tools session [--peers] [--json]` for verified local agent discovery
  without requiring gateway configuration.
- Display task ownership and comment provenance using gateway `owner_origin`.
- Include independent-agent coordination guidance in owned rule/skill templates.
- Document harness environment preservation and discovery from actual shell tools.
- Keep the pinned glibc 2.31 release baseline without unrelated package upgrades.

Upgrade CMUX Linux to v0.6.4 and restart it before updating this client. Current
Windows CMUX previews must retain agent-tools v1.18.0 until a session-capable
preview is available. Explicit but unavailable CMUX identity blocks task
mutations; outside CMUX, existing legacy behavior continues. Updates remain
manual.
