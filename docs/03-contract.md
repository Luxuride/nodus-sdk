# 03 — Contract and versioning

Status: draft · Last revised: 2026-11-06

The wire contract in `nodus-shared` is versioned independently of the two
consumers, because they deploy independently: the API can ship a new build on
Monday and a browser tab can be running last month's bundle. This document states
what a version number promises, how the three repositories stay consistent, and
what fails when they do not. The types themselves are described server-side in
[`recommender/docs/07-api.md`](../../recommender/docs/07-api.md).

## Version semantics

```text
0.4.0   ← major.minor.patch
│ │ │
│ │ └── patch: docs, internal refactor, no wire change (consumers need not bump)
│ └──── minor: additive wire change (new optional field, new endpoint, new feature flag)
└────── major: breaking wire change (removed/renamed field, changed semantics)
```

`CONTRACT_VERSION` in `nodus-shared` is the single source of that number. The API
reports it in `GET /api/v1/meta` and clients send it as `X-Contract-Version`; a
server also reports `min_contract`, the oldest version it still accepts, and
`features`, the capability list the client gates UI on instead of sniffing
versions.

## Rules inside a minor version

1. Additive only: new fields are `Option<T>` or `#[serde(default)]`; new endpoints
   appear behind a `Meta::features` flag.
2. Nothing is removed or renamed inside a minor — deprecate in the docs, remove in
   the next major.
3. `CONTRACT_VERSION` in `nodus-shared` is the single source; it is bumped
   in the same commit as the wire change.
4. `min_contract` (served by `GET /api/v1/meta`) only rises with a major version,
   so an old client can still reach a server that has moved on.
5. Tags are annotated, and the changelog entry says which consumers must bump.

## Coexistence

| Situation | Behaviour |
| --- | --- |
| new server, old client | server sends extra fields; client ignores unknown ones (`serde` default behaviour) |
| old server, new client | client compares `meta.contract`/`min_contract` at boot and gates features on `meta.features` |
| server too new for the client (`min_contract` > client's contract) | `426 client_too_old`; the client shows an update screen and keeps timelines working (instance traffic is independent) |
| SDK changed, consumer not rebuilt | impossible: path dependency, so the consumer either builds the new SDK or does not build |

The mismatch surfaces as `SdkError::ClientTooOld { server, min }`, which
`is_discovery_only()` classifies as a Feed-tab problem — the timeline plane never
touches the contract, so the app keeps rendering
([02-architecture.md](02-architecture.md#errors)).

## Pinning

The three projects are submodules of one superproject:

```text
Nodus/            ← superproject: .devcontainer, README, .gitmodules
├── sdk/      → nodus-client-sdk       (submodule)
├── recommender/  → nodus-recommender     (submodule)
└── web/      → nodus-web       (submodule)
```

- The superproject pins each project by **gitlink** (an exact commit), so one
  `Nodus` commit describes a known-good combination of all three.
- Consumers depend on the SDK by **path**, not by `git` URL:

  ```toml
  nodus-shared = { path = "../sdk/crates/shared", version = "0.1.0" }
  nodus-client-sdk    = { path = "../sdk/crates/client-sdk",    version = "0.1.0" }
  ```

  Paths work because the submodules always sit at fixed relative positions. The
  alternative (a `git = "https://…"` dependency) was rejected: it duplicates the
  sources into `~/.cargo/git`, forces a push before a cross-project change can even
  be compiled, and leaves two sources of pinning (gitlink *and* the URL in
  `Cargo.toml`) that can disagree. It also breaks the edit-and-build loop for a
  change that spans the boundary, and it makes offline builds and CI caching
  worse.
- `Cargo.lock` is committed in `recommender` and `web`.

Tags still exist in this repository, because they mark *what* a contract version
means over time and make bisecting an incident straightforward. They are a label,
not the pinning mechanism. Publishing is git tags only, not crates.io; revisit if a
consumer outside this superproject appears — that is also the point at which a
`git = "https://…"` dependency would become the right answer.

## Bump procedure

1. Change the type in `nodus-shared`, bump `CONTRACT_VERSION` in the same commit,
   and say in the changelog which consumers must bump.
2. Tag the commit in `sdk` (`v0.x.y`, annotated).
3. In the superproject: `git submodule update --remote sdk` and commit the gitlink.
   The consumers pick the change up on their next build; no `cargo update` dance,
   no tag to re-point.
4. The server bumps `min_contract` only when the metric shows no traffic from the
   old version (see failure modes).

## Compatibility CI

Every SDK PR runs:

1. `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`;
2. `cargo check --workspace --target wasm32-unknown-unknown` — the SDK is compiled
   for the browser too, so a wasm-only build failure is a broken contract;
3. a checkout of `recommender@main` and `web@main` built against the working tree.

The third step is the one that matters: it is the only place where "this change
still compiles for both consumers" is actually proven.

## Failure modes

| Failure | Symptom | Handling |
| --- | --- | --- |
| consumer updated, SDK not | build error (missing field/type) | expected: path dependency means the failure is loud and immediate |
| SDK minor bump with a non-additive change | old client fails to deserialize | review rule 1; CI builds the previous consumer tag against the new SDK |
| `CONTRACT_VERSION` not bumped | clients cannot detect the mismatch | bump is part of the review checklist for `shared/` diffs |
| `min_contract` raised too early | existing clients get `426` | raise only when `api_contract_version_total` shows no traffic from the old version |

## What the contract is not

The naming rules that keep the Mastodon mark out of crate and product names, and
the disclaimer text, live in
[`recommender/docs/11-trademark.md`](../../recommender/docs/11-trademark.md).
Questions about *publishing* the contract (crate names, licence, tag target) are
open questions in the recommender's
[`README.md`](../../recommender/docs/README.md#open-questions).
