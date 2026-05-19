# FortiState HTTP Contract for Dropply

Last updated: 2026-04-27

This document defines the optional remote snapshot contract used by Dropply's backend state adapter.

Important clarification:

- the `fortistate` npm package is Dropply's embedded live state kernel
- this document is about an optional remote sync/persistence endpoint
- they are related concepts, but they are not the same runtime surface
- a concrete service scaffold for this endpoint now lives in `private-components/state-service`

It is intentionally simple:

- one namespace
- one session snapshot document
- JSON payload compatible with Dropply's persisted pair-session schema

This is not the final ideal remote state API. It is the first stable contract Dropply can target while moving beyond pure local JSON-file persistence.

## 1. Purpose

Dropply needs an authoritative shared state layer for:

- pair sessions
- device registry
- revoked devices
- relay item metadata
- relay blob ledgers
- WebRTC signaling queues

The current backend already runs an embedded FortiState-style kernel in process.

Separately, it supports `PAIR_STATE_PROVIDER=fortistate` as an optional remote snapshot provider and will attempt to use a FortiState-style HTTP document endpoint before falling back to JSON.

The backend adapter now also tracks `ETag` values and retries snapshot saves after a reload/merge when it detects a `412` conflict.

## 2. Endpoint shape

Base config:

- `FORTISTATE_ENDPOINT`
- `FORTISTATE_NAMESPACE`

Resolved document URL:

`{FORTISTATE_ENDPOINT}/namespaces/{FORTISTATE_NAMESPACE}/pair-sessions`

Example:

`https://state.example.com/namespaces/dropply/pair-sessions`

## 3. Supported methods

### `GET`

Purpose:

- load the full persisted pair-session store

Expected responses:

- `200 OK` with JSON body matching `PersistedPairSessionStore`
- `404 Not Found` meaning "no state exists yet"

### `PUT`

Purpose:

- replace the full persisted pair-session store with the newest snapshot

Expected responses:

- `200 OK`
- `201 Created`
- `204 No Content`

## 4. JSON payload shape

The body matches Dropply's persisted pair-session store:

```json
{
  "version": 1,
  "savedAt": 1770000000000,
  "sessions": [
    {
      "token": "abc123",
      "devices": [
        {
          "deviceId": "device-1",
          "deviceType": "desktop",
          "label": "Dropply desktop",
          "lastSeenAt": 1770000000000,
          "transportPreference": "p2p"
        }
      ],
      "revokedDevices": [],
      "items": [],
      "blobs": [],
      "signals": {},
      "updatedAt": 1770000000000
    }
  ]
}
```

## 5. Auth headers

The Dropply backend supports these request headers for the remote snapshot endpoint:

- `x-api-key: <FORTISTATE_API_KEY>` when configured
- `Authorization: Basic ...` when `FORTISTATE_BASIC_USERNAME` and `FORTISTATE_BASIC_PASSWORD` are configured

Either or both may be sent depending on environment.

## 6. Current runtime behavior

When Dropply backend is configured with:

- `PAIR_STATE_PROVIDER=fortistate`

it will:

1. attempt `GET` from the configured endpoint on startup
2. attempt `PUT` on session persistence
3. fall back to local JSON storage if the endpoint is unavailable or incompatible

That fallback is deliberate so production does not break during migration.

## 7. What this contract is good for

This contract is enough to:

- move Dropply off pure local backend durability
- share pair/session state across restarts and compatible runtimes
- test a FortiState-aligned deployment path without changing Dropply's live in-process kernel

## 8. What this contract does not solve yet

This first contract does **not** yet provide:

- partial document mutation
- optimistic concurrency / ETags
- per-token sharding
- event streams
- true queue semantics for signaling
- fine-grained conflict resolution

Those are good next steps once the first remote state path is stable.

## 9. Recommended next evolution

After this snapshot contract proves stable, the next remote state version should split state into:

- `/sessions/{token}`
- `/sessions/{token}/signals/{deviceId}`
- `/sessions/{token}/items/{itemId}`
- `/sessions/{token}/blobs/{itemId}`

That would reduce write amplification and make signaling and resume ledgers much cleaner.
