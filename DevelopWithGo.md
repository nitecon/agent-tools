[← Back to Docs Index](./README.md)

# Develop With Go — Authoritative Guide

> **Provenance & Authority**
>
> This document is the authoritative reference for building Go applications with the **ndesign** frontend system. It merges 10+ years of production Go patterns with the ndesign HTML-only frontend contract: a Go binary serves HTML, JSON, and streaming endpoints; ndesign hydrates the page via `data-nd-*` attributes — no build step on the frontend, no React, no Redux. The companion canonical reference for the ndesign runtime is at `https://storage.googleapis.com/ndesign-cdn/ndesign/v<semver>/SPEC.md` (pinned, immutable). When this doc and the ndesign spec disagree on runtime detail, **the spec wins** — this guide focuses on the integration contract between Go and ndesign.
>
> **Pattern catalog.** Each numbered section in this document is also published as a discrete `agent-tools` pattern keyed by the slug listed beside it in the table of contents and in the HTML comment immediately under each heading (`<!-- pattern-slug: <slug> -->`). A future agent that loads a single pattern body must be able to apply that pattern correctly without reading the rest of the document; sections are therefore self-contained, with `### Scope` declaring what the pattern covers and `### See also` listing related pattern slugs. Cross-references use slug names (not section numbers) so they survive extraction.

## Table of Contents

- [How AI Agents Should Use This Guide](#how-ai-agents-should-use-this-guide) — `go-agent-operating-manual`
- [Mandatory Rules](#mandatory-rules) — `go-mandatory-rules`
- [1. Project Layout](#1-project-layout) — `go-project-layout`
- [2. Entry Point & Application Lifecycle](#2-entry-point--application-lifecycle) — `go-entry-point-lifecycle`
- [3. Logging — zerolog](#3-logging--zerolog) — `go-logging-zerolog`
- [4. Observability — OpenTelemetry](#4-observability--opentelemetry) — `go-observability-otel`
- [5. Router & Middleware — chi](#5-router--middleware--chi) — `go-router-chi`
- [6. Authorization — RBAC](#6-authorization--rbac) — `go-rbac-middleware`
- [7. Response Conventions — The ndesign Contract](#7-response-conventions--the-ndesign-contract) — `go-response-envelope`
- [8. WebSocket System](#8-websocket-system) — `go-websocket-system`
- [9. WebSocket JWT Auth](#9-websocket-jwt-auth) — `go-websocket-jwt-auth`
- [10. Queue System](#10-queue-system) — `go-queue-system`
- [11. Cache Layer — BigCache + singleflight](#11-cache-layer--bigcache--singleflight) — `go-cache-singleflight`
- [12. Configuration — Two Tier](#12-configuration--two-tier) — `go-config-two-tier`
- [13. Data Layer & Repository Pattern](#13-data-layer--repository-pattern) — `go-repository-pattern`
- [14. Prometheus Metrics — 4 Golden Signals](#14-prometheus-metrics--4-golden-signals) — `go-prometheus-metrics`
- [15. The `dry/` Package](#15-the-dry-package) — `go-dry-package`
- [16. Frontend Architecture (ndesign)](#16-frontend-architecture-ndesign) — `ndesign-architecture`
- [17. The Three Canonical Layouts](#17-the-three-canonical-layouts) — `ndesign-layouts`
- [18. Page Composition & Store Setup](#18-page-composition--store-setup) — `ndesign-page-composition`
- [19. Data Binding Contract](#19-data-binding-contract) — `ndesign-data-binding`
- [20. Forms](#20-forms) — `ndesign-forms`
- [21. Validation — Go Tags as Source of Truth](#21-validation--go-tags-as-source-of-truth) — `ndesign-validation`
- [22. WebSocket Integration (`data-nd-ws`)](#22-websocket-integration-data-nd-ws) — `ndesign-websocket-integration`
- [23. SSE Integration (`data-nd-sse`)](#23-sse-integration-data-nd-sse) — `ndesign-sse-integration`
- [24. Templates & Rendering Primitives](#24-templates--rendering-primitives) — `ndesign-templates`
- [25. Dev Workflow — air](#25-dev-workflow--air) — `fullstack-dev-workflow-air`
- [26. Front-to-Back Symmetry](#26-front-to-back-symmetry) — `fullstack-symmetry-table`
- [Appendix A: Architectural Principles — Quick Reference](#appendix-a-architectural-principles--quick-reference)
- [Appendix B: Source Files Reference](#appendix-b-source-files-reference)

---

## How AI Agents Should Use This Guide
<!-- pattern-slug: go-agent-operating-manual -->

### Scope

This pattern is the operating manual every AI coding agent (Claude, Codex, Gemini, or any other) MUST apply when creating, reviewing, or modifying a Go application built around ndesign. It defines defaults that hold unless the user explicitly overrides them. It does NOT cover the implementation details of any single subsystem — those live in the per-subsystem patterns referenced below.

### Directives

This guide is both a pattern catalog and an implementation contract. When an AI coding agent creates, reviews, or changes a Go application in this style, it MUST apply the rules here as defaults unless the user explicitly asks for a different architecture.

The agent MUST:

- **Build a single Go service** that owns process lifecycle, dependency wiring, HTML rendering, JSON APIs, streaming endpoints, observability, and graceful shutdown. One binary, three responsibilities (HTML, JSON, streaming).
- **Prefer boring, explicit Go** — constructor-injected dependencies, standard `http.Handler` signatures, small domain repository interfaces, structured errors, and no hidden package-level initialization. No `init()` functions for wiring; no global singletons.
- **Use ndesign as the frontend layer** — server-rendered HTML, CDN-pinned runtime assets, `data-nd-*` behavior, and Go response helpers that match the runtime envelope exactly. Never propose React/Vue/Svelte/Zod/Vite for new work; never preserve them when migrating.
- **Treat the backend as authoritative** for authentication, authorization, validation, persistence, caching, and business rules. The frontend may guide users (HTML5 attributes, server-rendered conditionals), but it does not enforce trust boundaries.
- **Honor the project layout rules** — no `internal/` directory, web subpackages live under `web/` (`web/wsock/` framework + `web/ws/` integration), cross-cutting helpers live in `dry/`. See the **`go-project-layout`** pattern.
- **Remove replaced approaches completely.** Do not preserve React-era, Zod-era, legacy router, fallback response, or old integration paths unless the user explicitly asks for migration compatibility. See the **`go-mandatory-rules`** pattern (No Legacy / Fallback Code Rule).
- **Ask before choosing an ndesign layout** for new UI work. The starting skeleton (`control-panel` / `app-shell` / `blog`) is part of the architecture, not a cosmetic choice. Do not guess from the task description, do not default silently. See the **`ndesign-layouts`** pattern.
- **When duplication is forming, move it to `dry/`** rather than copying it. As soon as a helper is needed in two packages, it belongs in the `dry` package. See the **`go-dry-package`** pattern.

Use this pattern as the default build playbook for new Go applications and as the review checklist for existing ones. When this guide and the canonical ndesign spec disagree on runtime detail, the spec wins; otherwise, this guide is binding.

### See also

- `go-mandatory-rules` — the SDK-first, no-legacy, no-internal, HTML-only rules referenced throughout the directives.
- `go-project-layout` — concrete file/directory shape every Go application in this style follows.
- `ndesign-layouts` — the mandatory ask-first layout-selection prompt.
- `fullstack-symmetry-table` — the Go ↔ ndesign mapping that makes this contract tractable.

---

## Mandatory Rules
<!-- pattern-slug: go-mandatory-rules -->

### Scope

This pattern aggregates the four hard rules that override every other recommendation in the catalog: SDK-first integration, no legacy/fallback code, no `internal/` (no stuttering paths), and no custom CSS/JS on the frontend. Apply these on every change. They do NOT describe how to implement any specific subsystem — for that, load the matching per-subsystem pattern.

### SDK-First Integration Rule

If an official SDK exists for a provider, service, or capability, it **MUST** be used. Raw HTTP calls are prohibited when an SDK is available.

- Before implementing any integration with an external service, verify whether an official SDK exists.
- If an official SDK exists, use it. No exceptions.
- If existing code uses raw HTTP where an SDK is available, it **MUST** be migrated immediately.
- This applies to all LLM providers (Google, OpenAI, Anthropic, AWS Bedrock, etc.) and any other external service integrations.
- Raw HTTP is only permitted when no official SDK exists for the required functionality (e.g., Ollama, self-hosted endpoints with no SDK).

**Official SDKs for LLM providers:**

| Provider | SDK | Module |
|----------|-----|--------|
| Google Gemini | go-genai | `google.golang.org/genai` |
| OpenAI | openai-go | `github.com/openai/openai-go` |
| Anthropic | anthropic-sdk-go | `github.com/anthropics/anthropic-sdk-go` |
| AWS Bedrock | aws-sdk-go-v2 | `github.com/aws/aws-sdk-go-v2/service/bedrockruntime` |

### No Legacy / Fallback Code Rule

We do **NOT** under any circumstances (unless EXPLICITLY required) leave fallback, legacy, or leftover code. Any upgrades or changes must follow this process:

1. **Document** — Record what is being replaced and why.
2. **Clean** — Fully remove the old implementation (functions, types, constants, helpers, imports).
3. **Rebuild** — Implement the replacement from scratch using the new approach.

**NO FALLBACK. NO LEGACY. NO LEFTOVERS.**

- When upgrading or replacing any functionality, delete the old implementation entirely.
- Do not keep old code "just in case" — if the new implementation fails to compile, fix it.
- This includes helper functions, types, constants, and any supporting code that was only used by the old implementation.
- Dead code paths cause confusion about what's actually running, mask bugs, and create maintenance burden.

### No `internal/`, No Stuttering Paths

Go already enforces public vs. private at the package level via exported (capitalized) and unexported (lowercase) identifiers. **The `internal/` directory is forbidden in this project's Go code.** It adds nothing the language doesn't already provide and produces stuttering paths (`internal/web/web.go`, `internal/wsock/wsock.go`) that read worse than flat ones (`web/web.go`, `web/wsock/wsock.go`).

The full layout — including the `web/wsock/` + `web/ws/` split and the cross-cutting `dry/` package — is defined in the **`go-project-layout`** pattern.

### No Custom CSS or JavaScript on the Frontend

ndesign is an **HTML-only** framework from the consumer's perspective. The bundled `ndesign.min.css` and `ndesign.min.js` provide all styling, interactivity, data binding, and server communication a page needs. Consumer pages SHOULD contain **zero** `<style>` blocks, **zero** inline `style="…"` attributes, and **zero** `<script>` blocks beyond the one that loads the runtime.

This is a deliberate constraint, not an oversight:

- If a visual treatment requires custom CSS, the framework is missing a component or utility class. The correct response is to extend the framework — not to patch around it with one-off styles.
- If an interaction requires custom JavaScript, the framework is missing a `data-nd-*` attribute, a success-chain action, or a store operation. The correct response is to extend the runtime — not to add ad-hoc `<script>` handlers.

Acceptable exceptions are third-party libraries that provide capabilities outside the framework's scope:

| Exception                | Examples                                |
|--------------------------|-----------------------------------------|
| Charting / visualization | Chart.js, D3, ECharts, Plotly           |
| Animation / motion       | GSAP, Lottie, anime.js                  |
| Rich text editing        | TipTap, ProseMirror, CodeMirror         |
| Maps                     | Leaflet, Mapbox GL                      |

Even when using these libraries, the surrounding layout, typography, cards, forms, and controls SHOULD still come from ndesign — only the specialised rendering surface itself should be third-party.

### See also

- `go-project-layout` — the concrete layout that the No-`internal/` rule produces.
- `go-agent-operating-manual` — the higher-level directive checklist that points to these mandatory rules.
- `ndesign-architecture` — the HTML-only philosophy fully explained for the frontend layer.

---

# Backend (Go)

## 1. Project Layout
<!-- pattern-slug: go-project-layout -->

### Scope

This pattern defines the canonical directory and package layout for a Go application built on this stack. It covers the flat (no-`internal/`) package shape, the `web/wsock/` + `web/ws/` split for WebSockets, the role of `dry/` for cross-cutting helpers, and the boundary types in `types/`. It does NOT cover what each package contains — see the per-subsystem patterns referenced below for entry-point lifecycle, response envelope, repository pattern, etc.

### Layout Rules

1. **No `internal/` directory.** Every package lives at the repository root or as a subpackage of one of the root packages. Public vs. private is controlled by Go's own capitalization rule, not directory placement.
2. **The `web` package is the HTTP server package.** WebSocket-related packages are subpackages of `web`, not siblings:
   - `web/wsock/` — the WebSocket *framework*: `SocketManager`, `ClientInfo`, heartbeat, message protocol, queue interface, queue provider adapters.
   - `web/ws/` — the actual WebSocket *integration handlers*: `HandleWSUpgrade`, `HandleTokenRequest` (JWT issuance), per-feed handlers.
3. **Cross-cutting helpers live in `dry/`.** When the same helper would otherwise be duplicated across packages — UUID generation, bcrypt hashing, bearer-token validation, slice diffs, template-merge utilities — it belongs in the top-level `dry` package (named for the Don't-Repeat-Yourself principle). See the **`go-dry-package`** pattern.
4. **Domain types live in `types/`.** Plain Go structs (POGOs) only. These are the only types that cross package boundaries. Database-specific wrappers (`sql.NullString`, `pgtype.*`, `queries.*` from sqlc) never leave `storage/`.

### Canonical Directory Tree

```
.
├── cmd/
│   └── main.go                      # entry point
├── web/                             # HTTP server package
│   ├── server.go                    # Server struct, NewServer, Router()
│   ├── routes.go                    # registerRoutes()
│   ├── response.go                  # RenderContent, RenderError, RenderFormErrors, ...
│   ├── auth_middleware.go           # AuthMiddleware, GetSession, session-in-context
│   ├── page.go                      # RenderPage, html/template orchestration
│   ├── handlers/                    # one package per domain (users/, orders/, ...)
│   ├── wsock/                       # WebSocket framework — SocketManager, message protocol
│   │   ├── wsock.go
│   │   ├── manager.go
│   │   ├── queues.go                # Queue interface + factory
│   │   └── qproviders/              # Pub/Sub, SQS, Redis Streams adapters
│   └── ws/                          # WebSocket integration handlers
│       ├── upgrade.go               # HandleWSUpgrade
│       └── token.go                 # HandleTokenRequest (JWT issuance)
├── auth/                            # session validation, JWT signing/verification
├── rbac/                            # Check(session, action, level), role definitions
├── repo/                            # repository interfaces (UserRepo, OrderRepo, ...)
├── storage/                         # repository implementations + cached decorators
│   ├── store.go                     # Store composition root
│   ├── cached_user_repo.go
│   ├── sqlite/                      # sqlc-generated queries + impls
│   ├── postgres/
│   └── mysql/
├── cache/                           # CacheService with singleflight
├── config/                          # bootstrap YAML + runtime config store
├── metrics/                         # Prometheus middleware + application metrics
├── dry/                             # cross-cutting helpers (Don't Repeat Yourself)
│   ├── uuid.go                      # dry.GenerateUUID
│   ├── hashes.go                    # dry.GenerateBCryptHash, dry.ValidateBearerToken
│   ├── slices.go                    # dry.Difference, dry.RemoveFromSlice (generics)
│   └── strings.go                   # dry.Contains, dry.JsonOrEmpty
├── types/                           # domain POGOs (cross-package value types)
├── templates/                       # Go html/template files
│   ├── _base.html
│   ├── users/
│   │   ├── list.html
│   │   └── edit.html
│   └── ...
├── static/                          # static assets (favicons, images) — optional
├── migrations/                      # golang-migrate SQL files
├── config.yaml
└── .air.toml
```

### Why this shape

- **Flat is honest.** A package's path matches what it does. `web/wsock/wsock.go` reads better than `internal/wsock/wsock.go`; the latter does not gain you anything Go's capitalization rules don't already enforce.
- **`web/wsock/` vs. `web/ws/` separates framework from integration.** The framework knows nothing about your routes; the integration package knows nothing about heartbeat protocol. Both can evolve independently.
- **`dry/` keeps the rest of the tree free of utility files.** Without it, every package grows a `helpers.go` and the same UUID generator gets re-implemented three times.
- **`types/` is the contract surface.** Handlers, repositories, and tests share these structs; nothing else crosses package boundaries.

### See also

- `go-mandatory-rules` — the underlying No-`internal/` rule.
- `go-dry-package` — full details on what lives in `dry/` and when to add to it.
- `go-repository-pattern` — how `types/`, `repo/`, and `storage/` interact.
- `go-entry-point-lifecycle` — what `cmd/main.go` looks like.

---

## 2. Entry Point & Application Lifecycle
<!-- pattern-slug: go-entry-point-lifecycle -->

### Scope

This pattern defines `cmd/main.go` and the application lifecycle contract: who owns process exit, how subsystems are wired, and how graceful shutdown works. It does NOT cover individual subsystems' construction details (those live in their own patterns), nor does it cover deployment topology.

### Rule

`main()` is the sole owner of process lifecycle. Every subsystem is constructed via a function that returns `(T, error)` — sub-packages **never** call `log.Fatal()`. Shutdown drains in-flight requests via `http.Server.Shutdown()` and uses `signal.NotifyContext` for signal handling.

### Implementation

```go
// cmd/main.go
func main() {
    setLogger()

    ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGINT, syscall.SIGTERM)
    defer stop()

    cfg, err := config.Load(ctx)
    if err != nil {
        log.Fatal().Err(err).Msg("failed to load config")
    }

    db, err := data.Connect(cfg.DBConnString)
    if err != nil {
        log.Fatal().Err(err).Msg("failed to connect to DB")
    }

    c, err := cache.New(cfg.CacheConfig)
    if err != nil {
        log.Fatal().Err(err).Msg("failed to initialize cache")
    }

    sm := wsock.NewSocketManager(ctx, cfg)
    app := web.NewServer(cfg, db, c, sm)

    srv := &http.Server{Addr: ":" + cfg.ListenPort, Handler: app.Router()}

    go func() {
        if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
            log.Fatal().Err(err).Msg("listen error")
        }
    }()

    log.Info().Msgf("Server started on :%s", cfg.ListenPort)
    <-ctx.Done()
    log.Info().Msg("Shutting down gracefully...")

    shutdownCtx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
    defer cancel()

    if err := srv.Shutdown(shutdownCtx); err != nil {
        log.Fatal().Err(err).Msg("forced shutdown")
    }
}
```

### Conventions

- Constructors return `(T, error)` — never `log.Fatal()` inside a sub-package.
- `main()` is the sole decision-maker about process exit.
- `http.Server.Shutdown()` drains in-flight requests on SIGINT/SIGTERM.
- `signal.NotifyContext` replaces manual channel management.
- Dependencies are explicit constructor arguments — no package-level globals, no implicit `Init*()` side effects.

### See also

- `go-logging-zerolog` — the `setLogger` body and logging conventions referenced above.
- `go-config-two-tier` — what `config.Load` reads (bootstrap YAML → DB-backed runtime config).
- `go-router-chi` — what `web.NewServer` constructs.
- `go-cache-singleflight` — what `cache.New` returns.

---

## 3. Logging — zerolog
<!-- pattern-slug: go-logging-zerolog -->

### Scope

This pattern defines the structured-logging conventions for every Go service in this stack: which library, how levels are toggled, what fields appear on every log line, and the rule against `log.Fatal()` outside `main`. It does NOT cover distributed tracing — that lives in the **`go-observability-otel`** pattern.

### Rule

`github.com/rs/zerolog` is the standard library — zero-allocation, JSON-native, fastest structured logger for Go. A global level toggled by `APP_DEBUG`:

```go
func setLogger() {
    zerolog.TimeFieldFormat = time.RFC3339Nano
    if os.Getenv("APP_DEBUG") != "" {
        zerolog.SetGlobalLevel(zerolog.DebugLevel)
        return
    }
    zerolog.SetGlobalLevel(zerolog.InfoLevel)
}
```

| Aspect | Convention |
|--------|-----------|
| **Library** | zerolog — structured, zero-allocation JSON logging |
| **Time format** | RFC 3339 with nanosecond precision |
| **Debug toggle** | Any non-empty `APP_DEBUG` value enables debug |
| **Default level** | `InfoLevel` in production |
| **Usage** | Import `github.com/rs/zerolog/log` directly — no wrapper |

Use structured fields (`.Str()`, `.Int()`, `.Err()`, `.Interface()`, `.Dur()`, `.Bool()`) for machine-parseable output.

### Rule: No `log.Fatal()` Outside `main()`

Sub-packages **must not** call `log.Fatal()` or `log.Panic()`. Return errors and let `main()` decide. This is the runtime corollary of the lifecycle ownership rule in **`go-entry-point-lifecycle`** — the sole exit decider is `main`.

### See also

- `go-entry-point-lifecycle` — where `setLogger()` is called and the no-Fatal rule originates.
- `go-observability-otel` — the tracing layer that completes the observability picture.
- `go-prometheus-metrics` — the third pillar (metrics) for full observability.

---

## 4. Observability — OpenTelemetry
<!-- pattern-slug: go-observability-otel -->

### Scope

This pattern defines distributed tracing for every Go service: how trace IDs are generated, how spans propagate through the chi/repo/queue/WebSocket chain, and how the trace ID is surfaced to the browser via `X-Trace-ID`. It does NOT cover logging (see `go-logging-zerolog`) or metrics (see `go-prometheus-metrics`); together those three patterns are the three pillars of observability.

### Why tracing

Structured logging tells you *what* happened. Distributed tracing tells you *why* — was the 3-second latency from the DB query, the queue publish, or the WebSocket dispatch?

**The Problem:** With WebSockets, Pub/Sub queues, and async execution chains, a single user action can span multiple goroutines and services. Correlating logs across these boundaries is manual and error-prone.

**The Solution:** OpenTelemetry middleware generates a `trace_id` per request and propagates it through the entire chain.

### Wiring

```go
import (
    "go.opentelemetry.io/otel"
    "go.opentelemetry.io/contrib/instrumentation/net/http/otelhttp"
)

func (s *Server) registerMiddleware() {
    s.router.Use(otelhttp.NewMiddleware("app-service"))
    s.router.Use(s.requestLogger())
}
```

### Trace propagation through the stack

```
HTTP Request (trace_id generated)
  → chi middleware (otelhttp)
    → AuthMiddleware (span: "auth")
      → DB query (span: "sql.query")
      → Cache lookup (span: "bigcache.get")
    → Handler (span: "handler.GetUsers")
      → Queue publish (span: "pubsub.publish")
        → WebSocket dispatch (span: "ws.send")
```

### Frontend correlation

Pass the trace ID to the browser via a response header so client-side errors can be correlated:

```go
func traceHeaderMiddleware(next http.Handler) http.Handler {
    return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
        span := trace.SpanFromContext(r.Context())
        if span.SpanContext().HasTraceID() {
            w.Header().Set("X-Trace-ID", span.SpanContext().TraceID().String())
        }
        next.ServeHTTP(w, r)
    })
}
```

### Key OTel dependencies

```
go.opentelemetry.io/otel
go.opentelemetry.io/otel/sdk
go.opentelemetry.io/contrib/instrumentation/net/http/otelhttp
go.opentelemetry.io/contrib/instrumentation/database/sql/otelsql
```

**Benefit:** When an engineer says "the WebSocket execution failed," you look at a single trace that shows the HTTP request, the DB lookup, the queue publish, and the WebSocket dispatch — all in one timeline with latency breakdowns.

### See also

- `go-logging-zerolog` — the logs side; pair every log entry with its `trace_id` for correlation.
- `go-prometheus-metrics` — the metrics side; the third pillar.
- `go-router-chi` — where `otelhttp.NewMiddleware` and `traceHeaderMiddleware` are registered.

---

## 5. Router & Middleware — chi
<!-- pattern-slug: go-router-chi -->

### Scope

This pattern defines the HTTP routing layer: which router, the `Server` struct, the convention for adding new routes, and the standard middleware stack. It does NOT cover RBAC (see `go-rbac-middleware`), the response envelope (see `go-response-envelope`), or metrics middleware (see `go-prometheus-metrics`) — those are layered on top.

### Library

The router is **chi** (`github.com/go-chi/chi/v5`) — lightweight, stdlib `http.Handler` compatible, with composable middleware. Handlers use the standard `func(w http.ResponseWriter, r *http.Request)` signature.

### Server Struct

```go
type Server struct {
    db     *sql.DB
    cfg    *config.AppConfig
    sm     *wsock.SocketManager
    cache  *cache.Service
    router chi.Router
}

func NewServer(cfg *config.AppConfig, db *sql.DB, c *cache.Service, sm *wsock.SocketManager) *Server {
    s := &Server{db: db, cfg: cfg, sm: sm, cache: c, router: chi.NewRouter()}
    s.registerMiddleware()
    s.registerRoutes()
    return s
}

func (s *Server) Router() http.Handler { return s.router }
```

### Convention: Adding New Routes

1. Create `web/handlers/<domain>/` with handler functions using `func(w http.ResponseWriter, r *http.Request)` signatures.
2. Add `<Domain>Routes(chi.Router)` in `routes.go`.
3. Add one line to `registerRoutes()`: `s.router.Route("/api/<domain>", <Domain>Routes)`.
4. Use chi middleware for auth: `r.With(AuthMiddleware(action, level)).Get("/path", handler)`.
5. Extract URL parameters with `chi.URLParam(r, "id")` — never from arbitrary context values.

### Standard Middleware Stack

```go
func (s *Server) registerMiddleware() {
    s.router.Use(middleware.RequestID)
    s.router.Use(middleware.RealIP)
    s.router.Use(otelhttp.NewMiddleware("app-service"))
    s.router.Use(traceHeaderMiddleware)
    s.router.Use(metrics.Middleware)
    s.router.Use(s.requestLogger())
    s.router.Use(middleware.Recoverer)
    s.router.Use(middleware.Timeout(60 * time.Second))
}
```

### See also

- `go-rbac-middleware` — the auth/RBAC middleware applied to protected routes via `r.With(AuthMiddleware(...))`.
- `go-response-envelope` — what handlers MUST use to write responses.
- `go-observability-otel` — origin of `otelhttp.NewMiddleware` and `traceHeaderMiddleware`.
- `go-prometheus-metrics` — the `metrics.Middleware` used in the stack.

---

## 6. Authorization — RBAC
<!-- pattern-slug: go-rbac-middleware -->

### Scope

This pattern defines authorization for every protected endpoint: hierarchical RBAC levels, the `AuthMiddleware`, session-in-context retrieval via `web.GetSession(r)`, and the rule that the backend is the only enforcer. It does NOT define the session schema or login flow — those are in `auth/`.

### Model

The default authorization model is **RBAC** (Role-Based Access Control) — a role per user-resource-scope tuple, with a small set of canonical actions (`create`, `read`, `update`, `delete`, plus optional `special`/`protected` for elevated operations).

> A richer ABAC (Attribute-Based Access Control) model is possible if your application needs context-sensitive policies (resource owner, time-of-day, geo, request attributes). RBAC is the default because it is simpler, easier to reason about, and sufficient for the vast majority of business applications. Promote to ABAC only when you have a concrete policy that RBAC cannot express.

### Hierarchical Levels

```go
const (
    NoneLevel     = iota // 0 — no permission check
    OrgLevel             // 1 — check org permissions
    TeamLevel            // 2 — check org read + team permissions
    ResourceLevel        // 3 — check org read + team read + resource permissions
)
```

### Middleware

Session is injected into the request context. Handlers retrieve it via `web.GetSession(r)`.

```go
type contextKey string

const sessionKey contextKey = "authenticated_session"

// AuthMiddleware returns chi-compatible middleware that validates authentication
// and RBAC permissions, injecting the session into request context.
func AuthMiddleware(action string, level int) func(http.Handler) http.Handler {
    return func(next http.Handler) http.Handler {
        return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
            isAuth, session := auth.IsAuthenticated(r)
            if !isAuth {
                RenderError(w, http.StatusUnauthorized, "unauthorized")
                return
            }
            if !rbac.Check(session, action, level) {
                RenderError(w, http.StatusForbidden, "forbidden")
                return
            }
            ctx := context.WithValue(r.Context(), sessionKey, session)
            next.ServeHTTP(w, r.WithContext(ctx))
        })
    }
}

func GetSession(r *http.Request) *data.Session {
    return r.Context().Value(sessionKey).(*data.Session)
}
```

**Permission model:** Six canonical actions (`create`, `read`, `update`, `delete`, `special`, `protected`), a small set of named roles (e.g., admin, editor, viewer), and a single `Role.CanDo(action) bool` method that the middleware calls.

**Session storage:** Three maps — `OrgRoles`, `TeamRoles`, `ResourceRoles` — populated at login from the database membership tables.

### Rule

The backend is the only enforcer. The frontend may hide buttons via server-rendered conditionals, but every protected handler MUST be wrapped with `AuthMiddleware`.

### See also

- `go-router-chi` — where `AuthMiddleware` is wired with `r.With(AuthMiddleware(action, level))`.
- `go-response-envelope` — `RenderError` is what the middleware calls on rejection.
- `go-websocket-jwt-auth` — the WebSocket equivalent of this middleware (claims-based, JWT-signed).

---

## 7. Response Conventions — The ndesign Contract
<!-- pattern-slug: go-response-envelope -->

### Scope

This pattern defines the seven HTTP response helpers (`RenderJSON`, `RenderContent`, `RenderPaginated`, `RenderSuccess`, `RenderError`, `RenderFieldErrors`, `RenderFormErrors`) — the load-bearing contract between Go and the ndesign frontend runtime. Every handler MUST use these helpers; deviating breaks the runtime's automatic error/success surfacing, form-error mapping, and auto-feedback alert slot. It does NOT cover form-error mapping rules on the frontend (see `ndesign-forms`) or validation tag conventions (see `ndesign-validation`).

### The Contract in One Place

| Helper | Status | JSON Body | Used For |
|---|---|---|---|
| `RenderJSON(w, status, body)` | caller-specified | `body` (any) | low-level escape hatch only |
| `RenderContent(w, data)` | 200 | `data` (bare object/array) | `data-nd-bind` GET responses |
| `RenderPaginated(w, data, meta)` | 200 | `{"data":[...],"meta":{...}}` | `data-nd-bind` + `data-nd-select="data"` |
| `RenderSuccess(w, message)` | 200 | `{"message": "..."}` | action endpoints (drives auto-feedback) |
| `RenderError(w, status, message)` | 4xx/5xx | `{"errors":{"error":"..."}}` | global failure |
| `RenderFieldErrors(w, status, fields)` | 4xx | `{"errors": <fields>}` | per-field validation only |
| `RenderFormErrors(w, status, msg, fields)` | 4xx | `{"errors":{"error":"...","field":"..."}}` | global + per-field validation |

### Why this shape

ndesign's runtime synthesises a single error envelope for every failure — fetch error, validation error, timeout, network drop. The shape is fixed:

```json
{ "errors": { "error": "Human-readable global message", "field_name": "per-field message" } }
```

- `errors.error` is the canonical global-message key.
- `errors._form` is accepted as a legacy alias for `errors.error`; both are routed to the feedback slot. **New backends MUST use `errors.error`.**
- Any other key in `errors` is treated as a field-level error and matched against an input by its `name=` attribute (forms only).
- `Content-Type: application/json` MUST be set on error responses — the runtime only parses JSON when the header matches.

For successful responses:

- `data-nd-bind` GETs read the response body directly. If you also need pagination, return `{"data": [...], "meta": {...}}` and the page sets `data-nd-select="data"` to unwrap it.
- Action endpoints (`data-nd-action`) read `responseData.message` to populate the auto-feedback success alert. Returning `{"message": "User created"}` is what makes the green success banner appear.

### Reference Implementation

```go
// web/response.go
package web

import (
    "encoding/json"
    "net/http"

    "github.com/rs/zerolog/log"
)

// RenderJSON is the low-level helper. Always sets Content-Type: application/json.
func RenderJSON(w http.ResponseWriter, status int, body any) {
    w.Header().Set("Content-Type", "application/json")
    w.WriteHeader(status)
    if body == nil {
        return
    }
    if err := json.NewEncoder(w).Encode(body); err != nil {
        log.Error().Err(err).Msg("response: failed to encode JSON")
    }
}

// RenderContent: 200 OK, body is the bare data object/array.
// Pairs with `data-nd-bind="${api}/api/foo"` on the page.
func RenderContent(w http.ResponseWriter, data any) {
    RenderJSON(w, http.StatusOK, data)
}

// RenderPaginated: 200 OK, body is {"data": [...], "meta": {...}}.
// Pairs with `data-nd-bind` + `data-nd-select="data"` on the page.
func RenderPaginated(w http.ResponseWriter, data any, meta any) {
    RenderJSON(w, http.StatusOK, map[string]any{
        "data": data,
        "meta": meta,
    })
}

// RenderSuccess: 200 OK, body is {"message": "..."}.
// The ndesign auto-feedback slot picks up `responseData.message` and shows
// it as the success alert next to the form / button.
func RenderSuccess(w http.ResponseWriter, message string) {
    RenderJSON(w, http.StatusOK, map[string]string{"message": message})
}

// RenderError: caller-specified status, body is {"errors": {"error": "..."}}.
func RenderError(w http.ResponseWriter, status int, message string) {
    RenderJSON(w, status, map[string]any{
        "errors": map[string]string{"error": message},
    })
}

// RenderFieldErrors: per-field validation only (no global message).
// Keys MUST match the form input `name` attributes on the page so ndesign
// can target their `.nd-form-error` siblings.
func RenderFieldErrors(w http.ResponseWriter, status int, fields map[string]string) {
    RenderJSON(w, status, map[string]any{
        "errors": fields,
    })
}

// RenderFormErrors: global error + per-field validation.
// Use this when the request as a whole is rejected AND specific fields are
// invalid — for example, a duplicate-email submission that also fails a
// password strength check.
func RenderFormErrors(w http.ResponseWriter, status int, globalMsg string, fields map[string]string) {
    out := map[string]string{"error": globalMsg}
    for k, v := range fields {
        out[k] = v
    }
    RenderJSON(w, status, map[string]any{"errors": out})
}
```

**These helpers are mandatory for any handler — never call `json.NewEncoder(w).Encode()` with ad-hoc shapes.** The response envelope is the contract between Go and ndesign; deviating breaks the framework's automatic error/success surfacing.

### Handler Examples

```go
// GET /api/users — list
func (s *Server) ListUsers(w http.ResponseWriter, r *http.Request) {
    users, err := s.repo.Users.List(r.Context())
    if err != nil {
        RenderError(w, http.StatusInternalServerError, "failed to list users")
        return
    }
    RenderContent(w, users)
}

// GET /api/users/paginated?page=2&per_page=25
func (s *Server) ListUsersPaginated(w http.ResponseWriter, r *http.Request) {
    page, perPage := parsePagination(r)
    users, total, err := s.repo.Users.Paginate(r.Context(), page, perPage)
    if err != nil {
        RenderError(w, http.StatusInternalServerError, "failed to list users")
        return
    }
    RenderPaginated(w, users, map[string]any{
        "page":     page,
        "per_page": perPage,
        "total":    total,
    })
}

// POST /api/users — create
func (s *Server) CreateUser(w http.ResponseWriter, r *http.Request) {
    var req CreateUserRequest
    if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
        RenderError(w, http.StatusBadRequest, "invalid request body")
        return
    }
    if errs := validate.Struct(req); errs != nil {
        // Map go-playground/validator errors to per-field map
        fields := mapValidationErrors(errs)
        RenderFormErrors(w, http.StatusUnprocessableEntity, "Please correct the form.", fields)
        return
    }
    if err := s.repo.Users.Create(r.Context(), &req); err != nil {
        if errors.Is(err, ErrDuplicateEmail) {
            RenderFieldErrors(w, http.StatusConflict, map[string]string{
                "email": "already taken",
            })
            return
        }
        RenderError(w, http.StatusInternalServerError, "failed to create user")
        return
    }
    RenderSuccess(w, "User created")
}
```

The matching ndesign-side wiring is in the **`ndesign-forms`** pattern.

### See also

- `ndesign-forms` — the frontend half of this contract: how `errors` keys map to `.nd-form-error` siblings.
- `ndesign-validation` — the Go `validate:` tag → HTML5 attribute mapping.
- `ndesign-data-binding` — how `RenderContent` / `RenderPaginated` pair with `data-nd-bind` and `data-nd-select`.
- `go-rbac-middleware` — uses `RenderError` for 401/403.

---

## 8. WebSocket System
<!-- pattern-slug: go-websocket-system -->

### Scope

This pattern defines the WebSocket framework: the `web/wsock/` ↔ `web/ws/` split, `SocketManager`, the `SocketMessage` protocol, broadcast/fan-out, and how subscription state is encoded in the URL. It does NOT cover authentication on upgrade — that lives in the **`go-websocket-jwt-auth`** pattern. It also does NOT cover frontend wiring; that is **`ndesign-websocket-integration`**.

### Package Split

WebSockets bridge the server and persistent clients (browsers, agent CLIs, IoT devices) for bidirectional message flow. The system splits across two subpackages of `web`:

- **`web/wsock/`** — the framework (`SocketManager`, `ClientInfo`, heartbeat, message protocol, queue interface, queue provider adapters). No HTTP handlers live here.
- **`web/ws/`** — the integration handlers (`HandleWSUpgrade`, `HandleTokenRequest` for JWT issuance, per-feed handlers). These are the entry points the router wires up.

### Connection flow (browser)

```
Browser  →  POST /api/auth/token (authenticated by session cookie)
         ←  { "token": "<jwt>", "expires_in": 300 }

Browser  →  GET /ws/feed?token=<jwt>     (Upgrade: websocket)
         ←  101 Switching Protocols      (server verifies signature, reads RBAC claims)

         ←→ JSON frames                  (server fans out subscribed channels)
```

### Message protocol

```go
type SocketMessage struct {
    Type     string `json:"type"`
    Channel  string `json:"channel,omitempty"`
    Action   string `json:"action,omitempty"`
    ActionID string `json:"action_id,omitempty"`
    Payload  any    `json:"payload,omitempty"`
}
```

### The URL is the subscription envelope

ndesign does NOT define a client-side subscribe frame. There is no init message the runtime sends on connect, no JSON-RPC handshake. **All subscription state — which feeds, channels, symbols, filters — is encoded in the URL itself** and read by the server on connect. Backends that expect a post-connect subscribe frame are working against the design.

Encode subscription parameters in the path (`/ws/account/42`), the query string (`?channels=ladder,news,pnl`), or both. Two elements that resolve to the same URL share one socket — multiplexing many channels onto a single connection is just "list them all in the same query string and let the server fan out the frames."

### Conventions

- The `SocketManager` is constructed in `main()` with `wsock.NewSocketManager(ctx, cfg)` and injected into the `Server`.
- A pluggable `Queue` interface (see **`go-queue-system`**) is injected into the `SocketManager` constructor for cross-instance fan-out.
- Per-feed handlers in `web/ws/` should be small — most of the work is in the framework.
- Encode subscription parameters in the WS URL — the URL is the subscription envelope.

### See also

- `go-websocket-jwt-auth` — token endpoint and upgrade-time signature verification.
- `go-queue-system` — the queue used for cross-instance broadcast fan-out.
- `ndesign-websocket-integration` — the frontend `data-nd-ws` pairing.
- `go-project-layout` — defines `web/wsock/` and `web/ws/`.

---

## 9. WebSocket JWT Auth
<!-- pattern-slug: go-websocket-jwt-auth -->

### Scope

This pattern defines authentication for WebSocket upgrades: why JWT (not a static API key), the `/api/auth/token` issuance endpoint, the `HandleWSUpgrade` verification step, and the query-string token contract that the browser API constraint (RFC 6455) forces. It does NOT cover the framework structure (see `go-websocket-system`) or the frontend `wsTokenProvider` wiring (see `ndesign-websocket-integration`).

### Why JWT, not a static API key

Browsers cannot set custom request headers on WebSocket upgrades — RFC 6455 / browser API constraint. There is no way for any client library to attach an `Authorization: Bearer <token>` header from the browser. The canonical browser auth path is therefore a query-string token. To make that token short-lived (so leaks expire on their own), we issue a JWT from a session-authenticated REST endpoint.

### Token endpoint

```go
// POST /api/auth/token — authenticated via session cookie (browser)
//                        or API key (server-side / CLI clients)
func (s *Server) HandleTokenRequest(w http.ResponseWriter, r *http.Request) {
    session := GetSession(r)

    claims := jwt.MapClaims{
        "sub":   session.UserID,
        "org":   session.OrgID,
        "team":  session.TeamID,
        "perms": session.RBACMap(),
        "exp":   time.Now().Add(5 * time.Minute).Unix(),
        "iat":   time.Now().Unix(),
    }

    token := jwt.NewWithClaims(jwt.SigningMethodHS256, claims)
    signed, err := token.SignedString(s.jwtSecret)
    if err != nil {
        RenderError(w, http.StatusInternalServerError, "token generation failed")
        return
    }

    RenderContent(w, map[string]any{"token": signed, "expires_in": 300})
}
```

### WebSocket upgrade handler

```go
func (s *Server) HandleWSUpgrade(w http.ResponseWriter, r *http.Request) {
    rawToken := r.URL.Query().Get("token")
    if rawToken == "" {
        RenderError(w, http.StatusUnauthorized, "missing token")
        return
    }

    claims, err := s.verifyJWT(rawToken)
    if err != nil {
        RenderError(w, http.StatusUnauthorized, "invalid token")
        return
    }

    // RBAC check on upgrade — claims["perms"] is advisory, the backend
    // still enforces on each action dispatch.
    if !rbac.AllowsWS(claims) {
        RenderError(w, http.StatusForbidden, "forbidden")
        return
    }

    conn, err := s.upgrader.Upgrade(w, r, nil)
    if err != nil {
        log.Warn().Err(err).Msg("ws: upgrade failed")
        return
    }

    s.sm.RegisterClient(conn, claims, r.URL.Query())
}
```

### Why this matters

| Concern | Static `Authorization: id:key` | JWT (this design) |
|---------|--------------------------------|-------------------|
| Browser support | Impossible (browsers can't set WS headers) | Native (token in query string) |
| Key rotation | Requires client reconnect with new key | Rotate signing key; clients re-fetch token |
| Auth cost per upgrade | DB lookup to validate key | Signature verification only (CPU, no I/O) |
| RBAC on upgrade | Separate permission check call | Claims embedded in token |
| Token lifetime | Static (never expires) | 5-minute expiry, auto-refresh |
| Revocation | Delete key from DB | Short TTL + optional deny-list |

### Conventions

- Token TTL should be short (5 minutes) — clients refresh before expiry.
- RBAC claims are advisory for the upgrade handler; the backend still enforces on each action dispatch.
- Use `HS256` with a server-side secret; no need for asymmetric keys in a single-service architecture.
- The `/api/auth/token` endpoint is authenticated via existing `AuthMiddleware`.
- Backends MUST accept `token=<value>` as a query parameter for browser clients. A backend that only accepts `Authorization` headers is unreachable from a browser.

### See also

- `go-websocket-system` — the framework this auth plugs into.
- `go-rbac-middleware` — the session and RBAC model the JWT claims mirror.
- `ndesign-websocket-integration` — the frontend `wsTokenProvider` wiring.
- `go-response-envelope` — `RenderError` / `RenderContent` used in the handlers above.

---

## 10. Queue System
<!-- pattern-slug: go-queue-system -->

### Scope

This pattern defines the pluggable `Queue` interface and provider factory pattern for cross-instance message delivery (Pub/Sub fan-out, async work, etc.). It does NOT prescribe a specific provider — the choice is configurable. It does NOT cover the WebSocket dispatch loop that consumes from the queue (see `go-websocket-system`).

### Interface

```go
type Queue interface {
    Enqueue(ctx context.Context, id string, payload string, ttl int64) error
    Receive(ctx context.Context) (<-chan Message, error)
    Close() error
}
```

### Factory

**Providers:** Google Pub/Sub (production), AWS SQS, Redis Streams. Each implements the interface and is selected via configuration. The factory function returns `(Queue, error)` — never `log.Fatal`. The `Queue` is injected into the `SocketManager` constructor.

```go
func NewQueue(cfg config.QueueConfig) (Queue, error) {
    switch cfg.Provider {
    case "pubsub":
        return pubsub.New(cfg)
    case "sqs":
        return sqs.New(cfg)
    case "redis":
        return redis.New(cfg)
    default:
        return nil, fmt.Errorf("unknown queue provider: %s", cfg.Provider)
    }
}
```

### Conventions

- Provider adapters live in `web/wsock/qproviders/` (per the **`go-project-layout`** pattern).
- Constructors return `(Queue, error)` — let `main()` decide what to do with errors.
- Use the SDK-first rule: each provider adapter MUST use the official SDK (`cloud.google.com/go/pubsub`, `aws-sdk-go-v2`, `github.com/redis/go-redis/v9`).

### See also

- `go-mandatory-rules` — SDK-first rule applies to every provider implementation.
- `go-project-layout` — adapter location (`web/wsock/qproviders/`).
- `go-websocket-system` — the consumer of the `Queue` interface.

---

## 11. Cache Layer — BigCache + singleflight
<!-- pattern-slug: go-cache-singleflight -->

### Scope

This pattern defines the in-memory cache layer: `CacheService` wrapping `bigcache` with `singleflight` stampede protection, the `GetOrFetch` API, and the cached-decorator usage at the repository boundary. It does NOT cover Redis or other external caches — this is the in-process caching layer. For the cached-decorator wrapper around a repository, see `go-repository-pattern`.

### The Problem

When a cache key expires under high traffic, 100 concurrent requests for the same key all miss the cache and hit the database simultaneously. This is a "thundering herd" or cache stampede.

### The Solution

`singleflight.Group` ensures that only one goroutine fetches from the database. The other 99 wait and share the result.

```go
import (
    "context"
    "encoding/json"
    "fmt"

    "github.com/allegro/bigcache/v3"
    "golang.org/x/sync/singleflight"
)

type CacheService struct {
    store *bigcache.BigCache
    sf    singleflight.Group
}

func NewCacheService(cfg bigcache.Config) (*CacheService, error) {
    store, err := bigcache.New(context.Background(), cfg)
    if err != nil {
        return nil, fmt.Errorf("cache init: %w", err)
    }
    return &CacheService{store: store}, nil
}

// GetOrFetch checks cache, deduplicates concurrent DB calls, then warms cache.
func (c *CacheService) GetOrFetch(key string, fetchFn func() (any, error)) (any, error) {
    // 1. Check cache
    if cached, err := c.store.Get(key); err == nil {
        var result any
        if err := json.Unmarshal(cached, &result); err == nil {
            return result, nil
        }
    }

    // 2. Singleflight: only one goroutine hits the DB
    v, err, _ := c.sf.Do(key, func() (any, error) {
        result, err := fetchFn()
        if err != nil {
            return nil, err
        }

        // 3. Warm cache for subsequent requests
        if data, err := json.Marshal(result); err == nil {
            _ = c.store.Set(key, data)
        }

        return result, nil
    })

    return v, err
}

func (c *CacheService) Invalidate(key string) {
    _ = c.store.Delete(key)
}
```

### Usage in the data layer

```go
func (r *MySQLUserRepo) GetByID(ctx context.Context, id string) (*types.User, error) {
    v, err := r.cache.GetOrFetch("user-"+id, func() (any, error) {
        u := &types.User{}
        row := r.db.QueryRowContext(ctx, `SELECT id, name, email FROM users WHERE id = ?`, id)
        return u, row.Scan(&u.ID, &u.Name, &u.Email)
    })
    if err != nil {
        return nil, err
    }
    return v.(*types.User), nil
}
```

**Impact:** Under a traffic spike with 100 concurrent requests for the same expired key, only 1 DB query executes instead of 100. The other 99 goroutines wait ~1ms and share the result.

### Defaults

| Setting | Value |
|---|---|
| TTL | 2 minutes |
| Cleanup interval | 1 minute |
| Hard memory limit | 8 GB |
| Shards | 1024 |

### See also

- `go-repository-pattern` — the cached-decorator pattern that wraps each repository with `CacheService`.
- `go-prometheus-metrics` — `cache_hits_total` / `cache_misses_total` are exposed as application metrics.
- `go-entry-point-lifecycle` — `cache.New` is constructed in `main()` and injected.

---

## 12. Configuration — Two Tier
<!-- pattern-slug: go-config-two-tier -->

### Scope

This pattern defines the two-tier configuration model: a minimal bootstrap YAML for DB connection only, and a runtime config store backed by the database. It covers scope resolution, change events, the critical-key confirmation rule, and the task-boundary application rule. It does NOT cover individual config keys (those are application-specific).

### Bootstrap YAML

The bootstrap file contains at most 4 fields: `listen_addr`, `data_dir`, `storage.backend`, `storage.dsn`. No feature flags, no subsystem tuning, no per-tenant configuration. If the file does not exist, sensible defaults apply.

```yaml
# config.yaml
listen_addr: :8080
data_dir: /var/lib/app
storage:
  backend: postgres
  dsn: postgres://app:pass@localhost:5432/app?sslmode=disable
```

### Runtime Config Store (Database)

- **`config_keys`** table defines the schema: key name, allowed scope, value type, default, critical flag, description.
- **`config_values`** table stores actual values as scoped key-value pairs.

**Scope resolution:** `agent → workspace → global → default`.

### Implementation Rules

1. **Never read config from YAML at runtime.** Only `LoadBootstrap()` reads the YAML file, and only at startup. All runtime config access goes through `ConfigStore.Resolve()`.
2. **Emit events on change.** Every `ConfigStore.Set()` call emits a `config.changed` event on the internal event bus.
3. **Critical keys require confirmation.** REST handlers must check `IsCritical()` and return a confirmation prompt to the dashboard before persisting.
4. **Task-boundary application.** Long-lived workers apply config changes at task boundaries, not mid-task. When `config.changed` arrives for a worker-scoped key, the worker finishes its current task and reads the new value before starting the next one.
5. **Type safety.** `ConfigStore.Resolve()` returns `string`. Callers parse to the target type using the `value_type` metadata from `config_keys`. Helper functions: `ResolveInt()`, `ResolveBool()`, `ResolveDuration()`.

`ConfigStore` is injected as a dependency. No singletons, no global state.

### See also

- `go-entry-point-lifecycle` — `LoadBootstrap()` is called once in `main()`.
- `go-repository-pattern` — `ConfigStore` is itself implemented as a repository over `config_keys` / `config_values` tables.

---

## 13. Data Layer & Repository Pattern
<!-- pattern-slug: go-repository-pattern -->

### Scope

This pattern defines how data flows from handler to database: domain types as the boundary in `types/`, granular per-domain repository interfaces in `repo/`, cached decorators wrapping inner implementations, and the `Store` composition root that wires it all together at startup. It also covers the four implementation mandates (context pass-through, no `sql.NullString` leakage, atomic migrations, trace correlation). It does NOT cover the cache implementation — see `go-cache-singleflight`.

### 13.1 Domain Model Boundary

All domain types live in `types/` as plain Go structs (POGOs). These are the **only** types that cross package boundaries.

```go
// types/user.go
type User struct {
    ID        string    `json:"id"           validate:"required,uuid"`
    Email     string    `json:"email"        validate:"required,email"`
    Name      string    `json:"name"         validate:"required,min=3,max=64"`
    OrgID     string    `json:"org_id"       validate:"required,uuid"`
    CreatedAt time.Time `json:"created_at"`
    UpdatedAt time.Time `json:"updated_at"`
}
```

**Rule:** Database-specific types (`queries.*` from sqlc, `sql.NullString`, `pgtype.*`, MySQL driver types) **never** appear outside `storage/`. Each backend (SQLite, PostgreSQL, MySQL/MariaDB) translates between domain types and database types at the repository boundary.

### 13.2 Granular Repository Interfaces

Repository interfaces live in `repo/`, one file per domain. Each interface is small, focused, and independently mockable.

```go
// repo/user_repo.go
type UserRepo interface {
    GetByID(ctx context.Context, id string) (*types.User, error)
    GetByEmail(ctx context.Context, email string) (*types.User, error)
    List(ctx context.Context) ([]*types.User, error)
    Paginate(ctx context.Context, page, perPage int) ([]*types.User, int, error)
    Create(ctx context.Context, u *types.User) error
    Update(ctx context.Context, u *types.User) error
    Delete(ctx context.Context, id string) error
}
```

**Rule:** Handler code depends on **individual** repository interfaces, not a monolithic `Store`. A handler that needs users imports `repo.UserRepo`. A handler that needs orders imports `repo.OrderRepo`. No handler ever imports the entire storage package.

### 13.3 Cached Decorator "Shield" Pattern

Cached repositories implement the same interface as their inner implementation, wrapping it with `singleflight`-protected cache-aside logic:

```go
// storage/cached_user_repo.go
type CachedUserRepo struct {
    inner repo.UserRepo
    cache *cache.Service
}

func (r *CachedUserRepo) GetByID(ctx context.Context, id string) (*types.User, error) {
    key := "user:" + id
    val, err := r.cache.GetOrFetch(key, func() (any, error) {
        return r.inner.GetByID(ctx, id)
    })
    if err != nil {
        return nil, err
    }
    return val.(*types.User), err
}

// Write operations invalidate the relevant cache entry (write-through).
func (r *CachedUserRepo) Update(ctx context.Context, u *types.User) error {
    if err := r.inner.Update(ctx, u); err != nil {
        return err
    }
    r.cache.Invalidate("user:" + u.ID)
    return nil
}
```

### 13.4 Composition Root

The `Store` struct in `storage/store.go` is a composition root — not a God object. It wires concrete implementations at construction time:

```go
type Store struct {
    Users  repo.UserRepo
    Orders repo.OrderRepo
    Audits repo.AuditRepo
    // one field per domain
}

func NewStore(cfg config.StorageConfig, c *cache.Service) (*Store, error) {
    var userBase repo.UserRepo
    switch cfg.Driver {
    case "postgres":
        userBase = postgres.NewUserRepo(cfg.DSN)
    case "mysql":
        userBase = mysql.NewUserRepo(cfg.DSN)
    default:
        userBase = sqlite.NewUserRepo(cfg.DSN)
    }

    return &Store{
        Users:  cache.NewCachedUserRepo(userBase, c),
        Orders: ordersFor(cfg, c),
        Audits: auditsFor(cfg, c),
    }, nil
}
```

### 13.5 Implementation Mandates

These are non-negotiable rules for all storage layer code:

| # | Mandate | Rationale |
|---|---------|-----------|
| 1 | **Context pass-through** — Every repository method accepts `context.Context` as its first parameter. Never use `context.Background()` inside a repository. | Enables cancellation propagation, deadline enforcement, and trace correlation across the full call chain. |
| 2 | **No `sql.NullString` leakage** — Database nullable types must be translated to Go zero values or `*T` pointers at the repository boundary. Domain types in `types/` never contain database-specific nullable wrappers. | Prevents database dialect concerns from infecting domain logic. Consumers should never need to check `.Valid` on a domain struct field. |
| 3 | **Atomic migrations** — Each migration file contains exactly one logical change. Never combine table creation with data backfill in a single migration. Use `golang-migrate` with embedded SQL files (`go:embed`). Migrations run automatically at startup via `Store.Migrate()`. | Enables safe rollback of individual changes. Prevents partial migration states that require manual intervention. |
| 4 | **Trace correlation** — Repository implementations must propagate the `trace_id` from context to any log entries or error wraps. Use `fmt.Errorf("repo.UserRepo.GetByID: %w", err)` for error wrapping with the full method path. | Enables end-to-end tracing from HTTP handler → repository → SQL query. Critical for diagnosing performance issues in production. |

### 13.6 Onboarding Note

> **If you find yourself writing a SQL query inside an HTTP handler, you have violated the architecture.**
>
> The correct flow is: `Handler → repo.Interface → storage/<driver>/ → sqlc-generated queries`. Every layer has a single responsibility. Handlers orchestrate. Repositories abstract. Storage implementations execute.

### See also

- `go-cache-singleflight` — the `CacheService` that the cached decorators wrap.
- `go-observability-otel` — trace correlation that mandate #4 plugs into.
- `go-project-layout` — defines `types/`, `repo/`, and `storage/`.
- `ndesign-validation` — the `validate:` tags on domain structs are also the source of truth for HTML5 validation attributes.

---

## 14. Prometheus Metrics — 4 Golden Signals
<!-- pattern-slug: go-prometheus-metrics -->

### Scope

This pattern defines the metrics layer: the HTTP middleware that captures the four golden signals (latency, traffic, errors, saturation), application-specific gauges, and the `/metrics` endpoint. It does NOT cover dashboards or alerting — those are downstream Prometheus/Grafana concerns.

### Why metrics

The backend exposes a `/metrics` endpoint serving Prometheus-format metrics. This covers the [4 Golden Signals](https://sre.google/sre-book/monitoring-distributed-systems/#xref_monitoring_golden-signals) (latency, traffic, errors, saturation) plus application-specific gauges.

Together with zerolog (logs), OpenTelemetry (traces), and Prometheus (metrics), the application covers the **three pillars of observability**.

### Dependency

```go
import (
    "github.com/prometheus/client_golang/prometheus"
    "github.com/prometheus/client_golang/prometheus/promauto"
    "github.com/prometheus/client_golang/prometheus/promhttp"
)
```

### HTTP Golden Signals Middleware

```go
// metrics/middleware.go
package metrics

import (
    "net/http"
    "strconv"
    "time"

    "github.com/go-chi/chi/v5"
    "github.com/prometheus/client_golang/prometheus"
    "github.com/prometheus/client_golang/prometheus/promauto"
)

var (
    httpRequestDuration = promauto.NewHistogramVec(prometheus.HistogramOpts{
        Name:    "http_request_duration_seconds",
        Help:    "HTTP request latency in seconds.",
        Buckets: prometheus.DefBuckets,
    }, []string{"method", "route", "status"})

    httpRequestsTotal = promauto.NewCounterVec(prometheus.CounterOpts{
        Name: "http_requests_total",
        Help: "Total number of HTTP requests.",
    }, []string{"method", "route", "status"})

    httpErrorsTotal = promauto.NewCounterVec(prometheus.CounterOpts{
        Name: "http_errors_total",
        Help: "Total number of HTTP 5xx responses.",
    }, []string{"method", "route"})
)

func Middleware(next http.Handler) http.Handler {
    return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
        start := time.Now()
        ww := NewResponseWriter(w) // wraps http.ResponseWriter to capture status code
        next.ServeHTTP(ww, r)

        status := strconv.Itoa(ww.Status())
        route := chi.RouteContext(r.Context()).RoutePattern() // route template, not raw path
        method := r.Method
        duration := time.Since(start).Seconds()

        httpRequestDuration.WithLabelValues(method, route, status).Observe(duration)
        httpRequestsTotal.WithLabelValues(method, route, status).Inc()

        if ww.Status() >= 500 {
            httpErrorsTotal.WithLabelValues(method, route).Inc()
        }
    })
}
```

**Key design decisions:**

- Uses `chi.RouteContext(r.Context()).RoutePattern()` (e.g., `/api/users/{id}`) not `r.URL.Path` — avoids unbounded cardinality from path parameters.
- `promauto` handles registration — no manual `prometheus.MustRegister` boilerplate.
- `DefBuckets` (5ms–10s) covers typical web handler latencies.
- Uses a `ResponseWriter` wrapper to capture the status code written by downstream handlers.

### Application Metrics

```go
// metrics/application.go
package metrics

import (
    "github.com/prometheus/client_golang/prometheus"
    "github.com/prometheus/client_golang/prometheus/promauto"
)

var (
    WebsocketConnectionsActive = promauto.NewGauge(prometheus.GaugeOpts{
        Name: "websocket_connections_active",
        Help: "Number of active WebSocket connections.",
    })

    QueueMessagesPending = promauto.NewGaugeVec(prometheus.GaugeOpts{
        Name: "queue_messages_pending",
        Help: "Number of pending messages in the queue.",
    }, []string{"provider"})

    CacheHitsTotal = promauto.NewCounter(prometheus.CounterOpts{
        Name: "cache_hits_total",
        Help: "Total cache hits.",
    })

    CacheMissesTotal = promauto.NewCounter(prometheus.CounterOpts{
        Name: "cache_misses_total",
        Help: "Total cache misses.",
    })
)
```

### Exposing the /metrics Endpoint

```go
func (s *Server) registerRoutes() {
    s.router.Use(metrics.Middleware)
    // Prometheus scrape endpoint — no auth required
    s.router.Handle("/metrics", promhttp.Handler())
    // ... existing routes
}
```

### Useful PromQL Queries

| Signal | Query | Purpose |
|--------|-------|---------|
| **Latency (p99)** | `histogram_quantile(0.99, rate(http_request_duration_seconds_bucket[5m]))` | Catch slow endpoints |
| **Traffic** | `sum(rate(http_requests_total[5m])) by (route)` | Requests per second by route |
| **Error rate** | `sum(rate(http_errors_total[5m])) / sum(rate(http_requests_total[5m]))` | Percentage of 5xx responses |
| **Saturation** | `websocket_connections_active` | Current WebSocket load |
| **Cache hit rate** | `cache_hits_total / (cache_hits_total + cache_misses_total)` | Cache effectiveness |
| **Queue backlog** | `queue_messages_pending` | Queue health per provider |

### Conventions

- All metrics live in `metrics/` — never define `prometheus.Counter` in domain packages.
- Use `promauto` for registration — keeps it declarative.
- Label cardinality must stay bounded — use route templates, not raw paths.
- `/metrics` endpoint requires no authentication — Prometheus needs unauthenticated access.
- Application metrics are updated at the point of state change, not via periodic polling (except for sweep-based gauges like connection counts).

### See also

- `go-logging-zerolog` — first pillar of observability.
- `go-observability-otel` — second pillar of observability.
- `go-router-chi` — `metrics.Middleware` is registered in the router middleware stack.
- `go-cache-singleflight` — `CacheHitsTotal` / `CacheMissesTotal` are incremented from there.

---

## 15. The `dry/` Package
<!-- pattern-slug: go-dry-package -->

### Scope

This pattern defines the `dry/` package — the project's home for cross-cutting helpers that would otherwise be duplicated across packages. It covers when to add to `dry/`, what kinds of helpers belong there, and the naming convention. It does NOT cover any specific helper's behaviour — read the individual function's doc comment.

### The Rule

When the same helper would otherwise be duplicated across two or more packages, it belongs in `dry/`. As soon as a function is needed in two packages, move it there rather than letting copies spread.

The package is named for the **D**on't **R**epeat **Y**ourself principle — `dry.GenerateUUID()` reads correctly at the call site and signals "this is a small, well-known utility, not a domain concern."

### What belongs in `dry/`

| Category | Examples |
|---|---|
| ID and key generation | `dry.GenerateUUID()`, `dry.GenerateRandomKey(n)` |
| Hashing and tokens | `dry.GenerateBCryptHash(s)`, `dry.ValidateBearerToken(r, expected)` |
| Slice utilities (generics) | `dry.Difference[T]`, `dry.RemoveFromSlice[T]`, `dry.GetAddRem[T]` |
| String utilities | `dry.Contains(slice, s)`, `dry.JsonOrEmpty(v any)` |
| Template utilities | `dry.StringFromTemplate(tmpl, data)`, `dry.MergeMap(a, b)` |

### What does NOT belong in `dry/`

- Anything domain-specific (`User`, `Order`, business rules) — those live with their domain.
- Anything that depends on a non-trivial subsystem (HTTP, DB, cache) — wrap it in the relevant package instead.
- Anything used only inside a single package — keep it local until a second caller appears.

### Conventions

- One file per category: `dry/uuid.go`, `dry/hashes.go`, `dry/slices.go`, `dry/strings.go`, `dry/templates.go`.
- Functions are `Exported` (capitalized) so all packages can import them.
- Every function has a doc comment with one or two examples.
- Tests live in `dry/<file>_test.go` — these utilities are easy to test exhaustively, so do.
- No `init()`, no package-level state. `dry/` is pure functions.

### When duplication is forming

The first time you copy-paste a helper from one package into another, stop and move it to `dry/` instead. The agent operating manual treats this as a default behaviour: "When duplication is forming, move it to `dry/` rather than copying it."

### See also

- `go-project-layout` — defines `dry/` as a top-level package.
- `go-agent-operating-manual` — the directive that mandates moving duplicated helpers to `dry/`.

---

# Frontend (ndesign)

## 16. Frontend Architecture (ndesign)
<!-- pattern-slug: ndesign-architecture -->

### Scope

This pattern defines the ndesign philosophy and runtime model: vanilla HTML + one CDN bundle, the lifecycle init order, the inventory of state types and where each lives, and the CDN pinning rule. It does NOT cover layouts (see `ndesign-layouts`), data binding (see `ndesign-data-binding`), or any specific directive — those are separate patterns.

### Philosophy

The frontend is **vanilla HTML + vanilla CSS + one bundled JS file from a CDN**. No build step. No React, Vue, Svelte. No Redux. No client-side router. The Go server renders the initial HTML; the ndesign runtime hydrates the page via `data-nd-*` attributes — fetch, render, submit, error-map, websocket.

> ndesign is a small runtime that turns plain HTML attributes into data-bound, server-talking UI.

### Lifecycle

The runtime ships as an IIFE that exposes `window.NDesign` and auto-initialises on `DOMContentLoaded`. If the script tag is placed at the end of `<body>` and the DOM is already parsed, init runs synchronously. The init order is:

1. Read `<meta name="endpoint:*">` and `<meta name="var:*">` into the store.
2. Wire `data-nd-bind` (fetch + render).
3. Wire `data-nd-action` on forms and buttons.
4. Wire `data-nd-ws` and `data-nd-sse`.
5. Wire components (selects, modals, toasts, tabs, dropdowns, tooltips, uploads, sortable, navigation).
6. Wire `data-nd-set` click triggers and `data-nd-model` two-way bindings.
7. Attach a single delegated `click` listener for theme toggling, toasts, and sidebar.

### State

| State Type | Where It Lives | How It's Set |
|---|---|---|
| **Server-rendered initial values** | HTML | `html/template` in Go |
| **Per-page scalars** | Store (vars) | `<meta name="var:NAME" content="...">` |
| **URL bases** | Store (endpoints) | `<meta name="endpoint:NAME" content="...">` |
| **Two-way form input** | Store (vars) via `data-nd-model` | user input |
| **Server data (lists, details)** | DOM, fetched on demand | `data-nd-bind` GET |

There is no client-side cache of server state — when ndesign needs fresh data, it refetches. No Redux, no signals, no virtual DOM.

### CDN / runtime references

- Pinned spec for production: `https://storage.googleapis.com/ndesign-cdn/ndesign/v<semver>/SPEC.md`
- Mutable spec for active dev: `https://storage.googleapis.com/ndesign-cdn/ndesign/latest/SPEC.md`
- Bundle: `ndesign.min.js`, `ndesign.min.css`, optional `themes/light.min.css` and `themes/dark.min.css`.

For production, **always pin to a specific `v<semver>`** so your app does not silently upgrade when the CDN's `latest/` pointer moves.

### See also

- `ndesign-layouts` — the three canonical starting skeletons every page begins from.
- `ndesign-page-composition` — the meta-tag system that populates the store at init.
- `ndesign-data-binding` — the five primitives the runtime exposes.
- `go-mandatory-rules` — the No-Custom-CSS-or-JS rule that this philosophy enforces.

---

## 17. The Three Canonical Layouts
<!-- pattern-slug: ndesign-layouts -->

### Scope

This pattern defines the three canonical starting layouts — `control-panel`, `app-shell`, `blog` — and the mandatory ask-first rule for choosing between them. It includes the shared `<head>` and the three skeleton bodies verbatim, plus the layout-misuse list. It does NOT cover individual `data-nd-*` attributes used inside a layout — those are separate patterns.

### The Rule

ndesign ships **three canonical starting layouts**. Every new page begins from one of them. Picking the wrong skeleton later means rewriting the entire shell, so **agents MUST ask the user which of the three to start from before writing a single line of HTML.** Do not guess from the task description, do not default silently, do not invent a fourth. Ask.

| ID              | Best for                                                       | Key markers                                              |
|-----------------|----------------------------------------------------------------|----------------------------------------------------------|
| `control-panel` | Dashboards, admin UIs, data-heavy internal tools with a sidebar. | `.app-layout` + `.sidebar` + `.app-body` + top `<header>` |
| `app-shell`     | Multi-page SaaS apps with a fixed sidebar and per-page content. | `.sidebar.sidebar-fixed` + `.app-main`                   |
| `blog`          | Editorial content — posts, articles, docs, marketing copy.     | Top `<nav>` + `.nd-container` + `.nd-panel` + `.nd-prose` |

### The mandatory prompt

The required prompt to the user, before writing any markup:

> Which of the three starting layouts should this page use —
> **control-panel** (sidebar + scrollable content for a dashboard),
> **app-shell** (fixed sidebar for a multi-page SaaS app), or
> **blog** (centered prose panel for an article)?

Once the user picks, copy the matching skeleton verbatim and build inside it. Do NOT mix layouts. If the user's need truly does not fit one of the three, flag it and discuss — do not silently invent a hybrid.

### Shared `<head>`

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Page title</title>
    <link rel="stylesheet"
          href="https://storage.googleapis.com/ndesign-cdn/ndesign/v0.3.5/ndesign.min.css">
    <link rel="stylesheet"
          href="https://storage.googleapis.com/ndesign-cdn/ndesign/v0.3.5/themes/light.min.css"
          class="theme" data-theme="light">
    <meta name="nd-theme" content="light"
          data-href="https://storage.googleapis.com/ndesign-cdn/ndesign/v0.3.5/themes/light.min.css">
    <meta name="nd-theme" content="dark"
          data-href="https://storage.googleapis.com/ndesign-cdn/ndesign/v0.3.5/themes/dark.min.css">
    <meta name="endpoint:api" content="{{.APIBase}}">
    <meta name="csrf-token" content="{{.CSRFToken}}">
  </head>
  <!-- body goes here — pick ONE of the three skeletons below -->
  <script src="https://storage.googleapis.com/ndesign-cdn/ndesign/v0.3.5/ndesign.min.js"></script>
</html>
```

`{{.APIBase}}` and `{{.CSRFToken}}` are Go `html/template` placeholders rendered by the page handler.

### control-panel

Use for admin UIs, dashboards, operations consoles — any data-heavy application with persistent left navigation, a top header bar, and a scrollable content area.

```html
<body class="app-page">
  <div class="app-layout nd-h-screen nd-overflow-hidden">

    <!-- Sidebar -->
    <nav class="sidebar" id="app-sidebar">
      <span class="nd-nav-brand">AppName</span>
      <p class="nd-nav-section">Main</p>
      <ul class="nd-nav-menu">
        <li><a href="#" class="nd-active">Dashboard</a></li>
        <li><a href="#">Users</a></li>
      </ul>
    </nav>

    <!-- Main column: header + scrollable content -->
    <div class="app-body">
      <header>
        <div class="app-header-left">
          <button class="hamburger"
                  data-nd-toggle="sidebar"
                  aria-label="Toggle navigation">&#9776;</button>
          <h1 class="app-header-title">Dashboard</h1>
        </div>
        <div class="app-header-right">
          <button class="nd-btn-ghost nd-btn-sm" data-nd-theme-toggle>Toggle Theme</button>
        </div>
      </header>

      <main class="app-content">
        <!-- Page content. Use .nd-row / .nd-col-* for grids.
             Do NOT wrap in .nd-container. -->
      </main>
    </div>

  </div>
</body>
```

### app-shell

Use for multi-page SaaS apps where the sidebar is always visible and the page's primary content sits in a single main column. Simpler than `control-panel` (no top header bar reserved as a structural region).

```html
<body class="app-page">

  <!-- Fixed sidebar -->
  <nav class="sidebar sidebar-fixed">
    <span class="nd-nav-brand">AppName</span>
    <p class="nd-nav-section">Main</p>
    <ul class="nd-nav-menu">
      <li><a href="#" class="nd-active">Dashboard</a></li>
      <li><a href="#">Reports</a></li>
    </ul>
  </nav>

  <!-- Overlay for mobile sidebar toggle -->
  <div class="nd-nav-overlay"></div>

  <!-- Main content area — .app-main reserves the 16 rem sidebar gutter -->
  <div class="app-main">
    <!-- Optional top bar -->
    <nav class="nd-relative nd-mb-lg">
      <button class="nd-nav-toggle"
              aria-label="Toggle sidebar"
              data-nd-toggle="sidebar">&#9776;</button>
      <span class="nd-nav-brand">Page Title</span>
      <div class="nd-nav-end">
        <button class="nd-btn-ghost nd-btn-sm" data-nd-theme-toggle>Theme</button>
      </div>
    </nav>

    <!-- Page content. Do NOT wrap in .nd-container. -->
  </div>

</body>
```

### blog

Use for blog posts, articles, documentation, marketing copy, and similar long-form reading. This is the only layout that uses `.nd-container` and `.nd-prose`.

```html
<body class="app-page">

  <!-- Top nav (flush to viewport edges courtesy of .app-page) -->
  <nav>
    <a href="/" class="nd-nav-brand">Brand <span class="nd-nav-brand-sub">Journal</span></a>
    <ul class="nd-nav-menu">
      <li><a href="#" class="nd-active">Home</a></li>
      <li><a href="#">Archive</a></li>
    </ul>
    <div class="nd-nav-end">
      <button class="nd-btn-secondary nd-btn-sm" data-nd-theme-toggle>Theme</button>
      <button class="nd-btn-primary nd-btn-sm">Subscribe</button>
    </div>
  </nav>

  <!-- Centered 900 px column; the article sits on a floating .nd-panel -->
  <main class="nd-container nd-mt-lg nd-mb-2xl">
    <div class="nd-panel nd-shadow-lg">
      <article class="nd-prose nd-mx-auto">
        <h1>Article title</h1>
        <p class="nd-text-lead">Lead paragraph.</p>
        <p>Long-form body text…</p>
      </article>
    </div>
  </main>

</body>
```

### Layout misuse

The framework's full-width default exists for a reason. Common misuses, all of which MUST be avoided:

- **Do NOT wrap `control-panel` or `app-shell` content in `.nd-container`.** The narrow 900 px column is for prose only. Apply it to a dashboard and you waste horizontal space and break the grid.
- **Do NOT mix layouts.** Don't bolt a `.sidebar` onto a `blog` skeleton, don't drop a `.nd-prose` `<article>` into the `app-content` region of a `control-panel`, don't add `.app-layout` to a `blog`. Each skeleton's CSS assumes the structural elements it expects.
- **Do NOT invent a fourth canonical layout.** If none of the three fit, flag the case and discuss with the user before improvising. The three layouts cover dashboard, SaaS app, and editorial — most pages fit one of them.
- **Do NOT add custom `<style>` blocks to "tweak" a layout.** The framework is HTML-only. If a layout visibly needs adjustment, the framework is missing a utility class or modifier and the fix belongs in the framework, not the page.

### See also

- `ndesign-architecture` — the philosophy and lifecycle these layouts live inside.
- `ndesign-page-composition` — how the shared `<head>` meta tags drive the runtime store.
- `go-mandatory-rules` — the No-Custom-CSS rule that forbids the misuse cases above.

---

## 18. Page Composition & Store Setup
<!-- pattern-slug: ndesign-page-composition -->

### Scope

This pattern defines how each page is composed: the four meta-tag namespaces (`endpoint:*`, `var:*`, `csrf-token`, `nd-theme`), the `${var}` interpolation rule, the Go-side `RenderPage` helper, and the legitimate use cases for `NDesign.configure()`. It does NOT cover layout choice (see `ndesign-layouts`) or directive bindings (see `ndesign-data-binding`).

### Meta tag system

ndesign reads two meta-tag namespaces at init and populates two maps inside its store:

| Meta name            | Purpose                                                       |
|----------------------|---------------------------------------------------------------|
| `endpoint:NAME`      | Registers a URL base under `NAME`, resolvable via `${NAME}` in attribute URLs. |
| `var:NAME`           | Registers an initial scalar value under `NAME`, resolvable via `${NAME}`. |
| `csrf-token`         | Read by the runtime's `buildHeaders()`; sent as `X-CSRF-Token` on every fetch and upload. |
| `nd-theme`           | Registers a named theme; `data-href` points at its stylesheet. |

```html
<meta name="endpoint:api" content="https://api.example.com">
<meta name="endpoint:ws"  content="wss://api.example.com">
<meta name="var:userId"   content="2">
<meta name="var:pageSize" content="25">
<meta name="csrf-token"   content="REPLACE_WITH_SERVER_TOKEN">
```

The runtime auto-initialises on `DOMContentLoaded`. The JS tag MUST be at the end of `<body>` so the DOM is fully parsed before init runs.

### Server-side responsibility (Go)

The Go page handler renders the meta tags via `html/template`:

```go
type PageData struct {
    Title     string
    APIBase   string
    WSBase    string
    UserID    string
    CSRFToken string
}

func (s *Server) RenderPage(w http.ResponseWriter, r *http.Request, name string, data PageData) {
    if data.CSRFToken == "" {
        data.CSRFToken = csrf.Token(r)
    }
    if data.APIBase == "" {
        data.APIBase = s.cfg.APIBase
    }
    w.Header().Set("Content-Type", "text/html; charset=utf-8")
    if err := s.tpls.ExecuteTemplate(w, name, data); err != nil {
        log.Error().Err(err).Str("template", name).Msg("page render failed")
    }
}
```

```html
<!-- templates/users.html -->
<head>
  <meta name="endpoint:api" content="{{.APIBase}}">
  <meta name="endpoint:ws"  content="{{.WSBase}}">
  <meta name="var:userId"   content="{{.UserID}}">
  <meta name="csrf-token"   content="{{.CSRFToken}}">
</head>
```

The same template runs against staging or production by changing one meta tag (`endpoint:api`) — there is no rebuild.

### `NDesign.configure()` (when needed)

Most apps drive everything from meta tags. For options that aren't meta-tag-driven, call `NDesign.configure()` once before any element fetches. The most common reasons:

```html
<script src="https://storage.googleapis.com/ndesign-cdn/ndesign/v0.3.5/ndesign.min.js"></script>
<script>
  NDesign.configure({
    headers: { 'X-Client': 'my-app' },
    wsTokenProvider: () => sessionStorage.getItem('ws_token'),
    onError: (url, envelope, err) => {
      // Custom error handler. Default toasts the global message.
      console.warn('[fetch error]', url, envelope, err);
      NDesign.toast(envelope.errors.error || 'Something went wrong', 'error');
    },
  });
</script>
```

> The config script is the **one and only** place the rule "no `<script>` blocks beyond the runtime loader" admits an exception — and only for `NDesign.configure()`. Behaviour belongs in `data-nd-*` attributes, not in inline JS.

### See also

- `ndesign-architecture` — the lifecycle and the runtime store this pattern populates.
- `ndesign-layouts` — the shared `<head>` block in the layout patterns.
- `ndesign-websocket-integration` — the `wsTokenProvider` glue that goes inside `NDesign.configure()`.
- `go-mandatory-rules` — the No-Custom-JS rule and its single inline-script exception.

---

## 19. Data Binding Contract
<!-- pattern-slug: ndesign-data-binding -->

### Scope

This pattern defines the five primitive directives that drive everything in ndesign: `data-nd-bind`, `data-nd-action`, `data-nd-set`, `data-nd-model`, `data-nd-confirm`. It includes the form-serialisation rules and the success-chain action vocabulary. It does NOT cover form-error handling specifically (see `ndesign-forms`) or template rendering (see `ndesign-templates`).

The Go server only needs to think about these five primitives. Everything else in ndesign is built on them.

### 1. `data-nd-bind` — fetch JSON, render into element

```html
<!-- Scalar: write a single field into the element's textContent -->
<strong data-nd-bind="${api}/api/stats" data-nd-field="version">…</strong>

<!-- Template: render a JSON array into a <tbody> via a <template> -->
<tbody data-nd-bind="${api}/api/users"
       data-nd-template="user-row">
  <template id="user-row">
    <tr>
      <td>{{id}}</td>
      <td>{{name}}</td>
      <td>{{email}}</td>
    </tr>
  </template>

  <template data-nd-loading>
    <tr><td colspan="3"><span class="nd-skeleton"></span></td></tr>
  </template>

  <template data-nd-empty>
    <tr><td colspan="3">No users found.</td></tr>
  </template>

  <template data-nd-error>
    <tr><td colspan="3" class="nd-text-danger">Couldn't load users.</td></tr>
  </template>
</tbody>
```

**Go contract:**

- `GET ${api}/api/stats` returns the bare JSON object: `{"version": "1.4.2", ...}` via `RenderContent(w, stats)`.
- `GET ${api}/api/users` returns a bare JSON array via `RenderContent(w, users)`.
- For paginated lists: return `{"data":[...],"meta":{...}}` via `RenderPaginated`, and on the page set `data-nd-select="data"` to unwrap before rendering.

**Bind extras:**

- `data-nd-refresh="MS"` → poll the URL every MS milliseconds.
- `data-nd-defer` → don't fetch on init; wait for an external `nd:refresh` event.
- `data-nd-mode="append|prepend|replace"` → how new renders combine with existing children (default `replace`).
- `data-nd-max="N"` → drop oldest children when count exceeds N.
- `data-nd-params="key=val&..."` → appended to the URL query string.

### 2. `data-nd-action` — submit and process response

Works on both `<form>` (intercepts `submit`) and `<button>`/`<a>` (intercepts `click`).

```html
<!-- Form: body is JSON-serialised inputs -->
<form data-nd-action="POST ${api}/api/users"
      data-nd-success="reset, refresh:#user-table, toast:Created">
  <input name="name">
  <input name="email" type="email">
  <input name="address.city">     <!-- dot-path → nested object -->
  <button type="submit" class="nd-btn-primary">Create</button>
</form>

<!-- Button: body is the data-nd-body JSON template -->
<button data-nd-action="POST ${api}/api/orders"
        data-nd-body='{"sku":"${sku}","qty":${qty}}'
        data-nd-success="refresh:#orders">Place order</button>

<!-- Delete with native confirm -->
<button class="nd-btn-danger"
        data-nd-action="DELETE ${api}/api/users/${userId}"
        data-nd-confirm="Delete this user?"
        data-nd-success="refresh:#user-table">Delete</button>
```

**Form serialisation rules** (named, enabled, non-file inputs):

| Input | Serialised as |
|---|---|
| `type="checkbox"` | `boolean` (`el.checked`) |
| `type="radio"` | the value of the selected radio; unchecked skipped |
| `<select multiple>` | `Array<string>` of selected `value`s |
| `type="number"` / `"range"` | `Number`, or `null` if empty |
| everything else | `string` |

Dot-notation names produce nested objects: `name="address.city"` → `{ "address": { "city": "..." } }`. File inputs are skipped — use `data-nd-upload` for uploads.

**Success chain values** (`data-nd-success="action[,action]*"`):

| Action | Behaviour |
|---|---|
| `reset` | `form.reset()` (forms only) |
| `reload` | `window.location.reload()` |
| `redirect:URL` | `window.location.href = URL` |
| `refresh:SELECTOR` | dispatch `nd:refresh` on every matching element |
| `emit:EVENT` | dispatch a bubbling `CustomEvent` carrying the response data |
| `toast:MESSAGE` | success toast |
| `close-modal` | close the nearest ancestor `<dialog>` |

**Confirmation** — `data-nd-confirm` has two forms (see primitive 5 below).

### 3. `data-nd-set` — write to the store

Used for client-side scalar state (pager index, current view, selected ID) without round-tripping the server.

```html
<!-- Pager: +/- buttons mutate ${page}, then refresh the bound list -->
<button data-nd-set="page=${page}+1"
        data-nd-success="refresh:#user-list">Next</button>
<button data-nd-set="page=${page}-1"
        data-nd-success="refresh:#user-list">Prev</button>

<!-- Capture the create response under 'currentUser' -->
<form data-nd-action="POST ${api}/api/users"
      data-nd-set="currentUser, lastUserId=${currentUser.id}">
  …
</form>
```

Operations supported in the right-hand side: literals (`null`, `true`, `false`, numbers, single-quoted strings), `${var}` references, arithmetic on a referenced var (`${page}+1`), and the special `$response` token (full response body of a paired `data-nd-bind`/`data-nd-action`).

### 4. `data-nd-model` — two-way input ↔ store

The **only** reactive primitive in ndesign. When the user types/selects, the store updates. When `${name}` is written elsewhere (e.g. by a `data-nd-set` after a successful response), the input re-syncs.

```html
<input data-nd-model="searchQuery" placeholder="Search…">
<button data-nd-set="searchQuery=''"
        data-nd-success="refresh:#results">Clear</button>
```

> **Important:** Store writes do **NOT** auto-refresh `data-nd-bind` elements. Pair every store mutation that should refresh a view with an explicit `data-nd-success="refresh:#id"` or a manual `dispatchEvent(new CustomEvent('nd:refresh'))`.

### 5. `data-nd-confirm` — confirm before action

Two forms, dispatched by the leading character:

- **Plain text** → `window.confirm(text)` (synchronous, native).
- **Dialog selector** (`#dialog-id`) → custom `<dialog>` confirm; the action proceeds only if a button with `[data-nd-confirm-accept]` is clicked.

```html
<!-- Native browser confirm -->
<button class="nd-btn-danger"
        data-nd-action="DELETE ${api}/api/users/3"
        data-nd-confirm="Delete this user?">Delete</button>

<!-- Custom <dialog> confirm -->
<button class="nd-btn-danger"
        data-nd-action="DELETE ${api}/api/users/3"
        data-nd-confirm="#confirm-delete">Delete</button>

<dialog id="confirm-delete" class="nd-modal">
  <p>Delete this user? This cannot be undone.</p>
  <menu>
    <button type="button" data-nd-dismiss>Cancel</button>
    <button type="button" class="nd-btn-danger" data-nd-confirm-accept>Delete</button>
  </menu>
</dialog>
```

### See also

- `go-response-envelope` — the Go contract that `data-nd-bind` and `data-nd-action` consume.
- `ndesign-forms` — full form-handling pattern using `data-nd-action`.
- `ndesign-templates` — `<template>` mechanics referenced by `data-nd-bind`.
- `ndesign-page-composition` — `${var}` interpolation comes from the store populated by meta tags.

---

## 20. Forms
<!-- pattern-slug: ndesign-forms -->

### Scope

This pattern defines the end-to-end form flow: the markup conventions (`.nd-form-group`, `.nd-form-error`), the runtime's three-step error-mapping behaviour, the Go-side handler, and the single-field global-message synthesis rule. It does NOT cover validation tag mapping (see `ndesign-validation`) or the response envelope itself (see `go-response-envelope`).

### Markup

```html
<form data-nd-action="POST ${api}/api/users"
      data-nd-success="reset, refresh:#user-table, toast:User created">

  <div class="nd-form-group">
    <label for="user-name">Name <span class="nd-required">*</span></label>
    <input id="user-name" name="name" required minlength="3" maxlength="64">
    <div class="nd-form-error"></div>
  </div>

  <div class="nd-form-group">
    <label for="user-email">Email</label>
    <input id="user-email" name="email" type="email" required>
    <small class="nd-form-help">We never share your email.</small>
    <div class="nd-form-error"></div>
  </div>

  <div class="nd-form-group">
    <label for="user-role">Role</label>
    <select id="user-role" name="role" required>
      <option value="">Choose…</option>
      <option value="admin">Admin</option>
      <option value="editor">Editor</option>
      <option value="viewer">Viewer</option>
    </select>
    <div class="nd-form-error"></div>
  </div>

  <button type="submit" class="nd-btn-primary">Create user</button>
</form>
```

### How errors appear in the UI (the contract)

When the Go handler returns:

```json
HTTP/1.1 422 Unprocessable Entity
Content-Type: application/json

{ "errors": { "error": "Please correct the form.", "email": "already taken" } }
```

The ndesign runtime:

1. Adds `.nd-error` to the `<form>`.
2. Walks the `errors` object. For every key whose name matches a form input's `name=` attribute, it adds `.nd-error` to that input's enclosing `.nd-form-group` and writes the message into the sibling `.nd-form-error` div.
3. Writes the `errors.error` global message into a feedback alert. If `data-nd-feedback="ID"` is declared on the form, the message goes into that element. Otherwise, the runtime auto-creates an `.nd-alert nd-form-feedback-auto` slot immediately before the submit button on the first error and reuses it on subsequent submits.

### Go-side handler

```go
type CreateUserRequest struct {
    Name  string `json:"name"  validate:"required,min=3,max=64"`
    Email string `json:"email" validate:"required,email"`
    Role  string `json:"role"  validate:"required,oneof=admin editor viewer"`
}

func (s *Server) CreateUser(w http.ResponseWriter, r *http.Request) {
    var req CreateUserRequest
    if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
        RenderError(w, http.StatusBadRequest, "invalid request body")
        return
    }

    // Struct-tag validation
    if err := s.validate.Struct(req); err != nil {
        fields := mapValidationErrors(err) // map[string]string keyed by JSON name
        RenderFormErrors(w, http.StatusUnprocessableEntity,
            "Please correct the form.", fields)
        return
    }

    // Business uniqueness check
    if err := s.repo.Users.Create(r.Context(), &req); err != nil {
        if errors.Is(err, ErrDuplicateEmail) {
            RenderFieldErrors(w, http.StatusConflict, map[string]string{
                "email": "already taken",
            })
            return
        }
        RenderError(w, http.StatusInternalServerError, "failed to create user")
        return
    }

    RenderSuccess(w, "User created")
}
```

The symmetry is exact: every `name="email"` form input is matched by the `errors.email` key on the response; the `.nd-form-error` sibling div fills in.

### Form rules

- **Each input MUST have a `name` attribute.** Inputs without `name` are skipped by the serialiser and silently NOT submitted.
- **The `.nd-form-error` slot MUST be a sibling of the input inside the same `.nd-form-group`.** Errors target fields by traversing up to `.nd-form-group` and writing into the nearest `.nd-form-error`.
- **File inputs are skipped** by `data-nd-action`'s JSON serializer. Use `data-nd-upload` for any form that includes a file input (refer to the ndesign spec for the full upload component API).
- **`<input type="checkbox">` serialises as a boolean** (`true`/`false`), not as the `value` attribute.
- **`<input type="number">` serialises as a `Number`** (or `null` if empty), NOT as a string.

### Single-field global message synthesis

If the server returns ONLY field errors (no `errors.error` key), ndesign synthesises a global message:

- Exactly one field error → `"Please correct the highlighted field: <LABEL>"` where `<LABEL>` is read from the `<label for="...">` matching the input.
- Two or more field errors → `"Please correct the N highlighted fields below."`

So `RenderFieldErrors(w, 409, map[string]string{"email": "already taken"})` produces the message `"Please correct the highlighted field: Email"` automatically — provided the form uses the `<label for>` + input pattern shown above.

### See also

- `go-response-envelope` — the seven helpers, including `RenderFieldErrors` and `RenderFormErrors`.
- `ndesign-validation` — the Go `validate:` tag → HTML5 attribute mapping for the markup above.
- `ndesign-data-binding` — `data-nd-action` and the success chain.

---

## 21. Validation — Go Tags as Source of Truth
<!-- pattern-slug: ndesign-validation -->

### Scope

This pattern defines the validation contract: Go `validate:` struct tags are authoritative; HTML5 input attributes mirror them as a convenience layer. It includes the tag-to-attribute mapping table, the Go-type-to-input-type table, and a worked example. It does NOT cover error rendering — that is the **`ndesign-forms`** pattern.

### Rule

The Go `validate:` struct tags are the authoritative source. HTML5 native validation attributes are written by hand on the inputs and **mirror** those tags. The backend is the only enforcer; HTML5 attributes are a convenience layer that reduces obviously-bad submissions before the round-trip.

### Mapping table

| Go `validate` Tag | HTML5 attribute | Example |
|---|---|---|
| `required` | `required` | `<input required>` |
| `min=N` (string) | `minlength="N"` | `validate:"min=3"` → `<input minlength="3">` |
| `max=N` (string) | `maxlength="N"` | `validate:"max=64"` → `<input maxlength="64">` |
| `min=N` (number) | `min="N"` | `validate:"min=0"` → `<input type="number" min="0">` |
| `max=N` (number) | `max="N"` | `validate:"max=100"` → `<input type="number" max="100">` |
| `email` | `type="email"` | `<input type="email">` |
| `url` | `type="url"` | `<input type="url">` |
| `uuid` | `pattern="..."` | `<input pattern="[0-9a-fA-F-]{36}">` |
| `len=N` | `minlength="N" maxlength="N"` | `validate:"len=36"` → `minlength="36" maxlength="36"` |
| `oneof=a b c` | `<select>` with `<option value="a/b/c">` (or radio group) | see role example in the **`ndesign-forms`** pattern |

### Type mapping (Go → JSON → HTML)

| Go Type | JSON shape | Recommended input |
|---|---|---|
| `string` | string | `<input type="text">` (or `email` / `url` per validation) |
| `int`, `int64`, `float64` | number | `<input type="number">` |
| `bool` | boolean | `<input type="checkbox">` |
| `time.Time` | ISO-8601 string | `<input type="datetime-local">` (server parses) |
| `*string` / nullable | nullable string | text input; empty field maps to omitted/empty per backend choice |
| `[]T` | array | `<select multiple>` or repeated inputs |

### Example

```go
type SignupRequest struct {
    Email    string `json:"email"    validate:"required,email"`
    Password string `json:"password" validate:"required,min=12,max=72"`
    Age      int    `json:"age"      validate:"required,min=18,max=120"`
    Plan     string `json:"plan"     validate:"required,oneof=free pro enterprise"`
}
```

The matching ndesign markup:

```html
<div class="nd-form-group">
  <label for="signup-email">Email</label>
  <input id="signup-email" name="email" type="email" required>
  <div class="nd-form-error"></div>
</div>

<div class="nd-form-group">
  <label for="signup-password">Password</label>
  <input id="signup-password" name="password" type="password"
         required minlength="12" maxlength="72">
  <div class="nd-form-error"></div>
</div>

<div class="nd-form-group">
  <label for="signup-age">Age</label>
  <input id="signup-age" name="age" type="number"
         required min="18" max="120">
  <div class="nd-form-error"></div>
</div>

<div class="nd-form-group">
  <label for="signup-plan">Plan</label>
  <select id="signup-plan" name="plan" required>
    <option value="">Choose…</option>
    <option value="free">Free</option>
    <option value="pro">Pro</option>
    <option value="enterprise">Enterprise</option>
  </select>
  <div class="nd-form-error"></div>
</div>
```

### Optional convenience: a Go AST walker

A small Go AST walker can read the struct definitions and emit a `_validate.html` partial of the input attributes per struct, so changing `validate:"min=3"` to `validate:"min=5"` propagates automatically. This is **optional**, not mandatory. The framework treats the Go tag as the only enforcer; the HTML5 attribute is convenience.

```go
// scripts/validate-html-gen/main.go (illustrative skeleton)
// Walks types/*.go for exported request structs, reads json + validate tags,
// emits templates/_validate/<TypeName>.html with attribute snippets keyed by JSON name.
// Templates can include {{ template "_validate/CreateUserRequest.email" }} to inject
// `required type="email"` into an input.
```

### Why this approach (not Zod, not a heavy generator)

The previous-generation pattern was Go → Zod codegen + a React resolver. ndesign removes that toolchain entirely:

- HTML5 covers the common-case validators directly.
- The browser does the cheap up-front rejection.
- The server owns the truth and returns `RenderFormErrors` with field-keyed messages.
- The runtime maps those messages into `.nd-form-error` slots automatically.

No build pipeline, no schema duplication, no `npm install`. Add `validate:"min=5"` in Go, set `minlength="5"` on the input, ship.

### See also

- `ndesign-forms` — how the validation errors render on the page.
- `go-response-envelope` — `RenderFormErrors` and `RenderFieldErrors` carry the validation messages.
- `go-repository-pattern` — domain types in `types/` carry the `validate:` tags.

---

## 22. WebSocket Integration (`data-nd-ws`)
<!-- pattern-slug: ndesign-websocket-integration -->

### Scope

This pattern defines how a page subscribes to a WebSocket stream: the `data-nd-ws` directive, the URL-as-subscription-envelope rule, the RFC-6455 query-string token contract, the `wsTokenProvider` bootstrap glue, the reconnect-with-backoff behaviour, and the connection-state classes. It does NOT cover the server framework (see `go-websocket-system`) or the JWT issuance/verification (see `go-websocket-jwt-auth`).

### Markup

```html
<!-- Status indicator: bound element gets nd-ws-connected / nd-ws-disconnected -->
<div id="ws-status"
     class="nd-badge"
     data-nd-ws="${ws}/ws/feed"
     data-nd-field="type">connecting…</div>

<!-- Trade rows: filter messages by type=trade, prepend new ones -->
<tbody data-nd-ws="${ws}/ws/feed?channels=ladder,news,pnl,orders"
       data-nd-ws-filter="type:trade"
       data-nd-template="trade-row"
       data-nd-mode="prepend"
       data-nd-max="20">
  <template id="trade-row">
    <tr><td>{{ts}}</td><td>{{symbol}}</td><td>{{price}}</td></tr>
  </template>
</tbody>
```

### The URL is the subscription envelope

ndesign does NOT define a client-side subscribe frame. There is no `data-nd-ws-subscribe` directive, no init message the runtime sends on connect, no JSON-RPC handshake. **All subscription state — which feeds, channels, symbols, filters — is encoded in the URL itself** and read by the server on connect. Backends that expect a post-connect subscribe frame are working against the design.

Encode subscription parameters in the path (`/ws/account/42`), the query string (`?channels=ladder,news,pnl`), or both. Two elements that resolve to the same URL share one socket — multiplexing many channels onto a single connection is just "list them all in the same query string and let the server fan out the frames."

### Browser auth (the only path that works)

Browsers cannot set custom request headers on WebSocket upgrades — RFC 6455 / browser API constraint. ndesign's canonical browser auth path is the query-string token, set via `NDesign.configure({wsTokenProvider})`:

```html
<script>
  NDesign.configure({
    wsTokenProvider: () => sessionStorage.getItem('ws_token'),
  });
</script>
```

The provider's return value is URI-encoded and appended as `token=<value>` to the WS URL on every connect (including reconnects). **Backends MUST accept `token=<value>` as a query parameter for browser clients.** A backend that only accepts `Authorization` headers is unreachable from a browser regardless of which framework is in use.

For non-browser callers (server-to-server, CLI tools) where headers ARE settable, use those instead — that path does not pass through ndesign.

### Pairing with the Go JWT endpoint

The full client-side flow:

```html
<script>
  // 1. Fetch a fresh short-lived JWT, cache in sessionStorage.
  async function refreshWSToken() {
    const r = await fetch('/api/auth/token', { credentials: 'include' });
    const body = await r.json();
    sessionStorage.setItem('ws_token', body.token);
    // Schedule a refresh just before expiry.
    setTimeout(refreshWSToken, (body.expires_in - 30) * 1000);
  }

  // 2. Wire ndesign to read from sessionStorage on every WS connect.
  NDesign.configure({
    wsTokenProvider: () => sessionStorage.getItem('ws_token'),
  });

  // 3. Kick off the first token fetch BEFORE init runs WS bindings.
  refreshWSToken();
</script>
```

> This is the one acceptable use of an inline `<script>` block — bootstrap glue between the auth endpoint and `NDesign.configure()`. Everything else stays in `data-nd-*` attributes.

### Reconnect with backoff

On `close` (non-intentional), a reconnect timer fires after `retryDelay` ms. `retryDelay` starts at 1000, doubles on each attempt (plus up to 500 ms of jitter), and caps at 30000. On `open`, `retryDelay` is reset to 1000. No application code needed.

### Connection state classes

Every bound element is stamped with `nd-ws-disconnected` at init. On `open`, `nd-ws-disconnected` is removed and `nd-ws-connected` is added; on `close`, the reverse. Style these in your application's framework-extending CSS to show a status indicator.

### Per-element message filtering

`data-nd-ws-filter="FIELD:VALUE"` selects messages where `String(getByPath(msg, FIELD)) === VALUE`. Combined with shared sockets (multiple elements with the same URL), this lets one socket fan out to multiple `<tbody>` elements without duplicating the connection.

### See also

- `go-websocket-system` — the server-side framework and message protocol.
- `go-websocket-jwt-auth` — the `/api/auth/token` endpoint and upgrade verification.
- `ndesign-page-composition` — `NDesign.configure()` glue and the meta-tag-driven `${ws}` endpoint.
- `ndesign-templates` — `data-nd-template` and `data-nd-mode` referenced in the markup.

---

## 23. SSE Integration (`data-nd-sse`)
<!-- pattern-slug: ndesign-sse-integration -->

### Scope

This pattern defines `data-nd-sse` for one-way streaming: when to use SSE vs WebSockets, the default `append` render mode, the Go-side handler that writes `text/event-stream`, and the auth posture (cookies for same-origin, JWT for cross-origin). It does NOT cover bidirectional streaming — that is the **`ndesign-websocket-integration`** pattern.

### When to use SSE

`data-nd-sse="URL"` subscribes via the browser's native `EventSource` and renders each incoming message into the element. Use SSE for one-way streaming dashboards, log tails, live counters, notification feeds. Use WebSockets when the client also needs to send messages to the server.

### Markup

```html
<tbody data-nd-sse="${api}/api/events"
       data-nd-sse-event="trade"
       data-nd-template="trade-row"
       data-nd-mode="prepend"
       data-nd-max="50">
  <template id="trade-row">
    <tr><td>{{ts}}</td><td>{{symbol}}</td><td>{{price}}</td></tr>
  </template>
</tbody>
```

### Defaults

- **Default render mode is `append`** (not `replace`). Most SSE streams want growth, not replacement.
- `data-nd-max="N"` caps the rendered children — older messages are dropped when the count exceeds N.
- `data-nd-sse-event="TYPE"` filters to messages dispatched under that named SSE event (`event: TYPE` in the stream). If absent, the element renders only the unnamed default `message` event.
- Reconnect is handled natively by `EventSource` — no application code needed.

### Go-side handler

```go
func (s *Server) StreamEvents(w http.ResponseWriter, r *http.Request) {
    w.Header().Set("Content-Type", "text/event-stream")
    w.Header().Set("Cache-Control", "no-cache")
    w.Header().Set("Connection", "keep-alive")

    flusher, ok := w.(http.Flusher)
    if !ok {
        RenderError(w, http.StatusInternalServerError, "streaming unsupported")
        return
    }

    sub := s.events.Subscribe(r.Context())
    defer sub.Close()

    for {
        select {
        case <-r.Context().Done():
            return
        case ev := <-sub.Events():
            payload, _ := json.Marshal(ev.Data)
            fmt.Fprintf(w, "event: %s\ndata: %s\n\n", ev.Type, payload)
            flusher.Flush()
        }
    }
}
```

### Auth posture

The same JWT pattern from the **`ndesign-websocket-integration`** pattern applies if SSE needs auth — but since SSE runs over HTTP, the browser can send cookies natively, which is usually enough. JWT-via-query-string is reserved for cross-origin SSE where cookies don't apply.

### See also

- `ndesign-websocket-integration` — for bidirectional streaming and the JWT pattern.
- `ndesign-templates` — `data-nd-template`, `data-nd-mode`, `data-nd-max` mechanics.
- `go-response-envelope` — `RenderError` is still the failure helper for the streaming setup phase.

---

## 24. Templates & Rendering Primitives
<!-- pattern-slug: ndesign-templates -->

### Scope

This pattern defines ndesign's two interpolation systems (`${var}` vs `{{field}}`), the `<template>` element mechanism, the token grammar, conditional rendering with `data-nd-if`, the three render modes (`replace` / `append` / `prepend`), and the lifecycle templates (`data-nd-loading` / `-empty` / `-error`). It does NOT cover the directives that consume templates — see `ndesign-data-binding` for `data-nd-bind`, `ndesign-websocket-integration` for `data-nd-ws`, `ndesign-sse-integration` for `data-nd-sse`.

### Two interpolation systems

ndesign has **two** distinct interpolation systems. Mixing them is a frequent bug.

| Token | Where it works | Resolves against | When evaluated |
|---|---|---|---|
| `${var}` | URLs and bodies in `data-nd-bind`, `data-nd-action`, `data-nd-ws`, `data-nd-sse`, `data-nd-body`, `data-nd-set` RHS | Store (vars + endpoints) | At fetch/submit time |
| `{{field}}` | Inside `<template>` bodies referenced by `data-nd-template` | The current row of the response | At render time |

**Do NOT mix them.** `{{field}}` does not work in URLs. `${var}` does not work inside templates.

### `<template>` elements

Templates are real `<template>` elements referenced by `id`. Rendering clones the template's content and walks all text nodes and attributes, replacing `{{path}}` tokens.

```html
<template id="user-row">
  <tr>
    <td>{{id}}</td>
    <td>{{name}}</td>
    <td data-nd-if="active">
      <span class="nd-badge nd-badge-success">Active</span>
    </td>
    <td data-nd-if="active" hidden>
      <span class="nd-badge">Inactive</span>
    </td>
  </tr>
</template>
```

### Token grammar

```
\{\{(\s*[\w.]+\s*)\}\}
```

- The path is `\w.` only — no pipes, no filters, no defaults. **There is NO `{{field|default}}` syntax.**
- Dot paths are supported: `{{user.profile.name}}`.
- Missing values yield the empty string in text nodes and `''` in attribute values.
- Text-node substitutions use `textContent` — the browser handles escaping.
- Attribute substitutions are HTML-escaped before assignment.

### Conditional rendering: `data-nd-if`

An element inside a template carrying `data-nd-if="FIELD"` is **removed from the rendered clone** when the named field is falsy.

### Render modes

`data-nd-mode` controls how template renders are inserted:

| Mode | Behaviour |
|---|---|
| `replace` | (default) Removes all non-`<template>` children, then appends. |
| `append` | Appends after existing children. |
| `prepend` | Inserts before the first non-template child. |

`data-nd-max="N"`: after each render, drop the oldest until the count is N. "Oldest" is the first child for `append`/`replace`, the last child for `prepend`.

### Loading / empty / error templates

Three direct-descendant `<template>` markers cover the bound element's lifecycle:

- `<template data-nd-loading>` — clone inserted while fetch is in flight; container also gets the `nd-loading` class.
- `<template data-nd-empty>` — fires only when the rendered data (after `data-nd-select`) is an array of length 0.
- `<template data-nd-error>` — when a bind fetch fails, the synthesised error envelope is available and the template is cloned into the container, replacing all non-template children. `.nd-error` is also added. If no error template is present, `config.onError(url, envelope, err)` is invoked instead.

These templates are matched by attribute, not `id` — they MUST be direct descendants of the bound element and MUST NOT be referenced by `id`.

### See also

- `ndesign-data-binding` — `data-nd-bind` and `data-nd-template` mechanics.
- `ndesign-websocket-integration` — `data-nd-ws` consumes the same template system.
- `ndesign-sse-integration` — `data-nd-sse` consumes the same template system.
- `ndesign-page-composition` — where `${var}` resolves from (the meta-tag-driven store).

---

# Full-Stack

## 25. Dev Workflow — air
<!-- pattern-slug: fullstack-dev-workflow-air -->

### Scope

This pattern defines the local dev loop and production build: a single Go binary serves HTML + JSON + streaming endpoints, `air` rebuilds on `.go`/`.html` change, and `go:embed` packs templates for production. There is no Vite, no Node, no `npm install`. It does NOT cover deployment topology beyond "run the static binary."

### Architecture

A **single Go binary** serves three things:

1. HTML pages rendered with `html/template`.
2. `/api/*` JSON handlers.
3. `/ws/*` WebSocket and `/sse/*` SSE endpoints.

There is no Vite, no Node, no `npm install`, no `go2zod`, no Zod. The ndesign runtime is loaded from CDN at page load; there is nothing to build on the frontend. `air` watches `*.go` and `*.html` files and recompiles the Go binary on change.

```
File change detected (*.go, *.html)
    ↓
air rebuilds: go build -o ./tmp/main ./cmd
    ↓
air restarts ./tmp/main
    ↓
Go server serves: HTML pages + /api/* + /ws/* + /sse/* + /metrics
                  ndesign loads from CDN at page load
```

### `.air.toml`

```toml
root = "."
tmp_dir = "tmp"

[build]
  cmd = "go build -o ./tmp/main ./cmd"
  bin = "./tmp/main"
  delay = 500
  include_ext = ["go", "html", "yaml"]
  exclude_dir = ["tmp", ".git", "static"]
  kill_delay = 500

[log]
  time = true

[misc]
  clean_on_exit = true
```

That's the entire build script. There is no `build.sh` because there is nothing to build on the frontend.

### Production deployment

For production, use `go:embed` to bundle the `templates/` directory into the binary:

```go
//go:embed templates/*.html templates/**/*.html
var templateFS embed.FS

func loadTemplates() *template.Template {
    return template.Must(template.ParseFS(templateFS, "templates/*.html", "templates/**/*.html"))
}
```

The result is a single static binary that includes every page template. The ndesign runtime is still loaded from CDN at runtime — pin to a `v<semver>` URL so the version never moves. If CDN access is unacceptable (air-gapped deployments, strict CSP), download the four files (`ndesign.min.js`, `ndesign.min.css`, `themes/light.min.css`, `themes/dark.min.css`) and serve them from the same Go binary; the only thing that changes is the URL in the `<link>` and `<script>` tags.

### Why this works

| Concern | Solution |
|---|---|
| Go changes | air detects `.go` → rebuilds and restarts |
| HTML changes | air detects `.html` → rebuilds (templates are loaded from disk in dev or `embed.FS` in prod) |
| Frontend changes | edit the `.html` template — ndesign hydrates on next page load |
| Type drift | gone — the API response is the source of truth; pages consume bare JSON |
| No dev server | Go serves everything — no CORS, no proxy |
| No caching issues | air rebuilds on every change |
| Fast iteration | `go build` ~2s — that's the entire cycle |

### See also

- `go-project-layout` — the directory shape `air` watches.
- `go-entry-point-lifecycle` — the binary `air` rebuilds.
- `ndesign-architecture` — the CDN-loaded runtime that needs nothing to build.

---

## 26. Front-to-Back Symmetry
<!-- pattern-slug: fullstack-symmetry-table -->

### Scope

This pattern is the single-page mapping table between every Go backend concept and its ndesign frontend counterpart. It exists to make the contract memorable: an engineer who learns one side immediately understands the other. It does NOT teach any concept in depth — for that, follow the linked pattern slugs.

### The principle

> The frontend learns the logic once — it applies to both sides of the stack.
>
> An engineer who understands `RenderFormErrors` immediately understands how `data-nd-action` shows validation errors.
> An engineer who understands BigCache's TTL immediately understands why `nd:refresh` is the only way to bust it.
> An engineer who adds `validate:"min=5"` to a Go struct only needs to update one HTML5 attribute on the input — both layers enforce, the backend is authoritative.

### Symmetry table

| Concern | Go backend | ndesign frontend |
|---|---|---|
| **Identity** | `AuthMiddleware` returns 401 on missing session | The browser handles 401 at the page level (server-rendered redirect or server-set cookie). There is no client-side router. |
| **Authorization** | `rbac.Check(session, action, level)` | UI visibility via server-rendered conditionals in `html/template`. The backend remains the only enforcer. |
| **Validation** | `validate:` struct tags | HTML5 input attributes mirror the Go tags |
| **Form submit** | Handler decodes JSON, validates, returns `RenderFormErrors` / `RenderFieldErrors` | `<form data-nd-action>` JSON-serialises inputs and submits; runtime maps errors to `.nd-form-error` siblings automatically |
| **List GET** | `RenderContent(w, items)` | `<tbody data-nd-bind data-nd-template="row">` |
| **Paginated GET** | `RenderPaginated(w, items, meta)` | `<tbody data-nd-bind data-nd-select="data" data-nd-template="row">` |
| **Action success** | `RenderSuccess(w, "Created")` | runtime shows the `message` in the `nd-alert nd-alert-success` feedback slot |
| **Global error** | `RenderError(w, 403, "forbidden")` | runtime shows `errors.error` in `nd-alert nd-alert-error` (or routes to `<template data-nd-error>`) |
| **Field errors** | `RenderFieldErrors(w, 422, fields)` | runtime maps each key to the matching input's `.nd-form-error` |
| **Combined errors** | `RenderFormErrors(w, 422, msg, fields)` | runtime shows `msg` globally + per-field errors inline |
| **Caching** | BigCache + singleflight (server) | None — ndesign refetches on `nd:refresh` |
| **WebSocket auth** | Short-lived JWT via `/api/auth/token`; verify on upgrade | `wsTokenProvider` returns the token; runtime appends `?token=...` query param |
| **WebSocket subscription** | Read channels/topics from URL on upgrade | `data-nd-ws` URL carries channels in path or query string |
| **SSE auth** | Cookies (same-origin) or query token (cross-origin) | `EventSource` natively sends cookies; no client config needed for same-origin |
| **Streaming render** | `text/event-stream` with `event:` + `data:` lines | `data-nd-sse` + `data-nd-template`, default mode `append`, `data-nd-max` cap |
| **CSRF** | Read `X-CSRF-Token` header in mutating handlers | runtime sends `<meta name="csrf-token">` on every fetch |
| **Observability** | OTel traces + zerolog + Prometheus `/metrics` | `X-Trace-ID` response header for correlating browser-side errors |
| **Metrics surfacing** | Prometheus scrape | Grafana dashboards / alerts |

### See also

- `go-response-envelope` — the seven Go helpers that anchor the table's middle rows.
- `ndesign-forms` — the frontend half of the form-submit row.
- `go-websocket-jwt-auth` + `ndesign-websocket-integration` — the WebSocket auth row, both halves.
- `go-cache-singleflight` — the caching row.
- `go-rbac-middleware` — the identity and authorization rows.

---

## Appendix A: Architectural Principles — Quick Reference
<!-- pattern-slug: none -->

This appendix aggregates the architectural principles that cross-cut every pattern in the catalog. Each row's full treatment lives in the pattern referenced in the rightmost column.

### Backend

| Principle | Implication | See pattern |
|---|---|---|
| `main()` owns lifecycle | Sub-packages return `(T, error)`; never `log.Fatal()` outside `main`. | `go-entry-point-lifecycle` |
| Graceful shutdown is mandatory | `http.Server.Shutdown()` + `signal.NotifyContext`; zero dropped requests on rolling deploy. | `go-entry-point-lifecycle` |
| Session in `context.Context` | Handlers use the standard `func(w, r)` signature; session via `web.GetSession(r)`. | `go-rbac-middleware` |
| Repository interfaces per domain | Handler depends on `repo.UserRepo`, not a monolithic `Store`. | `go-repository-pattern` |
| Cached decorators wrap repos | The handler calls the same interface; cache + singleflight live behind it. | `go-repository-pattern` |
| Composition root in `Store` | All driver/cache wiring is one function; degradation tiers wired once at startup. | `go-repository-pattern` |
| Trace correlation everywhere | Every repo log entry, every error wrap carries `trace_id`; `X-Trace-ID` returned to browser. | `go-observability-otel` |
| Metrics use route templates, not paths | `/api/users/{id}` not `/api/users/3` — bounded cardinality. | `go-prometheus-metrics` |
| Prometheus on `/metrics`, no auth | Scrapers need unauthenticated access. | `go-prometheus-metrics` |
| `validate:` tags are the source of truth for shape | Backend is the only enforcer; HTML5 attributes are the convenience layer. | `ndesign-validation` |

### Frontend

| Principle | Implication | See pattern |
|---|---|---|
| Vanilla HTML + one CDN bundle | No build step on the frontend; pin to `v<semver>` for production. | `ndesign-architecture` |
| No custom CSS / JS in pages | If you need it, the framework is missing it — extend the framework. | `go-mandatory-rules` |
| Three layouts, ask before writing | Pick one of `control-panel` / `app-shell` / `blog` before writing markup. | `ndesign-layouts` |
| URLs live on the element | Each `data-nd-*` carries its own URL; DRY via `<meta name="endpoint:NAME">` and `${NAME}/...`. | `ndesign-page-composition` |
| Server is authoritative | The page renders meta tags + initial HTML; ndesign hydrates lazily. | `ndesign-architecture` |
| Five primitives for everything | `bind`, `action`, `set`, `model`, `confirm`. The rest is components. | `ndesign-data-binding` |
| Two interpolation systems | `${var}` for attribute URLs/bodies; `{{field}}` inside templates. Don't mix. | `ndesign-templates` |
| One reactive primitive (`data-nd-model`) | Store writes do NOT auto-refresh `data-nd-bind`; pair with `refresh:#id`. | `ndesign-data-binding` |
| The URL is the WS subscription envelope | No client-side subscribe frame; encode channels in the URL. | `ndesign-websocket-integration` |
| Browser WS auth is query-string token | RFC 6455 forbids browser headers on upgrade; use `wsTokenProvider`. | `go-websocket-jwt-auth` |

### Operational

| Principle | Implication | See pattern |
|---|---|---|
| Three pillars of observability | zerolog (logs), OpenTelemetry (traces), Prometheus (metrics). | `go-logging-zerolog`, `go-observability-otel`, `go-prometheus-metrics` |
| Health endpoints | `/health` (liveness), `/ready` (readiness) — no auth. | `go-router-chi` |
| Graceful degradation by tier | `Store.New` decides feature levels at startup based on dependencies present. | `go-repository-pattern` |
| Two-tier config | Bootstrap YAML (≤4 fields) for DB connection; runtime config in DB. | `go-config-two-tier` |
| Critical config requires confirmation | `IsCritical()` gate before persisting destructive changes. | `go-config-two-tier` |

---

## Appendix B: Source Files Reference
<!-- pattern-slug: none -->

This appendix maps the canonical file/package layout to the patterns that govern each file. It is a navigation aid; for the rules each file embodies, follow the right-hand pattern slug.

### Backend

| File | Package | Key contents | Pattern |
|---|---|---|---|
| `cmd/main.go` | `main` | Entry point, signal handling, server construction, graceful shutdown | `go-entry-point-lifecycle` |
| `web/server.go` | `web` | `Server` struct, `NewServer`, `Router()` | `go-router-chi` |
| `web/routes.go` | `web` | `registerRoutes()`, route domain orchestration | `go-router-chi` |
| `web/response.go` | `web` | `RenderJSON`, `RenderContent`, `RenderPaginated`, `RenderSuccess`, `RenderError`, `RenderFieldErrors`, `RenderFormErrors` | `go-response-envelope` |
| `web/auth_middleware.go` | `web` | `AuthMiddleware`, `GetSession`, session-in-context | `go-rbac-middleware` |
| `web/page.go` | `web` | `RenderPage`, `html/template` orchestration, CSRF/CSRF-meta injection | `ndesign-page-composition` |
| `web/handlers/<domain>/` | per-domain | Domain handler functions (`func(w, r)` signatures) | `go-router-chi` + `go-response-envelope` |
| `web/ws/upgrade.go` | `ws` | `HandleWSUpgrade` — the integration handler the router wires | `go-websocket-jwt-auth` |
| `web/ws/token.go` | `ws` | `HandleTokenRequest` — short-lived JWT issuance for `wsTokenProvider` | `go-websocket-jwt-auth` |
| `web/wsock/wsock.go` | `wsock` | `SocketManager`, `ClientInfo`, heartbeat, message protocol | `go-websocket-system` |
| `web/wsock/manager.go` | `wsock` | Broadcast/fan-out, per-channel routing | `go-websocket-system` |
| `web/wsock/queues.go` | `wsock` | Queue interface, factory function | `go-queue-system` |
| `web/wsock/qproviders/` | per-provider | Pub/Sub, SQS, Redis Streams adapters | `go-queue-system` |
| `auth/` | `auth` | Session validation, JWT signing/verification | `go-rbac-middleware` + `go-websocket-jwt-auth` |
| `rbac/` | `rbac` | `Check(session, action, level)`, role definitions, `CanDo()` | `go-rbac-middleware` |
| `repo/` | `repo` | Domain repository interfaces (`UserRepo`, `OrderRepo`, ...) | `go-repository-pattern` |
| `storage/store.go` | `storage` | `Store` composition root, `NewStore` factory | `go-repository-pattern` |
| `storage/<driver>/` | per-driver | sqlc-generated queries + interface implementations | `go-repository-pattern` |
| `storage/cached_*.go` | `storage` | Cache decorators wrapping inner repos | `go-repository-pattern` + `go-cache-singleflight` |
| `cache/cache.go` | `cache` | `CacheService`, `GetOrFetch` (singleflight), `Invalidate` | `go-cache-singleflight` |
| `config/bootstrap.go` | `config` | `LoadBootstrap()` — minimal YAML reader | `go-config-two-tier` |
| `config/store.go` | `config` | `ConfigStore`, `Resolve*()` typed accessors, change events | `go-config-two-tier` |
| `metrics/middleware.go` | `metrics` | HTTP golden signals middleware | `go-prometheus-metrics` |
| `metrics/application.go` | `metrics` | Application gauges/counters | `go-prometheus-metrics` |
| `dry/uuid.go` | `dry` | `dry.GenerateUUID` | `go-dry-package` |
| `dry/hashes.go` | `dry` | `dry.GenerateBCryptHash`, `dry.ValidateBearerToken` | `go-dry-package` |
| `dry/slices.go` | `dry` | `dry.Difference`, `dry.RemoveFromSlice`, `dry.GetAddRem` (generics) | `go-dry-package` |
| `dry/strings.go` | `dry` | `dry.Contains`, `dry.JsonOrEmpty` | `go-dry-package` |
| `dry/templates.go` | `dry` | `dry.StringFromTemplate`, `dry.MergeMap` | `go-dry-package` |
| `types/` | `types` | Domain POGOs (the only types that cross package boundaries) | `go-repository-pattern` |

### Frontend

| File | Purpose | Pattern |
|---|---|---|
| `templates/_base.html` | Shared `<head>` + meta tags + script loader | `ndesign-layouts` + `ndesign-page-composition` |
| `templates/<domain>/list.html` | `data-nd-bind` to `/api/<domain>` + `<template>` for rows | `ndesign-data-binding` + `ndesign-templates` |
| `templates/<domain>/edit.html` | `<form data-nd-action="POST/PATCH ${api}/api/<domain>">` | `ndesign-forms` |
| Pinned ndesign CDN | `https://storage.googleapis.com/ndesign-cdn/ndesign/v<semver>/ndesign.min.js` | `ndesign-architecture` |
| Pinned ndesign CSS | `https://storage.googleapis.com/ndesign-cdn/ndesign/v<semver>/ndesign.min.css` | `ndesign-architecture` |
| Pinned theme stylesheets | `.../themes/light.min.css`, `.../themes/dark.min.css` | `ndesign-layouts` |
| Authoritative spec | `https://storage.googleapis.com/ndesign-cdn/ndesign/v<semver>/SPEC.md` | `ndesign-architecture` |

### Build & Tooling

| File | Purpose | Pattern |
|---|---|---|
| `.air.toml` | air config — `go build` only, no frontend pipeline | `fullstack-dev-workflow-air` |
| `go.mod` / `go.sum` | Go dependencies | `fullstack-dev-workflow-air` |
| `migrations/*.sql` | golang-migrate atomic migrations | `go-repository-pattern` |
| `prometheus.yml` | Scrape config (or `ServiceMonitor` for k8s) | `go-prometheus-metrics` |

---

[↑ Back to Top](#develop-with-go--authoritative-guide) | [← Back to Docs Index](./README.md)
