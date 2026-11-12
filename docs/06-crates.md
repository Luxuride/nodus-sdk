# Crate layout

The repository is one dependency direction: adapters point at the shared model,
the client runtime points at the model, and no adapter depends on the web client.

```text
Clients (web, mobile, desktop, bots)
        │
        ▼
nodus-client-sdk  ──►  nodus-runtime  ──►  nodus-state  ──►  nodus-shared
        │                  │
        │                  └──► nodus-persist
        │
        └──► nodus-client-core   (model, traits, commands)
                   ▲       ▲
                   │       │
            nodus-mastodon  nodus-misskey
```

| Crate | Depends on | Responsibility |
| --- | --- | --- |
| `nodus-shared` | nothing | Wire contract for the Nodus recommendation API. |
| `nodus-client-core` | nothing but serde/url | Canonical client model (`Actor`, `Post`, `Emoji`, `Media`), `InstanceApi`, `SyncApi`, command vocabulary. Mastodon names are aliases. |
| `nodus-mastodon` | client-core | Mastodon REST compatibility and native/browser transports. |
| `nodus-misskey` | client-core | Misskey REST compatibility mapped onto the core traits. |
| `nodus-state` | client-core, shared | Observed state tree and commands. |
| `nodus-runtime` | state, persist, discovery | The `Sdk<I, D, P, C>` state machine and shells. |
| `nodus-client-sdk` | runtime, optional adapters | The client entry point and feature gate for Mastodon and Misskey. |
| `nodus-discovery` | state/shared | Nodus recommendation API transport. |
| `nodus-persist` | client-core | Cache port and memory implementation. |

## Feature gates

`nodus-client-sdk` defaults to no protocol adapter. A client enables what it talks to:

```toml
nodus-client-sdk = { path = "../sdk/crates/client-sdk", features = ["mastodon"] }
```

- `mastodon`: `nodus-mastodon` + `nodus_client_sdk::mastodon`.
- `misskey`: `nodus-misskey` + `nodus_client_sdk::misskey`.

## Why `nodus-client-core` and not `nodus-core`

The recommender already uses `nodus-core` for its own domain/ranking types. Keeping
the client package distinct (`nodus-client-core`) means a single process can link
both without two crates claiming the same Rust crate name.

Server-side `ActivityPub` and federation primitives live in the sibling
`nodus-server-sdk` repository, not here.
