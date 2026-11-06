# 02 — Architecture

Status: draft · Last revised: 2026-11-06

The state machine and the ports it runs on. Everything here is in
`crates/sdk/src/`; the boundary rules are in [01-scope.md](01-scope.md), and the
wire types the `Discovery` port carries are in [03-contract.md](03-contract.md).

## Two planes, one state machine

```text
                    ┌──────────────────────────────┐
   browser input ──▶│  Sdk::dispatch(Command)      │──▶ AppState (Handles)
                    └──────────────────────────────┘         │
                              ▲                              │ subscribe
        async methods ────────┘                              ▼
   sign_in / load_timeline / load_feed / …            views render state
        │                │
        │                └── Discovery  ──▶ our API         (discovery plane)
        └────────────────── InstanceApi ──▶ the user's instance (data plane)
                                    ▲
                              Streaming
```

Two ports for two planes on purpose: the instance is the user's data plane
(failures break timelines) and `Discovery` is our discovery plane (failures only
weaken the Feed tab). Keeping them separate is why `SdkError::Instance` and
`SdkError::Discovery` are distinct and why `SdkError::is_discovery_only()` can
exist.

## The four ports

`Sdk` is generic over four ports, so the same logic runs in the browser, in native
tests, and in tooling:

```rust
pub struct Sdk<I, D, P, C> {
    instance: I,
    discovery: D,
    persistence: P,
    clock: C,
    state: AppState,
}

//  I: InstanceApi — the Mastodon instance surface
//  D: Discovery   — our recommendation API
//  P: Persistence — cache (memory, IndexedDB, …)
//  C: Clock       — time, so tests are deterministic
```

| Port | Methods | wasm implementation | native implementation |
| --- | --- | --- | --- |
| `InstanceApi` | `verify_credentials`, `timeline`, `tag_timeline`, `status`, `status_context`, `notifications`, `account`, `account_statuses`, `relationship`, `follow`, `unfollow`, `post_status`, `delete_status` | `gloo-net` (M1) | `reqwest` (M1, sync worker) |
| `Discovery` | `meta`, `feed`, `refresh`, `send_events`, `set_sync` | `gloo-net` (M1) | `reqwest` (M1) |
| `Persistence` | `load`, `store`, `remove`, `clear` over bytes | IndexedDB (M1) | memory / filesystem (tests) |
| `Clock` | `now_unix_millis` | system clock | `FixedClock` in tests |

Details that constrain implementations:

- The trait methods return `impl Future` rather than being `async fn`, so the
  returned future stays nameable and the browser and native transports satisfy the
  same declaration without `async-trait` boxing.
- `InstanceApi::Error` and `Discovery::Error` are the transport's own error types;
  the SDK maps them into `SdkError` at its boundary, so an implementation never
  translates into an enum it does not own.
- `Streaming` is a separate trait in `nodus-instance` with one method,
  `subscribe(kind)`. Streaming is optional — the sync worker polls, the browser
  streams, tests do neither — so it is not a method on `InstanceApi`. The
  implementation owns reconnection; the SDK only observes `StreamState`.
- `Persistence` values cross the port as bytes, so the port does not know every
  cached type and adapters do not know `serde`. Keys are coarse (`CacheKey`:
  `Timeline { host, which, tag }`, `Statuses { host }`, `Notifications { host }`,
  `Accounts { host }`, `Emoji { host }`, `Drafts`, `DiscoveryMeta`), so an adapter
  can evict a whole class without scanning.
- The M0 shell wires every port to a stand-in: `shell_sdk()` returns
  `Sdk<UnavailableBackend, UnavailableBackend, MemoryPersistence, SystemClock>`,
  and `Unavailable` fails every call with `no transport wired yet (M0 shell)` so a
  view that depends on network data fails loudly instead of rendering nothing.

## `Command` is the only mutation path

`Sdk::dispatch(Command)` is synchronous and is the single way state changes. The
enum is the complete vocabulary:

| Variant | Effect |
| --- | --- |
| `FeedLoaded(AccountFeed)` | replaces feed items, cursor, `generated_at`, degradation flags; clears dismissals |
| `DismissItem { item_id }` | hides one item until the next refresh (idempotent) |
| `ClearFeed` | resets the feed to `Default` |
| `StreamChanged(StreamState)` | sets the streaming socket state |
| `SignedIn(Session)` | stores the session |
| `SignedOut` | clears session, stream, feed and the whole timeline map (one notification each) |
| `TimelineLoaded { which, tag, page }` | inserts/replaces one page keyed by `(which, tag)` |

The async methods each perform one I/O step and then dispatch the matching
command, so the enum describes every reachable state change even though it is not
itself doing the I/O: `sign_in` → `SignedIn`, `sign_out` → `SignedOut` then
`persistence.clear()`, `load_timeline` → `TimelineLoaded`, `load_feed` →
`FeedLoaded`, `restore_home_timeline` → `TimelineLoaded` (cache misses and decode
failures are swallowed; the network request that follows refills it).
`report_events` and `set_sync` change no state at all — they report outward.

## The state tree

State is a tree of `Handle<T>` — `Rc<RefCell<T>>` plus a subscriber list:

```text
AppState
├── session:   Handle<Option<Session>>       // host, account id, acct, scopes — never the token
├── stream:    Handle<StreamState>           // Disconnected | Connecting | Connected | Reconnecting{attempt} | Stopped
├── feed:      Handle<FeedState>             // items, next_cursor, generated_at, degraded, dismissed
└── timelines: Handle<HashMap<TimelineKey, TimelinePage>>
```

- `TimelineKey` is `(Timeline, Option<String>)`, where `Timeline` is `Home`,
  `Local`, `Federated` or `Tag`; `TimelineRequest::<'_>::key()` builds it, so "tag
  timeline without a tag" is unrepresentable rather than a runtime check.
- `TimelinePage` is `status_ids` plus `max_id` — ids and a cursor, not status
  bodies. Bodies live in the `Statuses` cache slot.
- `FeedState::visible()` is the render path: `items` minus `dismissed`. Ordering is
  the server's; the client only filters.
- `Degraded` (`ColdStart`, `StaleSimilarity`, `NoServerSync`) is parsed from the
  wire flag by `Degraded::from_wire`, and unknown flags are dropped rather than
  failing the feed.
- `Session` deliberately has no token field: the token lives in the client's
  secret storage and is handed to the transport on demand.

`Handle` is a single observed value with `subscribe` → `Subscription` (dropping the
subscription unregisters the callback), `get`, `with`, `set` and `update`.
`notify` collects callbacks before running them, so a handler may subscribe or
unsubscribe itself. It is `Rc<RefCell<_>>`, not a thread-safe cell: the client runs
on one thread, and native tests drive the same code on one thread.

Because each field is its own handle, a view observes one field without
re-rendering on unrelated changes. The web crate adapts a handle to a Leptos
signal in `web/crates/web/src/state/bridge.rs` — that bridge is the only place
Leptos and the SDK meet.

## Errors

`SdkError<I, D>` is generic over the two transport error types:

| Variant | Meaning |
| --- | --- |
| `Instance(I)` | the instance rejected or failed the request |
| `Discovery(D)` | our API rejected or failed the request |
| `Persistence(PersistenceError)` | the cache failed (`Unavailable`, `Corrupt`, `Serde`) |
| `NotSignedIn` | the operation needs a session and there is none |
| `ClientTooOld { server, min }` | the server's `min_contract` is newer than this build |

`is_discovery_only()` is true for `Discovery` and `ClientTooOld`: those are the
failures where the layout keeps rendering and only the Feed tab degrades. The
version rules behind `ClientTooOld` are in [03-contract.md](03-contract.md).

## Why this compiles for both targets

Nothing in `crates/sdk` depends on `reqwest`, `web-sys`, `gloo-net` or a runtime.
The wasm build injects HTTP and IndexedDB; the native test build injects scripted
doubles; the state machine and its transitions are identical in both. That is the
property the `wasm32` CI check protects — see
[03-contract.md](03-contract.md#compatibility-ci).
