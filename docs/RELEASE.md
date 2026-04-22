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
