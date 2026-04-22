# Auth And Identity

## Rule

Identity is optional for local use and required for hosted cross-platform sync.

## Production provider

- Clerk for hosted identity, sessions, passkeys, social login, and account lifecycle

## User-facing sign-in methods

- Google OAuth
- email magic link
- passkeys

## Recommended rollout order

1. Google OAuth
2. Magic link
3. Passkeys

## Why this mix works

- Google reduces signup friction
- magic links cover users who do not want social login
- passkeys create a premium, modern security path

## Account boundaries

Local-only desktop usage:

- no account
- no hosted dependency
- no subscription gate

Hosted sync mode:

- account required
- device registration required
- session-based access to web/mobile sync surfaces

## Device model

Each signed-in account should maintain:

- user id
- device ids
- device labels
- last seen timestamps
- revocation state

## Recommended session approach

- Clerk-managed hosted sessions
- device-bound session records
- passkey challenge support
- backend verification of Clerk-issued identity context

## Recovery

Account mode should support:

- signed-in device list
- remote device revocation
- session expiration
- re-auth for sensitive actions
