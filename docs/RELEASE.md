# Release Packaging

## Version

Current release target: `1.0.0`

## Windows bundles

Produced by:

```bash
npm run tauri:build
```

Artifacts:

- `src-tauri/target/release/bundle/msi/Dropply_1.0.0_x64_en-US.msi`
- `src-tauri/target/release/bundle/nsis/Dropply_1.0.0_x64-setup.exe`

## Cross-platform release guidance

- Windows bundles must be built on Windows
- macOS bundles should be built on macOS runners or machines
- Linux bundles should be built on Linux runners or machines

## Recommended public release assets

- MSI installer
- NSIS setup executable
- checksums
- changelog
- security contact
- open-core feature matrix
- GitHub Releases page
- landing page download links

## v1.0.0 release gate

Before publishing the installers, verify:

- existing local databases migrate and old rows show safe Smart Drop defaults
- pasted text, drag/drop files, file picker imports, browser-share bundles, relay imports, and direct imports show source context, labels/tags, suggested actions, and lifecycle state
- desktop Smart Drop actions still preserve existing copy/download/delete behavior
- TUI can list, search, open, mark pending, mark completed, and revoke Smart Drops
- hosted pair dashboard can display Smart Drop metadata and create metadata for web-originated text/file uploads
- older relay payloads without Smart Drop fields still import normally
