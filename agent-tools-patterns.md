# Agent Tools Pattern Library Requirements

## Purpose

`agent-tools` should expose the gateway global pattern library as a first-class
CLI surface. Patterns are organization-wide markdown documents that describe how
we do things. They are not project-local tasks, and they are not memory entries.

## Gateway API

All endpoints require the same bearer token used by the existing gateway API.

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/v1/patterns?q=<query>&label=<label>&version=<version>&state=<state>&superseded_by=<id-or-slug>` | List or search pattern summaries. Search covers title, slug, summary, body, labels, version, and state. Filters are exact-match and can be combined with `q`. |
| `POST` | `/v1/patterns` | Create a pattern. |
| `GET` | `/v1/patterns/:id` | Fetch one pattern by id or slug, without comments. |
| `PATCH` | `/v1/patterns/:id` | Update pattern metadata or markdown body. |
| `DELETE` | `/v1/patterns/:id` | Delete a pattern. |
| `GET` | `/v1/patterns/:id/comments` | Fetch comments for one pattern. |
| `POST` | `/v1/patterns/:id/comments` | Add a comment to one pattern. |

Pattern create body:

```json
{
  "title": "Deploying Eventic Applications",
  "slug": "deploying-eventic-applications",
  "summary": "How we use main and tag deploys for independent sites.",
  "body": "# Deploying Eventic Applications\n\n...",
  "labels": ["eventic", "deploy"],
  "version": "draft",
  "state": "active",
  "author": "agent-id"
}
```

Pattern response shape:

```json
{
  "id": "uuid-v7",
  "title": "Deploying Eventic Applications",
  "slug": "deploying-eventic-applications",
  "summary": "How we use main and tag deploys for independent sites.",
  "body": "# Deploying Eventic Applications\n\n...",
  "labels": ["eventic", "deploy"],
  "version": "draft",
  "state": "active",
  "author": "agent-id",
  "created_at": 1777130000000,
  "updated_at": 1777130000000
}
```

List/search response shape is an array of summaries. Summaries omit `body` and
include `comment_count`.

`version` is lifecycle metadata, not semantic versioning. Allowed values are:

- `draft`: proposed or still being worked through.
- `latest`: current recommended practice.
- `superseded`: retained for historical discovery but not recommended.

`state` is required free-form lifecycle metadata. For active patterns use a
short state such as `active`. For superseded patterns, use
`superseded-by:<id-or-slug>` so agents can follow the replacement.

`labels` are topical tags used for search and filtering, such as `linux`,
`systemd`, `services`, `eventic`, `deploy`, or `encryption`.

Structured list filters:

- `q`: broad text search across title, slug, summary, body, labels, version,
  and state.
- `label`: exact topical tag match, for example `label=systemd`.
- `version`: exact lifecycle match; must be `draft`, `latest`, or
  `superseded`.
- `state`: exact state match, for example `state=active`.
- `superseded_by`: convenience filter for `state=superseded-by:<id-or-slug>`.

Comments are intentionally not included in `GET /v1/patterns/:id`. Agents should
only fetch comments when the user explicitly asks to address or review comments.

## CLI Surface

Recommended commands:

```bash
agent-tools patterns list
agent-tools patterns search "<query>" [--label x] [--version latest] [--state active] [--superseded-by slug]
agent-tools patterns get <id-or-slug>
agent-tools patterns create --title "..." --version draft --state active [--slug "..."] [--label x] [--summary "..."] --body-file path.md
agent-tools patterns update <id-or-slug> [--title "..."] [--version latest] [--state "superseded-by:..."] [--slug "..."] [--label x] [--summary "..."] [--body-file path.md]
agent-tools patterns delete <id-or-slug>
agent-tools patterns comments <id-or-slug>
agent-tools patterns comment <id-or-slug> "<markdown comment>"
agent-tools patterns check
agent-tools patterns use <id-or-slug> [--path path]...
```

`get` must print only the pattern document and metadata. It must not fetch or
display comments.

This separation is important because comments are collaboration state, not
approved guidance. A pattern can have unresolved review notes, proposed edits,
or user discussion that should not be mixed into the normal context an agent
uses to perform work. Pulling comments by default would make agents more likely
to treat pending debate as current practice, increase token usage on every
lookup, and make old comment threads unexpectedly affect unrelated tasks.
Comments are opt-in so an agent only loads them when the user is explicitly
asking to review or resolve that discussion.

`comments` should call `GET /v1/patterns/:id/comments` and print the thread.

`comment` should call `POST /v1/patterns/:id/comments` with:

```json
{
  "content": "...",
  "author": "<agent id>",
  "author_type": "agent"
}
```

## Agent Behavior

Agents should use patterns as durable global guidance. They should search the
pattern library when the task appears to involve an established organizational
practice, such as deployment, encryption, secrets handling, project setup,
frontend conventions, release workflows, or incident response.

When asked to validate repository pattern usage, `agent-tools` checks only
`$PWD/.patterns`. This mirrors the existing project-identity behavior: agents
are expected to run from the project directory they are operating on, and the
tool does not walk parent directories looking for a patterns file.

`.patterns` is intentionally sparse public repository metadata. It stores
gateway pattern ids, not pattern titles or body text, so public users without
gateway access do not receive sensitive organizational guidance. Generated
files must not include comments or explanatory prose.

Preferred form:

```yaml
<gateway-pattern-id>:
  - src/main.rs
  - /etc/someapp/config.py
```

For a pattern with no specific paths recorded, a bare id line is accepted:

```text
<gateway-pattern-id>
```

Agents should use `agent-tools patterns use <id-or-slug> --path <path>` after
they apply a pattern so the canonical gateway pattern id is recorded in
`.patterns`. If `.patterns` lists a superseded pattern, `agent-tools patterns
check` should create a gateway task on the current project to migrate to the
replacement and tell the agent which task was created or already existed.

When multiple matching patterns exist, agents should prefer `version=latest`.
If an otherwise relevant pattern has `version=superseded`, agents should inspect
`state` for a `superseded-by:<id-or-slug>` pointer and fetch that replacement.
Draft patterns can inform discussion, but should not override latest patterns
unless the user explicitly asks to work on draft guidance.

Agents should not treat pattern comments as part of the normal guidance pull.
Comments are review/collaboration material and should be fetched only when the
user says comments exist or asks to address them.

Pattern iteration is the main exception to the no-comments-by-default rule. If
the user says they want to iterate on, revise, update, or edit a specific
pattern, agents should fetch both the pattern body and its comments before
writing the update:

```bash
agent-tools patterns get <id-or-slug>
agent-tools patterns comments <id-or-slug>
```

The comments are review context for that requested edit, not standalone
approved guidance. Agents should apply the user's requested changes to a local
markdown draft, preserve unrelated sections unless the user asks to change them,
then update the gateway pattern with:

```bash
agent-tools patterns update <id-or-slug> --body-file <draft.md>
```
