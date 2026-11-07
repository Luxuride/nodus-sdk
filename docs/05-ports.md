# 05 — Ports

Status: draft · Last revised: 2026-11-06

The four traits `Sdk<I, D, P, C>` is generic over: what each one is allowed to
know, what a wrong implementation can and cannot break, and why the boundary sits
where it does. The state machine they hang off is described in
[02-architecture.md](02-architecture.md); the wire types `Discovery` carries are in
[03-contract.md](03-contract.md).

## Why there is a boundary at all

One state machine has three hosts: the browser build, the native test build, and
the tooling that will drive the same transitions headlessly. Anything compiled in
for one host is inherited by the other two — a `web_sys` call in `crates/sdk` ends
the native build, and a `reqwest` dependency ends the browser build, because
`web-sys` only links for `wasm32` and `reqwest` needs a runtime. The ports are
what is left after everything host-specific has been removed: four traits that
move data across the boundary, implemented outside `crates/sdk`, with the state
machine holding only the trait, never the implementation.

That is also why the ports are generic parameters rather than `Box<dyn Trait>`
fields. A trait whose methods return `impl Future` is not dyn-compatible, so a
`dyn` object would force `async-trait` boxing on both transports to buy dispatch
that a single-threaded client never needs; the cost of the static choice is that
one process gets one port combination, which is exactly what the client, the
tests and the shell each want.

## The ports

| Port | Parameter | Methods | What it is |
| --- | --- | --- | --- |
| `InstanceApi` | `I` | 13 | the user's own instance: one method per read or write |
| `Discovery` | `D` | 5 | our recommendation API, not the instance's |
| `Persistence` | `P` | 4 | a byte cache keyed by `CacheKey` |
| `Clock` | `C` | 1 | wall-clock time, injected so expiry rules are testable |
| `Streaming` | — | 1 | the instance's WebSocket, when a host wants one |

Five traits, four parameters. `Streaming` sits beside `InstanceApi` in
`nodus-instance` instead of in `Sdk`'s generics because streaming is a capability
and not a dependency: the sync worker polls, the browser streams, tests do neither,
and a required method would force every implementation — including the M0
`UnavailableBackend` — to write a `todo!()` for a socket it will never open. The
M0 wiring is `Sdk<UnavailableBackend, UnavailableBackend, MemoryPersistence,
SystemClock>`; see [02-architecture.md](02-architecture.md) for how the shell
assembles it.

## `InstanceApi`

```rust
pub trait InstanceApi {
    type Error: Debug;                                              // the transport's own

    fn verify_credentials(&self) -> impl Future<Output = Result<Account, Self::Error>>;
    fn timeline(&self, kind: StreamKind, params: PageParams)
        -> impl Future<Output = Result<Page<Status>, Self::Error>>;
    fn follow(&self, id: &LocalId) -> impl Future<Output = Result<Relationship, Self::Error>>;
    // ten more: tag_timeline, status, status_context, notifications, account,
    // account_statuses, relationship, unfollow, post_status, delete_status
}
```

Thirteen methods, one per instance operation the client performs: the reads a
timeline or thread view needs, and the four writes that change something on a
third-party server (`follow`, `unfollow`, `post_status`, `delete_status`). There is
deliberately no `request(path, body)` escape hatch — such a method would push
endpoint knowledge and error mapping back into the caller, which is the thing this
port exists to own, and it would make the trait impossible to fake at the level the
SDK uses it.

`impl Future` rather than `async fn` is the choice that keeps two adapters honest.
The `gloo-net` browser implementation and the `reqwest` native implementation both
declare the same signature with no `async-trait` boxing and no allocation per call,
and the returned future stays nameable for an implementor that needs to name it.
The trade — no `dyn` — is recorded above.

What a wrong implementation can break: it can make state untrue without making it
inconsistent. A `verify_credentials` that returns the wrong account mints a
`Session` for that account, so the "home timeline" the client shows is somebody
else's; a `follow` that returns `Ok` without sending anything leaves the optimistic
relationship on screen until the next refresh replaces it. What it cannot do is
corrupt the state machine's rules: a port cannot write into `AppState`, because
only `Command` can, so every failure above arrives as data that a later read
corrects. It also cannot be policed from here — the access token lives inside the
transport by construction (`Session` has no token field), so a transport that
leaks the token leaks it. The port boundary limits what a *view* can see, not what
an implementation does.

## `Streaming`

```rust
pub trait Streaming {
    type Error: Debug;
    fn subscribe(&self, kind: StreamKind) -> impl Future<Output = Result<(), Self::Error>>;
}
```

One method, and the implementation owns everything behind it: reconnection with
backoff, the visibility pause, and the decision that a socket has given up. The SDK
learns about all of it through `Command::StreamChanged(StreamState)` and never
holds the socket.

A wrong implementation is the quietest failure in the crate. A `subscribe` that
returns `Ok` and then delivers nothing is indistinguishable, from the state
machine's side, from a quiet timeline: no error is raised, no state changes, and
the UI sits on `Connecting` or `Connected` indefinitely. Silence is not a failure
signal, and this port has no vocabulary for "healthy but silent" — which is why the
implementation must drive `Reconnecting { attempt }` and `Stopped` itself rather
than leaving the SDK to infer them. What it cannot break is the timeline: a lost
event means a page is stale, not wrong, and the next focus or refetch resolves it.

## `Discovery`

```rust
pub trait Discovery {
    type Error: Debug;

    fn meta(&self) -> impl Future<Output = Result<Meta, Self::Error>>;
    fn feed(&self, cursor: Option<&str>) -> impl Future<Output = Result<AccountFeed, Self::Error>>;
    fn posts(&self, cursor: Option<&str>) -> impl Future<Output = Result<PostFeed, Self::Error>>;
    fn refresh(&self, feed_id: &str) -> impl Future<Output = Result<AccountFeed, Self::Error>>;
    fn send_events(&self, batch: &EventBatch) -> impl Future<Output = Result<(), Self::Error>>;
    fn set_sync(&self, enable: bool, telemetry_opt_in: bool)
        -> impl Future<Output = Result<(), Self::Error>>;
}
```

Our API, not Mastodon's: `meta` reports the contract version and feature flags,
`feed` pages an opaque cursor over recommended *accounts*, `posts` does the same over
recommended *posts*, `refresh` recomputes under the server's 30-second
floor and returns the new snapshot, `send_events` reports interactions, and
`set_sync` carries two independent consents (server-side sync, and telemetry) in one
call so they cannot be granted separately by accident. The two feeds are separate
methods rather than one generic one because the server gives them different lifetimes:
an account snapshot is reused for `SNAPSHOT_REUSE` (ten minutes), while a fresh post
page is always new work — so a caller that conflated them would either re-rank accounts
on every reload or serve a stale "anything new?". The `append` decision lives in
`Sdk::load_posts`, which knows whether the caller passed a cursor. `refresh` returns an
`AccountFeed` rather than `()` because a recompute is also a read: the caller
usually wants to render the result it just caused.

A wrong implementation produces a feed that looks like a feed. If it ignores
`cursor`, "load more" returns the first page again and the list loops; if it
re-sorts or re-filters items, nothing fails, because the SDK deliberately never
checks ordering — ordering is the server's, per invariant 4 in
[01-scope.md](01-scope.md) — so a bad order is visible only to a person. A
`send_events` that double-sends inflates one ranking signal; deduplication is per
`(item_id, kind, client_ts)` on the server, and the client cannot verify it. What it
cannot break is anything on the instance plane, and that is the reason this is a
second trait rather than one trait with two base URLs: `SdkError::Discovery` and
`SdkError::Instance` are distinct types, so `is_discovery_only()` can keep the
layout rendering while only the Feed tab degrades.

## The discovery transport

`nodus-discovery` is the concrete client, in its own crate because `crates/sdk`
must stay free of `reqwest` and `gloo-net` while this transport needs one of them.
`nodus_discovery::DiscoveryClient` is the native (`reqwest`) implementation and
`nodus_discovery::BrowserDiscovery` the browser (`gloo-net` + `web-sys`) one; both
implement `Discovery` and map a `426` to their own `ClientTooOld`, so the web crate
constructs `Sdk<BrowserInstance, BrowserDiscovery, …>` and the native harness
constructs `Sdk<HttpInstance, DiscoveryClient, …>`. The SDK never names either
type: it sees only the trait, and a transport's `ClientTooOld` reaches the UI as
`SdkError::Discovery(..)`, which is still discovery-only.

## `Persistence`

```rust
pub trait Persistence {
    fn load(&self, key: &CacheKey)
        -> impl Future<Output = Result<Option<Vec<u8>>, PersistenceError>>;
    fn store(&self, key: &CacheKey, value: Vec<u8>)
        -> impl Future<Output = Result<(), PersistenceError>>;
    fn remove(&self, key: &CacheKey) -> impl Future<Output = Result<(), PersistenceError>>;
    fn clear(&self) -> impl Future<Output = Result<(), PersistenceError>>;
}
```

Values cross as `Vec<u8>` so the port does not have to know every cached type and
the adapters do not have to know `serde`; keys are coarse (`Timeline { host, which,
tag }`, `Statuses { host }`, `Notifications { host }`, `Accounts { host }`,
`Emoji { host }`, `Drafts`, `DiscoveryMeta`) so an adapter can evict a whole class
without scanning. `load` returns `None` for a slot that is absent *or* expired, so
the port may enforce its own age policy without the SDK knowing about it.

A wrong implementation costs a cold start, and that is the whole blast radius. A
`load` that always errors makes every boot paint from the network; a `load` that
returns the wrong bytes makes `restore_home_timeline` either fail to decode (the
error is swallowed, by design: a cold cache is not an error) or restore a page
whose status ids have no bodies in the `Statuses` slot, which renders as skeletons
until the first network page replaces it. It cannot corrupt state, because the
cache is never the source of truth: the feed is not cached at all, a timeline slot
is a copy of an earlier server answer, and every cached read is followed by a
revalidation. The one place a persistence bug is more than cosmetic is `clear` on
sign-out — a no-op `remove` or `clear` leaves another account's timeline on disk —
which is why `clear` returns a `Result` and `Sdk::sign_out` propagates it instead of
swallowing it. No `CacheKey` variant addresses the token, so the SDK can neither
read nor write it through this port; custody stays with the caller.

## `Clock`

```rust
pub trait Clock {
    fn now_unix_millis(&self) -> i64;
}
```

Time is a dependency because every interesting client rule is time-shaped: cache
soft-expiry, streaming backoff, feed staleness, and the ordering of "N new posts".
Taking it from a trait means those rules are tested by advancing a `FixedClock`
instead of sleeping, which is the difference between a suite that runs in
milliseconds and one that flakes under load. `Sdk::now_unix_millis` is the only
reader today; the scheduling rules live in `crates/runtime`'s `Scheduler`, which is a
pure function of `(now: Instant, budget: Budget)` and therefore needs no port at all
— that is the cheaper shape whenever the decision can be expressed without reading a
stored timestamp.

A wrong implementation is silent in the expensive direction. A clock frozen in the
past makes every stored entry look fresh forever, so expired pages are served
indefinitely and the bug never raises an error; a clock that jumps forward expires
everything and turns every boot into a cold start; a clock that returns `0`
guarantees both. What it cannot break is the data: a wrong time changes when a
cached value is *ignored*, never what it contains. The one place a wrong clock
escapes into the wire format is `InteractionEvent::client_ts`, which is RFC 3339 —
and this port only reports epoch milliseconds, so the caller formats it and owns the
conversion.

## The scheduler is not a port

`Scheduler::plan(now, &Budget) -> Plan` answers four questions at once: is the
timeline stale, is the feed stale, when is the next retry due, when is the next
reconnect due. It takes `std::time::Instant` and a plain `Copy` budget, returns a
plain `Copy` plan, and performs no I/O — which is why it did not need a fifth port.
The only reason to promote it later is if a rule needs the stored `Clock` (a cached
`expires_at` compared against real time); until then the caller passes `Instant`.

Two of its constants encode decisions rather than durations. `FEED_STALE` (10 min) is
twice `TIMELINE_STALE` because the server reuses a recent snapshot: refreshing sooner
would spend a request to receive the snapshot it just served. `MIN_ATTEMPT_GAP` (1 s)
exists so a burst of user commands collapses into one load — the caller keeps the
last intent, not the last ten.

On `wasm` a `Plan` is a hint. Timers are throttled in background tabs and dropped in
a sleeping one, so the caller wakes late, re-plans, and acts on the elapsed time
rather than the planned one; the plan is cheap precisely so that this is safe.

## Two rules the boundary enforces

**Domain types only.** Every signature above speaks in `nodus_instance`'s model
(`Status`, `Account`, `Page<T>`, `StreamKind`), `nodus_shared`'s wire DTOs, a plain
`i64` or `Vec<u8>`, or a type owned by `crates/sdk` itself (`CacheKey`,
`PersistenceError`). No `web_sys`, no `reqwest::Response`, no `gloo_net` request, no
`sqlx` row. The reason is the crate graph, not taste: the SDK is compiled for
`wasm32-unknown-unknown` *and* `x86_64`, and a port that named a browser type ends
the native build while a port that named a server type ends the browser one. That is
a two-line change in a trait header away at all times, which is why it is a review
rule and not a comment.

**Errors belong to the transport.** `InstanceApi::Error` and `Discovery::Error` are
associated types, so an implementation reports its own failure type and the SDK maps
it into `SdkError::Instance` or `SdkError::Discovery` at the boundary; neither side
has to translate into an enum it does not own, and the SDK never invents a variant
for a failure it does not understand. `PersistenceError` is the deliberate
exception: it lives in the SDK because the SDK is what decides that a stored value
which will not decode under the current schema is *corrupt* rather than merely
absent.

## How a new port is added

The test is one question: does the thing move data, or does it *decide* something?
Data movement belongs in a port — bytes to a store, a token to an instance, a
question to a clock. Decisions belong in the state machine, where a `Command`
describes them and a native test can assert them without a browser. A port that
decides reintroduces exactly what splitting state out of the client removed: logic
that can only be tested by faking a framework.

Four checks apply before a fifth parameter appears. First, would a wrong
implementation show up as a difference in `AppState`? If not, the difference is a
decision and belongs in `dispatch`. Second, can a scripted double be written in ten
lines? If not, the trait is carrying policy that a fake would have to reproduce.
Third, does it name a framework type? Then it is an adapter for an existing port,
not a new port. Fourth, does the sync worker need it too? Something only the browser
needs is more likely UI. When the answer survives all four, the port is added the
way everything else here is: docs first, in the same additive spirit as
[01-scope.md](01-scope.md) § Relationship to the Mastodon API.

## Ports we deliberately do not have

**No `Logger` port.** A log line is an event nobody reads back through the state
machine, so there is no data to move and no decision to test. Logging goes to
`tracing`, whose target-based filtering already gives a caller the control a port
would expose — and `tracing` is not yet a dependency here, arriving with the
transports in [04-roadmap.md](04-roadmap.md) M1.

**No `ErrorReporter` port.** The client is not a telemetry endpoint: crash and error
reporting belongs to the host page's instrumentation, the same boundary that owns
the CSP, and `SdkError` already carries the classification a reporter would need.
A port would also create a second consent surface — the telemetry opt-in is a
recommendation-signal consent (`Discovery::set_sync`), and reusing it for diagnostics
would blur what the user actually agreed to.

**No `Notifications` port.** Permission prompts are UI: they have to appear at the
moment the user acts, in the page, dismissible, and they read a permission store the
browser owns. A port between the view and that store moves the prompt's logic into
the SDK, where the UI's own tests can no longer reach it — the prompt becomes
untestable in the layer that renders it, in exchange for a fake the UI test would
have had to supply anyway. The SDK's share of that problem is already in state:
`StreamState::Stopped` is what the UI renders when a socket cannot open.

`SyncApi` — `following`, `favourites`, `bookmarks` — is a trait in `nodus-instance`
but not one of the four, because it describes what the crawler reads on the user's
behalf while the browser is closed, and a crawler surface has no write method for a
reason. Browsing needs two of those operations for its saved-posts routes, so the
reconciliation is a roadmap question: either they move onto `InstanceApi` when those
views land, or the client gains a fifth parameter.
