# Production Launch Checklist

This is the practical path to make Dropply production-ready, downloadable, and publishable.

## 1. Public Repo Shape

- Create a GitHub repository for the open-source desktop core.
- Keep `private-components/` private or move it into a separate private repository before public launch.
- Remove local-only logs and machine-specific artifacts before first push.
- Add a strong repo description, screenshots, and release badges.

## 2. Download Distribution

- Publish the Windows installers through GitHub Releases.
- Link the installers from your landing page and hosted web product.
- Add SHA-256 checksum files for every installer.
- Add release notes for every tagged version.
- Code-sign Windows builds before public distribution.

## 3. Web + Cloud Deployment

- Deploy the private web app on Vercel.
- Deploy the private backend on your own server behind HTTPS.
- Put the API behind a stable subdomain such as `api.dropply.com`.
- Put the web app behind a stable domain such as `app.dropply.com`.
- Add environment separation for development, staging, and production.
- Configure Clerk for the production web domain before go-live.

## 4. Security Before Real Users

- Move hosted identity to Clerk in production.
- Wire Clerk to Google OAuth, magic-link email flows, passkeys, and session management.
- Set `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY`, `CLERK_SECRET_KEY`, and `CLERK_JWT_KEY` in production.
- Store sessions securely with rotation, expiry, and revocation support.
- Add CSRF protection where cookies are used.
- Add per-endpoint rate limiting for auth, pairing, uploads, and relay traffic.
- Add audit logs for sign-in, session creation, device linking, and administrative actions.
- Add encrypted secret storage and never keep secrets in the repo.
- Finish the future end-to-end encryption design before promising private cloud sync.

## 5. Reliability

- Add crash reporting for desktop, web, mobile, and backend.
- Add structured backend logging with request IDs.
- Add health checks and uptime monitoring.
- Add automated backups for cloud metadata stores.
- Add staging smoke tests before every production deploy.

## 6. Product Gaps To Finish

- Complete real WebRTC sync instead of scaffolding.
- Complete relay fallback with auth-bound access control.
- Complete actual account session handling on desktop, web, and mobile.
- Add real device management, logout, and revoke-session flows.
- Add update strategy for the desktop app.
- Add deletion, export, and retention rules for hosted accounts.

## 7. Legal + Trust

- Add a Privacy Policy.
- Add Terms of Service for hosted plans.
- Add a support contact and security disclosure address.
- Define what is open-source and what remains proprietary.

## 8. GitHub Release Flow

1. Push the cleaned desktop core to GitHub.
2. Tag a release such as `v1.0.0`.
3. Upload the signed installers and checksum files to GitHub Releases.
4. Publish release notes.
5. Point your marketing site and web app download buttons to the GitHub Release assets.

## 9. Recommended Near-Term Order

1. Finish the Dropply rebrand in all shipped surfaces.
2. Split public core vs private hosted code clearly.
3. Push the desktop core to GitHub.
4. Deploy the private web app to Vercel.
5. Deploy the private backend behind your own domain and TLS.
6. Move hosted auth to Clerk and finish production wiring.
7. Add code signing and public download pages.
