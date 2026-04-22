# Dropply Platform Architecture

## Product split

Dropply is intentionally split into two layers.

Open-source core:

- local-first desktop application
- local storage and item lifecycle
- single-stream scratchpad
- local pairing groundwork
- optional self-hosted relay foundation

Private hosted platform:

- web app
- mobile apps
- account system
- managed cloud sync
- subscriptions and feature controls
- private/team collaboration surfaces

## Product modes

### 1. Local mode

No account required.

Capabilities:

- use desktop immediately
- store items locally
- add/delete/export items
- local device pairing by code or QR
- no hosted dependency

### 2. Cloud account mode

Optional sign-in required.

Capabilities:

- cross-platform sync across desktop, web, and mobile
- hosted identity
- device recovery
- private synced account stream
- future paid storage and team features

## Client surfaces

### Desktop

- Tauri + React + Rust
- remains the flagship local-first surface
- works without login
- can optionally attach to an account

### Web

- private hosted application
- intended for Vercel deployment
- account-first experience
- optimized for quick access to synced stream data

### Mobile

- private hosted/account-connected application
- account-first for reliable sync and recovery
- local cache with online reconciliation

## Service topology

### Public edge

- Vercel-hosted web frontend
- auth UI
- marketing/download site
- account onboarding flows
- Clerk-hosted or Clerk-backed identity surfaces

### Private backend

- sync API
- account/device registry
- pairing and device authorization
- relay orchestration
- rate-limit enforcement
- billing and entitlement checks

### Storage model

- local desktop storage remains SQLite + blobs
- hosted cloud stores metadata and signed blob references
- blob storage should be object-store backed in the private platform

## Sync model

### Free / local-first path

- local storage first
- direct device linking where possible
- code/QR pairing
- account not required

### Account-backed hosted path

- account-scoped device graph
- hosted metadata exchange
- secure blob transfer paths
- policy-based limits and plan enforcement

## Security baseline

- no-login local usage stays supported
- account mode uses signed sessions and device authorization
- hosted sync should support encryption roadmap from day one
- secrets and signing keys never live in public clients

## UX principle

The user experience must remain simple:

- use Dropply instantly without an account
- sign in only if you want hosted sync, web access, or mobile continuity
