---
integration: prebuilt-install
status: implemented
last_verified: 2026-10-17
owners: []
selected_spec_version: "GitHub public release assets; curl >=8.4.0; Bash >=3.2; Windows PowerShell >=5.1 / .NET Framework ZIP APIs"
selected_sdk: "none"
---

# Pinned prebuilt installation

## Question and sources

Download an explicitly selected existing public archive without Git, Rust, Node,
Python or GitHub CLI; check an operator-supplied SHA-256 before opening or running
it. This is the first P12-001/P9-004 slice, not signed distribution or an updater.

Official sources accessed 2026-10-17 UTC before implementation:

- https://curl.se/llms.txt and https://learn.microsoft.com/llms.txt were not
  accessible through the research fetcher; continued to official references.
- https://docs.github.com/en/repositories/releasing-projects-on-github/linking-to-releases
  and https://docs.github.com/en/rest/releases/assets?apiVersion=2022-11-28:
  public assets expose browser download URLs. Use the repository's existing
  `releases/download/<tag>/<asset>` layout, not the latest-release alias or REST
  authentication. Exact names come from `scripts/package-release.sh`.
- https://curl.se/docs/manpage.html: `--disable` first suppresses curlrc;
  `--proto`, `--proto-redir`, `--fail`, `--location`, `--max-redirs`,
  `--connect-timeout`, `--max-time` and `--max-filesize` constrain downloads.
  Before 8.4.0, unknown Content-Length bypassed the file-size limit; require
  curl 8.4.0 or later, including the Windows `curl.exe` application.
- https://raw.githubusercontent.com/libarchive/libarchive/v3.8.1/tar/bsdtar.1:
  listing selects members; `-xO` writes selected contents to stdout. Use a fixed
  member, check it is exactly one regular file, and write stdout to an owned
  path. Never extract archive-supplied paths. GNU tar compatibility requires
  execution tests as well as the native macOS bsdtar lane.
- https://learn.microsoft.com/en-us/dotnet/api/system.io.compression.ziparchiveentry?view=netframework-4.8.1:
  `FullName`, `Length`, `ExternalAttributes` and `Open()` allow bounded reading
  of one exact ZIP member without `ExtractToDirectory`. Reject duplicate names,
  links, empty entries and oversized uncompressed payloads.
- https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.utility/get-filehash?view=powershell-5.1:
  SHA-256 file hashing is available in Windows PowerShell 5.1.

## Contract and trust

No credentials, scopes, webhooks, SDK, API quota or provider types are involved.
Downloads reveal the caller's IP and selected public release to GitHub/CDN.
TLS authenticates the download hosts. An explicit SHA-256 authenticates bytes
only relative to the supplied digest; a checksum fetched from the same compromised
release would not establish publisher identity. Current archives are unsigned.
No signed-release, notarization, provenance or immutable-tag guarantee is added.

Only `ci-<40 lowercase hex>` and numeric `vMAJOR.MINOR.PATCH` tags and numeric
package versions are accepted. No arbitrary repository/base URL or moving alias.
Downloads use HTTPS including redirects (at most five), a 15-second connection
timeout, 120-second total deadline and 256 MiB ceiling; no automatic retries.
Binary materialization also has a 256 MiB ceiling. Platform selection is native
Linux/macOS x86_64/ARM64 or Windows x86_64; unsupported targets fail before network.
Intel macOS remains cross-built without native binary-smoke evidence.

After verification, check `jarvis version` matches the requested package, then
delegate to existing per-user `jarvis install`. First installs only; an existing
destination executable is refused. No service, setup, profile, provider call or
update is started. An explicit absolute directory and no-PATH mode allow isolated
acceptance. Failures remain nonzero, with no success message, and staging is cleaned.

## Verification and unresolved limits

Execute native scripts against real archives with inert fixture programs. Prove
wrong hashes, missing/duplicate/link members, malformed selectors, unsupported
platforms, download errors, version mismatch and install failure never install.
Mutate the checksum guard and require the corrupt-byte test to fail. Exercise
the real GitHub Windows asset in an isolated directory without changing PATH.
CI must run Bash on Linux/macOS and PowerShell on Windows. A local Git Bash pass
is not native macOS/Linux evidence. Signed publisher verification, old OS curl
availability, clean-machine onboarding, update/schema rollback and uninstall
lifecycle remain separate gates. No third-party implementation was copied.

## Evidence, 2026-10-17

Native Windows PowerShell **5.1** and local Git Bash execution passed successful
installation, wrong digest, malformed selectors, duplicate/missing/link/directory
members, oversized decompression, download failure, version mismatch, install
failure, existing-install preservation and staging cleanup. Bash additionally
checks the curl version floor and transport options. The Bash fixtures use real
tar/gzip and executable shell peers; Windows uses real ZIP streams and an inert
program-invocation seam. Neither substitutes for native clean-machine acceptance.
Removing each script's checksum guard in a disposable copy made its corrupt-hash
test fail with the expected refusal assertion; production guards stayed intact.
CI runs the fixture scripts on native Ubuntu/macOS and Windows PowerShell 5.1.

Real public asset accepted by the unmodified Windows download/extraction/execution
path (curl 8.21.0, PowerShell 5.1):

- Release: https://github.com/props-nothing/jarvis-core/releases/tag/ci-a402d5ba373bfee2433a635bd6aceede624a5926
- Archive: `jarvis-0.1.0-windows-x86_64.zip`, 16,573,709 bytes.
- SHA-256: `040a76d9344648f46c64ddadff893c7597990055d2785726dc5b5e8bb65ee0bb`.
- Isolated absolute destination with `-NoPath`; no owner profile/service/startup.
- Repeated with PATH restricted to Windows System32 and Windows PowerShell 5.1;
  no Git, Cargo, rustc, Node, Python or GitHub CLI was discoverable. The actual
  download/install and both version commands still passed.
- Installed `jarvis version`: `jarvis-cli 0.1.0`; `jarvis daemon --version`:
  `jarvisd version=0.1.0 target=x86_64-windows config_schema=1 database_schema=31`.
- Local ignored logs under `artifacts/p12-001/prebuilt/`. This is a development
  host and the existing prerelease, not a clean machine or acceptance of the new
  script revision on other operating systems.

Baseline: 3,228 workspace tests pass, zero failures, six existing opt-in ignores;
61 release-flow and six documentation tests pass, with formatting, strict
all-feature/all-target Clippy, local links and dependency policy. No Rust source,
schema, protocol or dependency changes were needed for this slice.
