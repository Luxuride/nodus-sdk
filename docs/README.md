# Design documentation — SDK

Status: **draft** — written before implementation, expected to change as the
first vertical slice teaches us things. Each document carries its own status line.

`sdk` is a repository, not a service. It is both the **contract** between the
recommender and the client and the **source of truth for client state**: the wire
types both sides speak, the instance surface transports implement, and the state
machine the web client renders. Three crates, and nothing else:

| Crate | Purpose |
| --- | --- |
| `nodus-shared` | our wire DTOs (`AccountFeed`, `EventBatch`, `Meta`, error envelope), `CONTRACT_VERSION`. No I/O, no async, compiles for `wasm32` and `x86_64`. |
| `nodus-instance` | Mastodon domain model (`Status`, `Account`, `Notification`, `Page`, …) and `InstanceApi`, the instance surface transports implement. |
| `nodus-sdk` | the client state machine: `AppState`, `Command`, `Sdk<I, D, P, C>` and its four ports. No framework dependency. |

## Documents

| Doc | Contents |
| --- | --- |
| [01-scope.md](01-scope.md) | What belongs in each crate, what does not, the five invariants |
| [02-architecture.md](02-architecture.md) | The three crates, the four ports, `Sdk<I, D, P, C>`, `Command`, the state tree |
| [03-contract.md](03-contract.md) | Contract versioning, coexistence, pinning via submodules, compatibility CI, failure modes |
| [04-roadmap.md](04-roadmap.md) | The SDK-facing slice of the milestones, and what is deferred |
| [05-ports.md](05-ports.md) | The four ports, what a wrong implementation can and cannot break, and how to add one |

## Where the rest of the design lives

The three projects are submodules of one superproject, so they always sit at these
relative paths:

```text
Nodus/            ← superproject: .devcontainer, .gitmodules, README
├── sdk/          nodus-shared, -instance, -sdk · docs/ ← you are here
├── recommender/      core, store, ingest, api
└── web/          the Leptos client
```

- Server-side design — schema, ingestion, similarity, ranking, the HTTP API we
  serve: [`recommender/docs/`](../../recommender/docs/README.md).
- UI design — rendering, routes, sanitisation, quality budget, the parity
  matrix: [`web/docs/`](../../web/docs/README.md).

This repository is the seam between them: the wire format in
[03-contract.md](03-contract.md) is what the recommender serves and the client
consumes, and the state model in [02-architecture.md](02-architecture.md) is what
the client renders.

## Conventions

- Markdown, one H1 per file, numbered files, `Status:` line at the top.
- Cross-link with relative paths; never duplicate a table that lives elsewhere —
  link to it.
- Design changes land as a doc change first, then code.
- A statement about "the instance" means a third-party Mastodon-compatible server
  we do not control; treat its responses as untrusted input.
