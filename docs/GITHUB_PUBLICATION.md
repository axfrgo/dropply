# GitHub Publication

Use the public desktop core as the GitHub-facing repository and keep the hosted platform private.

## Recommended split

- Public GitHub repo:
  - `src/`
  - `src-tauri/`
  - `src-tauri/src/bin/dropply-cli.rs`
  - `src-tauri/src/cli_tui.rs`
  - `docs/` for public desktop-core docs
  - desktop release workflows and scripts
- Private repo or private workspace:
  - hosted backend
  - hosted web app
  - mobile app
  - billing, relay control plane, admin tooling

## Public repo prep

Run:

```powershell
./scripts/prepare-public-repo.ps1
```

This creates a sanitized copy of the repo that excludes local artifacts and the `private-components/` directory.
It also strips the private-only `zenith-core` path dependency from the public `src-tauri/Cargo.toml`.

To refresh the actual public GitHub repo from the sanitized copy:

```powershell
./scripts/sync-public-github-repo.ps1
```

## Release path

- Push the public repo to GitHub.
- Tag releases like `v1.0.0`.
- Let GitHub Actions build artifacts and generate checksums.
- Publish installers through GitHub Releases.
- Keep the Clerk-backed web app and private backend in your private repo or private workspace.
