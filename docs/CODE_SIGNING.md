# Code Signing

Dropply should be code-signed before public Windows distribution.

## Windows

- Obtain an Authenticode certificate from a trusted code-signing provider.
- Export the certificate as a password-protected `.pfx`.
- Store the certificate in GitHub Actions secrets as:
  - `WINDOWS_CERTIFICATE_BASE64`
  - `WINDOWS_CERTIFICATE_PASSWORD`
- The GitHub workflow at `.github/workflows/release-desktop-windows.yml` will import the certificate and sign the MSI and NSIS artifacts when those secrets are present.

## Recommended production setup

- Use a hardware-backed or cloud-backed signing certificate where possible.
- Timestamp every signed artifact.
- Sign installers before checksums are published.
- Keep certificate rotation and revocation procedures documented.
