---
integration: website-signed-install
status: implementation
last_verified: 2026-10-18
selected_spec_version: JARVIS-RELEASE-1
selected_sdk: none; Cosign v3.1.3 standalone verifier
---

# Website signed installation

## Scope and official sources

Promote the existing signed prebuilt installer to the website default. No release
discovery API, new provider, dependency or signature format is introduced.

Sources accessed 2026-10-18 UTC:

- GitHub index: <https://docs.github.com/llms.txt> could not be read by the browser
  (unsupported Markdown content type); followed official documentation instead.
- Exact release links: <https://docs.github.com/en/repositories/releasing-projects-on-github/linking-to-releases>.
- Download limits/HTTPS/redirect options: <https://curl.se/docs/manpage.html>.
- Signature identity and issuer: <https://docs.sigstore.dev/cosign/verifying/verify/>.
- Pinned verifier release: <https://github.com/sigstore/cosign/releases/tag/v3.1.3>.
- PowerShell string execution and scope:
  <https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.utility/invoke-expression?view=powershell-5.1>.
- Selected public prerelease:
  <https://github.com/props-nothing/jarvis-core/releases/tag/ci-a176e79843a22eae2802d97aafba74a0bf6e282f>.

The existing [prebuilt contract](prebuilt-install.md) and
[signature contract](release-signing.md) remain controlling for extraction and
cryptography. No relevant change observed in the consulted contracts; curl >=8.4,
Windows PowerShell >=5.1, Bash >=3.2 and Cosign 3.1.3 remain selected. No SDK added.

## Contract and decisions

The standalone website scripts embed an exact release tag and package version
0.1.0, plus the existing platform-specific Cosign SHA-256 pins. Public HTTPS GETs
fetch the manifest and bundle, then the checksum-pinned verifier. Verification
requires GitHub Actions' OIDC issuer and this repository's release workflow on
main. The strict manifest must match the embedded tag, commit, version and platform.
Only then may the archive be downloaded, hashed, narrowly extracted and executed.
No credentials, API scopes, tokens, webhooks or pagination are involved.

Downloads retain 15-second connection/120-second total deadlines, five HTTPS
redirects, 8 KiB manifest/1 MiB bundle/256 MiB archive caps. Signature verification
has a two-minute deadline. Decompression stays bounded. Failure stops installation;
there is no source-build or unsigned fallback and no automatic retry. Existing
installation bytes are preserved; updates use the separate offline maintenance
commands. Setup and service startup remain separate, so piped stdin never collects
keys. User data and grants remain owned by the Rust program.

GitHub and Sigstore receive ordinary download/trust-metadata requests and network
metadata, never profile content or keys. No paid API calls or application telemetry
are introduced. Bootstrap delivery over the website's HTTPS origin remains a trust
root: signing the payload does not authenticate a compromised bootstrap itself.

Website scripts are maintained in the independent frontend repository. They reuse
the core bootstrap implementation with fixed selectors and without the historical
unsigned interface. The source tab remains an explicit manual build. Updating the
pin requires reviewed code, public-asset verification and native installation tests;
neither mutable `latest` nor the newest untested commit chooses an executable.

## Verification plan and unresolved gates

- Execute Bash and Windows PowerShell fixtures with real archives; prove signature,
  verifier hash, manifest identity, archive hash and version failures precede install.
- Execute the actual stdin/Invoke-Expression entry points and test existing-install
  preservation, cleanup, separate setup and install failure propagation.
- Falsify the signature guard and require the rejection test to fail, then restore it.
- Install the real public signed Windows archive with a fresh profile and restricted
  PATH; run version/doctor and a refused repeat installation.
- Add native Linux/macOS/Windows CI using the same published website scripts.

Native Authenticode/notarization, complete dependency notices/SBOM, clean-machine
onboarding/logon and Intel macOS runtime acceptance remain separate gates. Existing
hosted core acceptance does not by itself prove the new website entry points.

## Local verification results

2026-10-18: 19 frontend tests, both real shell parsers, production build and exact
built-installer byte comparison pass. Actual signed Windows public installation
passes with development tools hidden from PATH: both version commands, fresh-profile
diagnosis and unchanged existing-install refusal. Removing the signature call in
disposable Bash/PowerShell copies makes each fixture suite fail. Production files
are never mutated for falsification.

Workspace formatting, strict all-target/all-feature Clippy, 3267 workspace tests
(6 existing opt-in ignores), six documentation tests, both repositories' link
checks and frontend workflow lint pass.

## Native website and production verification

2026-10-18: all seven jobs in [website CI 39131200970](https://github.com/props-nothing/jarvis-frontend/actions/runs/39131200970)
pass for `66915f1`: three native fixture/parser/build jobs and four actual signed
public installations (Windows x86_64, macOS ARM64, Linux x86_64/ARM64). No CI fixes
were required. A temporary Vercel deployment hold prevented publication until success;
the hold-only removal `11d7a1f` then deployed through the existing integration.
All pushes went directly to main; no branch or PR was created.

Production verification at 13:24 UTC confirms both scripts match committed checkout
bytes, accounting for the declared LF/CRLF attributes, and both serve as
`text/plain; charset=utf-8` with nosniff. JavaScript/CSS match the tested build;
HTML matches after build-host line-ending normalization; the apex redirects to www.
Hashes and exact commits are in the [website deployment record](https://github.com/props-nothing/jarvis-frontend/blob/main/docs/DEPLOY.md#signed-default-revision-2026-10-18).
No clean-machine or native-certificate acceptance is inferred.
