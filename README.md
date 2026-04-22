# Dropply

Dropply is a premium local-first shared scratchpad for text, files, images, and fast device handoff. The desktop app is built with Tauri, React, and Rust, with SQLite metadata storage, blob-backed local persistence, and sync foundations designed for WebRTC-first replication with relay fallback.

## Product

- Drag in files, screenshots, images, and text
- Type or paste directly into the built-in composer
- Keep everything in one time-ordered stream
- Export or delete items individually
- Pair devices without accounts
- Pin the desktop window on top when you want a persistent drop surface

## Repository layout

- `src/`: React desktop UI
- `src-tauri/`: Tauri host, Rust storage layer, commands, and desktop packaging config
- `relay-server/`: optional relay service foundation
- `docs/`: release, security, and open-core documentation
- `private-components/`: documentation-only placeholder for proprietary modules and hosted services

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
See [PLATFORM_ARCHITECTURE.md](docs/PLATFORM_ARCHITECTURE.md) for the hosted/private architecture direction.
See [AUTH_AND_IDENTITY.md](docs/AUTH_AND_IDENTITY.md) for the account model.
See [PLANS_AND_LIMITS.md](docs/PLANS_AND_LIMITS.md) for the free vs paid hosted limits plan.
See [PRODUCTION_READY.md](docs/PRODUCTION_READY.md) for the production launch checklist.
See [GITHUB_PUBLICATION.md](docs/GITHUB_PUBLICATION.md) for the public-vs-private repo split.
See [CODE_SIGNING.md](docs/CODE_SIGNING.md) for Windows signing setup.
See [PRIVACY_POLICY.md](docs/PRIVACY_POLICY.md) and [TERMS_OF_SERVICE.md](docs/TERMS_OF_SERVICE.md) for hosted-service legal baseline.

## Release status

`v1.0.0` is the first packaged desktop release baseline.

Included:

- polished premium desktop UI
- local text/file/image capture
- SQLite + blob storage
- item export and deletion
- desktop always-on-top pin toggle
- Windows installer packaging

Still in progress:

- production-complete cross-device WebRTC sync
- production-complete relay/cloud sync
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

Prepare a sanitized public GitHub repo copy:

```powershell
./scripts/prepare-public-repo.ps1
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
