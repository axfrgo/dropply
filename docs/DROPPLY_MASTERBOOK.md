# Dropply Masterbook

Last updated: 2026-05-14

This document is the current source of truth for how Dropply works today across the Windows desktop app, the hosted pair dashboard, and the hosted backend.

## 1. Product summary

Dropply is a local-first Windows desktop stream and cross-device intent layer that can exchange Smart Drops with phones, browsers, and terminal users. It uses:

- a Windows desktop app as the canonical local stream owner
- a hosted pair dashboard for phones and browsers
- WebRTC data channels for direct media transfer in `p2p` mode
- hosted relay metadata and chunked blob storage in `relay` mode

Smart Drops are additive metadata on the existing item stream. A Smart Drop can carry source context, local semantic context, suggested actions, target-device intent, lifecycle state, and trust/provenance context while the file/text/image/link still uses the existing storage and transfer paths.

## 2. Verified live state

Verified against the live backend on 2026-04-26:

- `GET /v1/public/config` returns:
  - `hosted_sync_requires_login: false`
  - `hosted_sync_available: false`
  - `auth_configured: false`
- `GET /v1/auth/providers` returns providers with `enabled: false`
- `POST /v1/pair/register` preserves `transportPreference`
- FortiCore logs show the current runtime serving the updated backend behavior

This matters because earlier stale-runtime issues are no longer the current state.

## 3. What ships today

### Desktop app

Primary role:

- owns the local stream
- stores imported items locally
- pairs devices
- publishes relay manifests and relay blobs
- serves direct media over WebRTC in `p2p` mode

Language support:

- desktop UI supports `EN` and `FR`
- the pair dashboard also supports `EN` and `FR`
- language can auto-detect and can be switched manually
- desktop and pair-dashboard preferences now sit on FortiState-style local UI stores

Current visible capabilities:

- write text into the stream
- import files, images, and video
- create Smart Drops with source context, local labels/tags, suggested actions, and lifecycle state
- copy text items
- open files in the default desktop app
- delete single items
- mark items as pending, completed, or revoked
- clear the full stream
- download items directly to the Windows Downloads folder
- generate a pair token and QR
- switch transport preference between `p2p` and `relay`
- remove a specific paired device
- unpair the desktop entirely

### Hosted pair dashboard

Primary role:

- join an existing pairing token
- show received items
- preview image and video
- download files
- upload text and files back into the shared stream
- display and create Smart Drop metadata for web-originated text and file uploads
- use direct media fetch in `p2p` mode when available
- use relay blob storage in `relay` mode

Adaptive client behavior:

- phones register as `mobile`
- desktop browsers register as `web`
- manual overrides exist through query params:
  - `?device=mobile`
  - `?device=browser`
  - `?client=mobile`
  - `?client=browser`

### Hosted backend

Primary role:

- store pair-session state
- track devices per token
- accept relay manifests
- accept relay blob chunks
- serve reconstructed relay items
- broker WebRTC signaling
- expose public config and published plan limits

### Companion CLI/TUI

Primary role:

- join the same Dropply relay session from a terminal
- send text and files from CLI workflows
- expose a TUI stream for terminal-first users
- search Smart Drops by item name, text, label, source context, and tags
- open items and update pending/completed/revoked lifecycle state

## 4. High-level architecture

### Data ownership

- The Windows desktop is the canonical user-facing stream owner.
- The backend is a hosted session broker plus relay store.
- The pair dashboard is a paired client, not the canonical source of truth.

### Session model

Each pairing token maps to a session containing:

- active devices
- revoked devices
- relay items
- relay blobs
- WebRTC signaling queues
- update timestamps

Relay items can also include nullable Smart Drop metadata:

- `source_context`
- `semantic_context`
- `suggested_actions`
- `intent_state`
- `trust_context`

Older peers that omit those fields continue to import normally. The desktop fills safe defaults and local labels during import.

### Persistence model

Desktop persistence:

- local desktop stream items are persisted in the desktop app

Backend persistence:

- pair sessions now sit behind a backend state-store adapter
- the live in-process session runtime now uses an embedded `fortistate-kernel`
- desktop, hosted web, and backend now all share one real local package import for that kernel: `@dropply/fortistate-kernel`
- the active persistence provider today is `json`
- the persistence provider is configurable through `PAIR_STATE_PROVIDER`
- an optional remote snapshot contract now exists in [FORTISTATE_HTTP_CONTRACT.md](/C:/Users/alexj/Documents/OpenDrop/docs/FORTISTATE_HTTP_CONTRACT.md)
- pair sessions are currently persisted to a JSON file by that provider unless a compatible remote snapshot endpoint is configured
- default store path: `.data/pair-sessions.json`
- default pair-session TTL: `168` hours
- relay chunk bytes can also be stored in FortiBuckets object storage when configured

Important nuance:

- backend state is now restart-durable
- backend session persistence is no longer hard-coded directly into the route layer
- the FortiState package is now treated as the backend's live state kernel, not confused with the optional remote snapshot endpoint
- a dedicated remote snapshot service scaffold now exists under `private-components/state-service`
- relay media durability is stronger because blob chunks can live outside process memory
- backend state is still not a fully shared multi-node database layer until a compatible remote state service is used behind the persistence adapter

## 5. Device types and pairing behavior

Supported device types:

- `desktop`
- `web`
- `mobile`

Pairing behavior today:

- the desktop registers itself with label `Dropply desktop`
- phones register as `Dropply phone`
- desktop browsers register as `Dropply browser`
- each device stores a `transportPreference`

Removal behavior:

- `Unpair this desktop` removes the desktop from the session from the desktop side
- `Remove` on a paired device revokes that specific device from the token
- revoked devices cannot silently rejoin with the same device id

## 6. Network modes

Dropply now has a real transport policy switch.

### `p2p` mode

Meaning:

- media is expected to travel through the direct WebRTC path
- the UI only treats the direct path as connected media transport
- relay is not treated as a valid media fallback for image and file imports in this mode
- text and other manifest data still use the normal session/relay metadata channel

### `relay` mode

Meaning:

- media is expected to travel through hosted relay storage
- the desktop uploads relay blobs first, then publishes relay item metadata
- the pair dashboard loads media from relay without requiring a direct link

### Local mode storage

The desktop remembers the selected network mode in local storage:

- key: `dropply-network-mode`
- this preference now flows through the frontend FortiState-style store layer rather than ad-hoc component state

## 7. End-to-end transport flows

### Desktop -> phone or browser in `p2p`

1. Desktop registers with `transportPreference: "p2p"`.
2. Pair dashboard registers and sees the desktop preference.
3. Item metadata appears in the shared stream.
4. Media bytes are fetched over WebRTC data channels.
5. If the browser or phone disconnects after success, the desktop may still observe a normal transport teardown event.

### Desktop -> phone or browser in `relay`

1. Desktop registers with `transportPreference: "relay"`.
2. Desktop exports the media into relay blob chunks.
3. Desktop uploads `/v1/relay/blob/push` chunks.
4. Desktop publishes `/v1/relay/push` metadata only after blobs exist.
5. Pair dashboard loads `/v1/relay/item` and receives valid reconstructed base64.
6. When bucket storage is configured, Dropply stores relay chunk bytes in FortiBuckets and removes them again when the item is deleted.

### Phone or browser -> desktop

1. Pair dashboard publishes text or media to the session.
2. Media relay uploads use chunked `/v1/relay/blob/push`.
3. Desktop imports relay items.
4. For media, desktop fetches the full `/v1/relay/item` payload before import so file bytes are present.

## 8. Content classes

### Text

- stored as item metadata, not a relay blob
- rides the shared item stream
- works regardless of direct media availability
- best for note-sized content, not giant document bodies

### Image

- stored locally on desktop
- may appear inline in a relay snapshot if tiny enough
- otherwise travels as relay blob chunks in `relay` mode
- travels as direct WebRTC chunks in `p2p` mode
- may carry Zenith sidecar metadata when exported through the full relay-item path

### Video

- same transport model as image
- typically uses blob chunks in relay mode because inline manifest budgets are intentionally small
- should be expected to bypass Zenith synthesis in many cases because compressed video tends to be high entropy

### Arbitrary file

- same transport model as image/video
- desktop saves downloads directly to Windows Downloads
- browser downloads go to the browser's normal download destination
- may carry Zenith eligibility metadata, but generic files should not assume Zenith synthesis is available

### Smart Drop metadata

- stored as small JSON metadata on the item record
- syncs through relay/direct manifests with the item
- defaults safely when older peers omit it
- classified locally from item type, MIME type, file name, text preview, bundle source, and size
- does not use cloud AI or universal screen reading in v1

## 9. Transport limits by content class

These are the real transport-layer limits and behaviors in the current codebase, not marketing-plan caps.

| Content class | Direct `p2p` path | Relay path | Hard transport facts | Practical guidance |
| --- | --- | --- | --- | --- |
| Text | Not sent as a dedicated WebRTC file stream | Included in item metadata and relay manifest | No chunked blob path exists for text today. Large text must still fit inside the relay manifest budget. | Use for normal notes, links, snippets, and short clipboard payloads. |
| Image | WebRTC data channel chunks | Relay item metadata plus relay blob chunks | Direct chunks are `64 KiB`. Relay blob chunks are `128 KiB`. | Good fit for both `p2p` and `relay`. Very small images may ride inline in the manifest. |
| Video | WebRTC data channel chunks | Relay item metadata plus relay blob chunks | Same chunk sizes as image. Backend request body limit is `50 MiB` per request. | Large videos should use chunked relay or direct transfer, not inline manifest bytes. |
| Arbitrary file | WebRTC data channel chunks | Relay item metadata plus relay blob chunks | Same chunk sizes as image/video. | Good fit for both modes; relay handles bytes safely because blobs are chunked before metadata is published. |
| Manifest metadata | Not applicable | `/v1/relay/push` | Total snapshot JSON budget is `480 KiB`. Inline base64 budget inside that snapshot is `192 KiB`. | The manifest is for metadata first, not for bulk media. |

## 10. Exact low-level transfer constants

### Relay constants

- Relay snapshot JSON budget: `480 * 1024` chars (`480 KiB` target ceiling)
- Relay inline base64 budget inside a snapshot: `192 * 1024` chars (`192 KiB`)
- Relay blob chunk size: `128 * 1024` bytes (`128 KiB`)

Effects:

- oversized relay snapshots are trimmed oldest-first
- inline media is capped aggressively to avoid proxy/body-limit failures
- large media should live in relay blobs, not inline bytes

### Direct-transfer constants

- WebRTC chunk size: `64 * 1024` bytes (`64 KiB`)
- Per-channel buffered high-water mark: `2 * 1024 * 1024` bytes (`2 MiB`)
- Signaling poll interval: `2000 ms`
- Pair-dashboard direct-file request timeout: `8000 ms`

Effects:

- direct transfer is chunked and back-pressured
- the web client waits a bounded time for a direct media response

### Backend body and request limits

- Fastify body limit: `50 * 1024 * 1024` bytes (`50 MiB`) per request
- Global default rate limit: `120` requests per `60_000 ms`

Route-specific rate limits:

- `POST /v1/pair/register`: `60/min`
- `POST /v1/relay/push`: `120/min`
- `POST /v1/relay/blob/push`: `480/min`
- `POST /v1/webrtc/signal`: `240/min`

## 11. Product plan limits

These are the published plan-limit values exposed by the backend. They are not the same thing as transport chunk sizes.

| Plan | Monthly synced items | Max upload size | Max devices | Notes |
| --- | --- | --- | --- | --- |
| Free | `500` | `25 MB` | `3` | `hosted_sync_required_login: true` in the published plan object |
| Pro | `10,000` | `250 MB` | `12` | larger hosted limits |
| Team | pooled | not listed as a simple scalar | not listed as a simple scalar | `pooled_limits`, `private_streams`, `admin_controls` |

Important nuance:

- these values are published at `/v1/plans/limits`
- they are not the same as the low-level relay manifest and chunk constants

## 12. Downloads and file placement

### Desktop app

- downloading from the desktop app writes directly to the Windows Downloads folder
- duplicate names are auto-adjusted instead of overwriting silently
- no save dialog is required for the normal desktop download flow

### Browser or phone dashboard

- downloads use the browser's normal download behavior
- if the browser is configured to ask where to save, the browser still controls that

## 13. Auth and hosted cloud behavior

Current truth:

- local pairing does not depend on sign-in
- hosted auth endpoints exist
- hosted auth is not the center of the current local-first flow
- the desktop now checks backend config before presenting hosted auth as usable

Live backend config currently reports:

- `hosted_sync_available: false`
- `auth_configured: false`

Meaning:

- hosted auth UI should stay hidden or soft-disabled unless the backend later reports it as configured

## 14. Release and packaging

Current release baseline:

- source semver: `1.0.0`
- release bundle version: `1.0.0`
- release bundle label: `EN-FR`
- product wedge: Smart Drops v1 inside Dropply

Why both appear:

- Tauri and Cargo use normal semver safely at `1.0.0`
- the desktop and pair dashboard are bilingual even though versioning stays normal semver
- installer artifacts are emitted as bilingual `EN-FR` bundles
- Smart Drops are additive metadata on existing items, not a separate object store or product rename

This is intentional and correct.

## 15. Deployment and operations

### Frontend surfaces

- desktop app is local
- hosted pair dashboard ships through Vercel after GitHub pushes

### Backend surface

- hosted backend deploys through FortiCore using project name `dropply-backend`
- FortiCore should keep `sourceDirectory` pinned to `private-components/backend`
- use `scripts\deploy-dropply-backend.cmd` from the repo when deploying; it prefers the local FortiCore `1.2.5` package, builds a backend-only temp bundle, vendors `packages/fortistate-kernel` into that bundle, and deploys the Fastify package without packaging the full repo root as a frontend app
- FortiBuckets-backed relay blobs use:
  - `BLOB_STORAGE_ENDPOINT`
  - `BLOB_STORAGE_BUCKET`
  - `BLOB_STORAGE_BASIC_USERNAME`
  - `BLOB_STORAGE_BASIC_PASSWORD`
  - `BLOB_STORAGE_PREFIX`

Operational verification used in this release cycle:

- live endpoint checks against `dropply-backend.fortifie.com`
- FortiCore logs review
- live FortiBuckets write/read/delete verification through Dropply relay routes
- desktop-to-web and desktop-to-phone transfer validation for photo and video through both `p2p` and `relay`

## 16. Known limitations

- text does not have a separate chunked relay blob path today
- backend pair-session persistence is durable across restarts but not yet a shared database-backed cluster store
- FortiBuckets currently uses control-plane Basic Auth credentials; a dedicated service credential would be cleaner long-term
- published plan limits exist, but transport-level limits still matter independently
- `p2p` mode is intentionally strict now; if the direct path is unavailable, media should not pretend relay is a valid import path for that mode
- `PAIR_STATE_PROVIDER=fortistate` is now a real backend config target, but it currently falls back to the JSON provider until a live FortiState adapter is wired
- relay items can now carry structured Zenith metadata in addition to the optional `zenith_equation` field
- Smart Drops v1 does not yet perform automatic routing decisions, cloud AI summarization, file comparison, or project attachment

## 17. Troubleshooting notes

### "No Access-Control-Allow-Origin" on a relay request

Usually means:

- the upstream proxy or body limit rejected an oversized request before normal CORS headers were attached

First things to check:

- relay snapshot stayed under the manifest budget
- media used blob-chunk upload instead of giant inline base64

### "Relay item is missing file bytes"

Meaning:

- the client imported metadata without fetching the full relay item payload

Current status:

- fixed in the current desktop app path

### "The direct media link encountered a transport error"

Meaning:

- the direct channel saw a disconnect or teardown

Current interpretation:

- if the transfer already completed, this can simply reflect the browser or phone leaving the session

## 18. Plain-English product truth

If you only remember five things, remember these:

1. The desktop is the main stream owner.
2. `p2p` means direct media transfer is the real path.
3. `relay` means hosted chunked media transfer is the real path.
4. Small metadata rides the manifest; large media rides blob chunks.
5. Relay blobs are now bucket-durable, but session metadata is still not a shared multi-node state store.
