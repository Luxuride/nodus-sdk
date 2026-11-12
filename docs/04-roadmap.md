# 04 — Roadmap

Status: draft · Last revised: 2026-11-06

The SDK-facing slice of the milestones. Ordered by risk retired, and derived from
the programme roadmap in
[`recommender/docs/10-roadmap.md`](../../recommender/docs/10-roadmap.md) and the
client parity matrix in
[`web/docs/01-scope.md`](../../web/docs/01-scope.md) — the three sets describe the
same sequence from three sides.

| SDK milestone | Lands with | Why it is on this side of the line |
| --- | --- | --- |
| M1 — ports | recommender M1 (contract tag `v0.1.0`), web M2 | the sync worker needs the `reqwest` `InstanceApi`, the browser needs `gloo-net` + IndexedDB; neither can start without them |
| M2 — feed and events | recommender M2, web M2 (Feed tab) | the Feed tab is the reason the client exists; it is wired in the first usable client, not after parity |
| M3 — parity surface | web M3, closing before web M5 | the remaining parity rows that need SDK state, not just new views |

## M0 — done

State, commands, ports, the error model and the pure state transitions, with
tests. No transports: `shell_sdk()` wires every port to `Unavailable`, so a view
that reaches for the network fails loudly. See
[02-architecture.md](02-architecture.md).

## M1 — Port implementations

Scope:

- `InstanceApi` for both targets: `reqwest` (`native` feature, used by the sync
  worker) and `gloo-net` (`wasm` feature, used by the browser), one endpoint
  surface, feature-gated. The split is the intent recorded in
  `crates/client-core/src/lib.rs`; the feature flags themselves and the transport
  modules land with this milestone;
- `Discovery` over our API, both targets, including `meta` cached at boot and the
  `426 client_too_old` path surfacing as `SdkError::ClientTooOld`;
- `Persistence` on IndexedDB for one host's timeline, statuses, notifications,
  accounts and emoji, plus the `DiscoveryMeta` slot, so `restore_home_timeline`
  renders before the network answers;
- streaming: a `Streaming` implementation driving `StreamState` through
  `Connecting` → `Connected` → `Reconnecting { attempt }` → `Stopped`;
- a `FakeInstance` in the test tree — a scripted `InstanceApi` +
  `Discovery` + `FakeClock` — so every state transition is testable without a
  browser or a network.

**Exit:** `cargo test --workspace` drives sign-in, a timeline page, a feed page and
a reconnect entirely through the fake ports; `cargo check --workspace --target
wasm32-unknown-unknown` is green with the `wasm` feature; `recommender` and `web`
both build against the working tree (the compatibility CI in
[03-contract.md](03-contract.md#compatibility-ci)).

Risks retired: whether one endpoint surface really satisfies both targets, whether
IndexedDB behaves behind the byte-oriented `Persistence` port, and whether
streaming reconnection can be pure SDK state.

## M2 — Feed loading and event reporting

Scope: `load_feed` paging through the opaque cursor, `DismissItem` applied
locally until the next refresh, `Degraded` flags surfaced verbatim, and
`report_events` batching impressions/clicks/follows/dismissals with idempotency per
`(item_id, kind, client_ts)`. Cache the feed snapshot so a reload keeps the tab
instant.

**Exit:** the Feed tab renders a real page from a real server through the SDK, a
dismissal survives a route change and clears on refresh, and an offline reload
renders the cached feed with its staleness visible.

Risks retired: cursor stability across pages, and whether reporting events through
the SDK (rather than from views) is enough to keep "what was shown" attributable.

## M3 — The rest of the parity surface

Scope: the parity rows that need SDK state rather than a new view — notifications
(paging, mark-as-read, type filters), lists (CRUD plus list timelines, which means
a new `Timeline` key and new `InstanceApi` methods), and server-side filters
applied to rendered content. Optimistic updates and their rollback land here too,
since follow/boost/favourite are the actions that need them.

**Exit:** the parity matrix in
[`web/docs/01-scope.md`](../../web/docs/01-scope.md) has no remaining rows whose
blocker is SDK state; contract tag `v0.2.0` (additive — new endpoints behind
`meta.features`, new trait methods). A new trait method with no default is still a
breaking change, which is why this repository stays below 1.0 until this milestone
closes.

Risks retired: whether the state tree generalises from three fixed timelines to
lists, and whether optimistic rollback can be expressed as commands.

## Deferred (explicitly not now)

- **SSR / hydration.** The state machine would need to be serialisable at a point
  in time and rehydrated on the server; nothing needs it until a public,
  unauthenticated page has to be indexed. The client's position is in
  [`web/docs/05-quality.md`](../../web/docs/05-quality.md) (decision D8).
- **Offline outbox.** Queuing composed posts and follows for replay needs a
  durable command log and conflict rules; the M3 cache is read-only.
- **Multi-account.** One session at a time in v1. `AppState::session` is
  `Handle<Option<Session>>`; making it a map is a breaking state change, so it
  waits for a decision rather than being scaffolded now.
- **Native/desktop shells and tooling consumers.** The ports make them possible;
  nothing in the milestones needs them.
