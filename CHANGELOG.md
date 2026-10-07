# Changelog

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
