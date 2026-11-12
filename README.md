# nodus-client-sdk

The client SDK for Nodus and the fediverse: one state machine, one shared model,
and protocol adapters for ActivityPub, Mastodon and Misskey. It knows nothing about
how a client looks.

The client is deliberately split in two:

| Project | Owns |
| --- | --- |
| `sdk` (this repository) | Session, timelines, feed, streaming status, cache policy, transport traits, the Mastodon domain types |
| `web` | Rendering that state, and turning user input into commands |
| `recommender` | Recommendations, sync, storage |

The web client renders SDK state and dispatches commands. It does not fetch, cache,
paginate or decide *what* to show — otherwise the same logic would exist twice (once
for the browser, once for the sync worker) and drift.

## Crates

| Crate | Purpose |
| --- | --- |
| `nodus-shared` | Wire contract with our recommendation API (`AccountFeed`, `EventBatch`, `Meta`, errors). No I/O, no async, compiles for `wasm32` and `x86_64`. |
| `nodus-client-core` | Implementation-agnostic model (`Actor`, `Post`, `Notification`, `Page`, …), the `InstanceApi`/`SyncApi` traits, and the command vocabulary. Mastodon's `Account`/`Status` names remain aliases. |
| `nodus-mastodon` | Mastodon REST compatibility (`/api/v1`, `/api/v2`) plus native and browser transports. |
| `nodus-misskey` | Misskey compatibility mapped onto the same core traits. |
| `nodus-protocol` | The Nodus extensions on top of Mastodon: capability negotiation, event subscriptions with semantic kinds, batch requests. No I/O, no state. |
| `nodus-client-sdk` | The state machine: [`Sdk<I, D, P, C>`], [`AppState`], [`Command`], and the ports it runs on. |

## Two surfaces, one client

`nodus-mastodon` is the **compatibility floor**: it speaks Mastodon 4.3 and nothing
else, so the client works against any instance in the fediverse. `nodus-protocol` is the
**ceiling**: what our own server adds (typed events for likes and edits, batch requests,
recommendations).

The rule the SDK follows is *prefer the optimized path, always keep the compatible one*:
the client reads `/api/v1/meta` once per session into `Capabilities`, states what a screen
needs (`Need::none().events().batch()`), and `choose` answers with the surface to use plus
the fallback that always works. A screen never mixes the two — half-conditional state is
worse than one round trip.

## Ports (traits) and generics

`Sdk` is generic over four ports, so the same logic runs in the browser, in native
tests, and in tooling:

```rust
pub struct Sdk<I, D, P, C> { /* ... */ }

//  I: InstanceApi — the Mastodon instance surface
//  D: Discovery   — our recommendation API
//  P: Persistence — cache (memory, IndexedDB, …)
//  C: Clock       — time, so tests are deterministic
```

| Port | Methods (summary) | wasm implementation | native implementation |
| --- | --- | --- | --- |
| `InstanceApi` | timelines, notifications, status/context, account, follow, post, delete | `gloo-net` (M1) | `reqwest` (M1, sync worker) |
| `Discovery` | `meta`, `feed`, `refresh`, `send_events`, `set_sync` | `gloo-net` (M1) | `reqwest` (M1) |
| `Persistence` | `load`, `store`, `remove`, `clear` over bytes | IndexedDB (M1) | memory / filesystem (tests) |
| `Clock` | `now_unix_millis` | system clock | `FixedClock` in tests |

Two ports for two planes on purpose: the instance is the user's data plane
(failures break timelines) and `Discovery` is our discovery plane (failures only
weaken the Feed tab). Keeping them separate is why
`SdkError::Instance` and `SdkError::Discovery` are distinct and why
`SdkError::is_discovery_only()` can exist.

[`InstanceApi`]: crates/client-core/src/api.rs
[`Sdk<I, D, P, C>`]: crates/client-sdk/src/sdk.rs
[`AppState`]: crates/client-sdk/src/state.rs
[`Command`]: crates/client-sdk/src/command.rs

## State model

State is a tree of `Handle<T>` — `Rc<RefCell<T>>` plus a subscriber list:

```text
AppState
├── session:  Handle<Option<Session>>       // host, account id, acct, scopes — never the token
├── stream:   Handle<StreamState>           // Disconnected | Connecting | Connected | Reconnecting{..} | Stopped
├── feed:     Handle<FeedState>             // items, cursor, generated_at, degraded, dismissed
└── timelines:Handle<HashMap<TimelineKey, TimelinePage>>
```

Rules that keep it honest:

1. **One mutation path.** Views call `Sdk::dispatch(Command)`. There is no other
   way to change state, so "what can happen" is one enum, and every transition is
   testable without a browser.
2. **No framework types.** `Handle` is deliberately not a Leptos signal: this crate
   must compile for native tests. `web/crates/web/src/state/bridge.rs` adapts a
   handle to a revision signal in about forty lines.
3. **No tokens in state.** The token lives in the client's secret storage and is
   handed to the transport; a view that could read it is a view that could leak it.
4. **No ranking math.** Ordering comes from the server. The client may dismiss,
   paginate and explain — not score.
5. **No I/O in `dispatch`.** It is synchronous, so a click can change the UI on the
   same frame.

## How consumers depend on this repository

The three projects are submodules of one superproject (`Nodus`), so this
checkout always sits at `../sdk` relative to `recommender` and `web`. Consumers use
plain path dependencies:

```toml
nodus-shared      = { path = "../sdk/crates/shared",      version = "0.1.0" }
nodus-client-sdk  = { path = "../sdk/crates/client-sdk",  version = "0.1.0" }
nodus-mastodon    = { path = "../sdk/crates/mastodon",    version = "0.1.0" }
```

Why paths and not `git = "https://…"`:

* the superproject pins the exact commit of this repository (a gitlink), so a
  `Cargo.lock` plus a gitlink is one reproducible state, not two;
* no second copy of the sources in `~/.cargo/git`, so `clippy` in the SDK and
  `clippy` in the client see the same files;
* edits across the boundary (SDK change + client change) are one commit set instead
  of an unmerged pair waiting on a push;
* offline builds and CI caching stay simple.

Git dependencies become the right answer when an *external* consumer appears that
is not part of this superproject; see [03-contract.md](docs/03-contract.md) for the
pinning and versioning rules that already cover that case.

## Building

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace --target wasm32-unknown-unknown
```

The `wasm32` check matters: this repository is compiled for both targets, and a
dependency that only builds on one of them is a bug.

## Status

M0 (this commit): state, commands, ports, error model, and the pure state
transitions, with tests. Transports, streaming, IndexedDB and the sync of feed
impressions arrive in M1/M2; see [docs/04-roadmap.md](docs/04-roadmap.md).

See [docs/03-contract.md](docs/03-contract.md) for the contract and versioning
rules, [docs/02-architecture.md](docs/02-architecture.md) for the state model, and
[`web/docs/03-data-layer.md`](../web/docs/03-data-layer.md) for how the client uses
the ports.

## Trademark

Nodus is not affiliated with, endorsed by, or sponsored by Mastodon GmbH.
"Mastodon" is a trademark of Mastodon GmbH and is used in this repository only
to describe compatibility; it is deliberately absent from the crate name
(`nodus-client-core`, not `mastodon-*`). See
[`recommender/docs/11-trademark.md`](../recommender/docs/11-trademark.md).
