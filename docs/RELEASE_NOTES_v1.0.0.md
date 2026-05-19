# Dropply v1.0.0

## Included in this release

- premium desktop UI with a chat-like composer and Smart Drop cards
- local-first text, image, file, link, and browser-bundle capture
- Smart Drop source context, local labels/tags, suggested actions, and lifecycle state
- local deterministic classification with no cloud AI or model dependency
- SQLite metadata storage with blob-backed persistence
- additive SQLite migration for Smart Drop metadata on existing stream items
- item open, copy, download, delete, pending, completed, and revoked actions
- QR/code-based pairing with relay/direct Smart Drop metadata compatibility
- companion CLI/TUI workflows for terminal-first users
- hosted pair dashboard display and creation of Smart Drop metadata
- browser-share bundle compatibility with source URL/title context
- packaged Windows installer output

## Security and hardening

- unused shell plugin removed
- tighter packaged-app CSP
- scoped asset protocol access to Dropply data directories
- local-first storage with no account requirement for the core app
- Smart Drops v1 uses local heuristics only; no universal screen reading and no cloud-AI calls

## Not fully complete in v1.0.0

- automatic routing decisions across presence-aware target devices
- cloud AI summarization, version comparison, and project attachment adapters
- multi-stream workspaces
- hosted private tier
- account system
