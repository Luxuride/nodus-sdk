# 01 — Scope

Status: draft · Last revised: 2026-11-06 (decisions D10, D13, D14)

What belongs in this repository, what belongs in a consumer, and the rules that
keep the split from quietly collapsing. The architecture these constraints produce
is in [02-architecture.md](02-architecture.md).

## The three layers, one question

The contract and the client state live together because they are one question
asked at three layers:

- `nodus-shared`: what *our* API returns to clients;
- `nodus-instance`: what *an instance* returns to both the sync worker and the
  browser;
- `nodus-sdk`: what the client *does* with both — session, timelines, feed,
  streaming, cache policy.

The last one is the reason this repository is not a passive "contract". If state
lived in the client crate, the browser would own the only copy of pagination,
cache-expiry and optimistic-update rules, and the sync worker would grow a second
copy. Splitting state out is what lets the web client stay a renderer.

It also keeps the SDK honest: it must compile for `wasm32` *and* `x86_64` with no
framework dependency, so anything web-only cannot sneak in.

## What belongs in each crate

| Crate | Owns | Does not own |
| --- | --- | --- |
| `nodus-shared` | the wire DTOs, `CONTRACT_VERSION`, the shared error envelope and `Meta` | HTTP, retries, any I/O |
| `nodus-instance` | the Mastodon domain model and `InstanceApi` / `Streaming`, plus instance and app-registration helpers | a chosen transport in the default build |
| `nodus-sdk` | `AppState`, `Command`, `Sdk<I, D, P, C>`, `Handle`, the four port traits, `SdkError` | rendering, routing, token custody |

Anything that would make one consumer's code the other's problem does not belong
here. `core` (ranking) stays recommender-only precisely because the client must
not recompute scores; the client renders what it is given.

## The client-side boundary

What the SDK owns, and therefore what the web client must not re-implement:

- session lifecycle (sign-in, sign-out, cache clear);
- timeline paging and cache policy (soft TTL, page caps);
- recommendation feed state, local dismissals, degradation flags;
- streaming socket state and reconnect bookkeeping;
- optimistic updates and their rollback.

What it explicitly does not do:

- ranking, scoring or candidate generation (recommender);
- rendering, sanitisation, routing, keyboard handling (web);
- token custody: the token is handed to a transport, never stored in `AppState`.

## The five invariants

These are the rules a change to `nodus-sdk` is reviewed against.

1. **One mutation path.** Views call `Sdk::dispatch(Command)`. There is no other
   way to change state, so "what can happen" is one enum, and every transition is
   testable without a browser.
2. **No framework types.** `Handle` is deliberately not a Leptos signal: this
   crate must compile for native tests. `web/crates/web/src/state/bridge.rs`
   adapts a handle to a revision signal in about forty lines.
3. **No tokens in state.** The token lives in the client's secret storage and is
   handed to the transport; a view that could read it is a view that could leak
   it. `Session` carries host, account id, `acct` and scopes — nothing else.
4. **No ranking math.** Ordering comes from the server. The client may dismiss,
   paginate and explain — not score.
5. **No I/O in `dispatch`.** It is synchronous, so a click can change the UI on
   the same frame.

## Deliberately not here

- **Ranking, similarity, candidate generation.** They live in `recommender`
  (`nodus-core`), and the feed arrives already ordered with reasons attached
  ([`recommender/docs/06-ranking.md`](../../recommender/docs/06-ranking.md)).
- **HTTP for our API, as a concrete client.** The SDK declares the `Discovery`
  port; the adapters (`gloo-net`, `reqwest`) are built on top of it in
  [04-roadmap.md](04-roadmap.md) M1.
- **Framework types.** No Leptos, no `web-sys`, no DOM in the state machine.
- **Tokens in state.** See invariant 3.
- **A rendered UI, routes, or styles.** Those are
  [`web/docs/`](../../web/docs/README.md).

## Relationship to the Mastodon API

`nodus-instance` models the parts of the instance API the client and the worker
both use. It is not a complete Mastodon client: missing endpoints are added when a
consumer needs one, in the same additive spirit (a new trait method with a
default-free requirement is a breaking change for 1.0, which is why the SDK stays
below 1.0 until the M3 parity matrix closes).
