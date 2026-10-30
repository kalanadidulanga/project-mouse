# Security policy

## Reporting a vulnerability

Please do not open a public issue. Use GitHub's private reporting instead:
[Report a vulnerability](https://github.com/kalanadidulanga/project-mouse/security/advisories/new).
You will get a reply within a week.

Worth reporting: anything that lets someone else run code through project-mouse, its installer or
its updates, or that keeps a power request or other system change alive after it exits.

## Supported versions

Only the latest release gets fixes. The app updates itself, so the tray will offer the fix.

## How releases are protected

- Every release is built by GitHub Actions from a `v*` tag, after the CI checks pass.
- Updates are signed; installed copies refuse an update that is not signed with the project key.
- The signing key is available only to the release job, and each release run is approved by hand.
