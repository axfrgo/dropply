# Dropply

Dropply is a premium local-first cross-device intent layer for text, files, images, links, and fast device handoff. The desktop app is built with Tauri, React, and Rust, with SQLite metadata storage, blob-backed local persistence, and sync foundations designed for WebRTC-first transfer with relay fallback.

The v1 wedge is **Smart Drops**: every item in the stream can carry where it came from, a local label/summary, suggested next actions, target-device intent, and lifecycle state. Smart Drops are additive metadata on the existing Dropply stream.

## Product

- Drag in files, screenshots, images, and text
- Type or paste directly into the built-in composer
- Keep everything in one time-ordered Smart Drops stream
- See source context, local labels, tags, suggested actions, and lifecycle state
- Export or delete items individually
- Pair devices without accounts
- Use the desktop app or the TUI/CLI companion for terminal-heavy workflows
- Pin the desktop window on top when you want a persistent drop surface

## Repository layout

- `src/`: React desktop UI
- `src-tauri/`: Tauri host, Rust storage layer, CLI/TUI, commands, and desktop packaging config
- `relay-server/`: optional relay service foundation
- `docs/`: release, CLI, and open-core documentation

## Open-core model

Dropply is structured as an open-core product.

Open-source desktop core:

- local-first desktop app
- single shared stream
- local persistence
- item import, export, and deletion
- device pairing groundwork
- release packaging

Private / hosted tier:

- managed relay and hosted sync coordination
- account-backed device recovery
- team/private streams
- paid cloud plans and policy controls
- hosted APIs, billing, and admin services

See [OPEN_CORE.md](docs/OPEN_CORE.md) for the current boundary.
See [DROPPLY_CLI_QUICKSTART.txt](docs/DROPPLY_CLI_QUICKSTART.txt) for CLI/TUI setup.
See [CODE_SIGNING.md](docs/CODE_SIGNING.md) for Windows signing setup.

## Release status

`v1.0.0` is the first packaged desktop release baseline.

Included:

- polished premium desktop UI
- local Smart Drops for text, files, images, links, and browser bundles
- source context, local labels/tags, suggested actions, and lifecycle state
- SQLite + blob storage
- item open/copy/download/delete plus pending/completed/revoked intent updates
- desktop always-on-top pin toggle
- relay/direct Smart Drop metadata compatibility
- companion CLI/TUI stream surface
- Windows installer packaging

Still in progress:

- automatic cross-device routing decisions
- richer hosted continuity adapters
- multi-stream workspaces
- end-to-end hosted private tier

## Development

Prerequisites:

- Node.js 20+
- Rust stable
- Tauri prerequisites for your OS

Run the desktop app:

```bash
npm install
npm run tauri:dev
```

Run the relay server:

```bash
cd relay-server
cargo run
```

Build release bundles:

```bash
npm run tauri:build
```

Generate checksums:

```powershell
./scripts/generate-checksums.ps1
```

## Release artifacts

Windows release bundles are emitted under:

- `src-tauri/target/release/bundle/msi/`
- `src-tauri/target/release/bundle/nsis/`

See [RELEASE.md](docs/RELEASE.md) for packaging notes and distribution guidance.

## Security

Desktop hardening included in this repo:

- scoped asset protocol access
- tightened packaged-app CSP
- unused shell capability removed
- local-first storage design

See [SECURITY.md](SECURITY.md).

## Licensing

The open-source desktop core is released under the MIT license in [LICENSE](LICENSE).

Private hosted and proprietary components are not part of the open-source license. See [LICENSE-COMMERCIAL.md](LICENSE-COMMERCIAL.md).
