---
integration: release-signing
status: implemented
last_verified: 2026-10-17
owners: []
selected_spec_version: "Sigstore bundle; GitHub Actions OIDC"
selected_sdk: "Cosign v3.1.3 (11926fa5bbbbde47e88fc006b625a17769b743b2), standalone executable"
---

# Release signatures and bootstrap verification

## Scope and controlling sources

Authenticate a release manifest with the official repository's release workflow
identity, then verify its selected archive before extraction/execution. Keep native
Apple notarization and Windows Authenticode separate: Sigstore does not grant
either OS's publisher reputation or remove its security prompts.

Official sources read live on 2026-10-17 UTC:

- https://docs.sigstore.dev/llms.txt was inaccessible; GitHub's
  https://docs.github.com/llms.txt returned unsupported text/markdown through the
  research fetcher. Continued to official pages and pinned source.
- https://docs.sigstore.dev/cosign/signing/signing_with_blobs/:
  `sign-blob --bundle` stores signature, certificate and transparency evidence.
- https://docs.sigstore.dev/cosign/verifying/verify/:
  `verify-blob --bundle --certificate-identity --certificate-oidc-issuer` verifies
  both signed bytes and expected identity. Never disable SCT/tlog verification.
- https://docs.sigstore.dev/quickstart/quickstart-ci/ and
  https://docs.github.com/en/actions/reference/security/oidc:
  GitHub Actions uses short-lived OIDC with `id-token: write`; expected issuer
  `https://token.actions.githubusercontent.com`. Scope permission to publication.
- https://raw.githubusercontent.com/sigstore/cosign/v3.1.3/pkg/providers/github/github.go:
  Cosign obtains its GitHub token from the Actions-provided request URL/token;
  those credentials are never supplied to installers or persisted in assets.
- https://raw.githubusercontent.com/sigstore/cosign/v3.1.3/cmd/cosign/cli/sign/sign_blob.go
  and https://raw.githubusercontent.com/sigstore/cosign/v3.1.3/cmd/cosign/cli/verify/verify_blob.go:
  bounded command context, bundle and trusted-material handling.
- https://raw.githubusercontent.com/sigstore/cosign/v3.1.3/cmd/cosign/cli/options/verify.go:
  identity/issuer and GitHub claim constraints; insecure switches are not used.
- https://github.com/sigstore/cosign/releases/tag/v3.1.3 and the live release-assets
  API supplied the immutable executable versions/checksums below. The downloaded
  Windows executable matched its digest and reported v3.1.3, commit above,
  Go 1.26.4. Its actual `verify-blob --help` confirms bundle/identity/issuer and
  global `--timeout` (default three minutes); JARVIS selects two minutes.

No SDK is linked into JARVIS, no cryptographic algorithm is implemented locally,
and no third-party source is copied. Cosign is Apache-2.0. The installer downloads
the original standalone verifier temporarily from its official release, pinned
by SHA-256, so end users need no Git, GitHub CLI, compiler, Node or Python.

| Asset at `https://github.com/sigstore/cosign/releases/download/v3.1.3/` | SHA-256 |
| --- | --- |
| `cosign-linux-amd64` | `4629c757b7618056f8ddd7e2625ae9fdd94c0372a65049520bc7d9df9efc7f71` |
| `cosign-linux-arm64` | `c5d324e091826b0d7a78eb16fef316450b4eb9aaec045611c08ba06f5e73220a` |
| `cosign-darwin-amd64` | `2347488e5d5b25336644024dfeca5601b190e91197a71a917bda44744aff106c` |
| `cosign-darwin-arm64` | `5cf948c2f4dfe59687bdd0b8523709067383e03982cc543475c8a7dc70e92a76` |
| `cosign-windows-amd64.exe` | `9fe59be0eca1271873ce019061335eb1ac419b7059202e797828467ddabe33be` |

## JARVIS decisions and limits

Sign a small strict text manifest containing repository, exact release tag, tested
commit, package version and all five archive names/sizes/SHA-256 values. Signing
the tag prevents replaying an older correctly signed archive as a new CI release.
Only the official `.github/workflows/release.yml` on main or the selected numeric
version tag is accepted. A signature, identity, manifest or digest failure aborts
before executing JARVIS; signed mode has no fallback to the existing explicitly
checksum-pinned unsigned path. Sign and verify before uploading/publishing, then
verify GitHub's received bytes for the manifest and bundle as well as packages.

The verifier pin and bootstrap itself are the initial trust root; maintain them
through reviewed repository changes, including rotation/revocation after a tool
or workflow compromise. TUF supplies Sigstore public trust material. No JARVIS
signing private key is generated/stored. Verification requires public network
access to GitHub and potentially Sigstore trust metadata, but no user account.
Signing publishes the public workflow identity/digest to Sigstore transparency;
no profile, keys or user data enter the manifest. Use 120-second download/verification
deadlines, bounded manifests/bundles/verifier size, and fail closed on outages.

## Discriminating acceptance

- Real pinned verifier accepts a real upstream bundle and refuses changed bytes,
  wrong issuer and wrong identity; never infer crypto correctness from a stub.
- Script fixtures prove verification precedes archive extraction/program execution,
  old-release manifest substitution fails, and a verifier failure cannot fall back.
- Mutating the signature-verification guard must make the refusal test fail.
- Hosted release must produce a real repository-identity bundle; retain that live
  evidence separately from offline fixture success. Verify the published manifest
  with the actual Windows/Linux/macOS verifier paths before claiming acceptance.
- Native certificate credentials are still an owner-provided prerequisite for
  notarization/Authenticode. Update recovery and clean-machine onboarding retain
  their own acceptance; signing alone does not implement them.

## Recorded evidence (2026-10-17)

- 66 local manifest/release-flow tests pass, including sign/verify failure before
  publication, feature-branch refusal and uploaded signature digest mismatch.
- Native Windows PowerShell 5.1 and Git Bash archive fixtures pass signed and
  explicit-digest modes, foreign/stale manifest refusal, verifier pin refusal,
  signature failure before archive download, archive tamper and cleanup.
- `node scripts/test-release-signing.mjs` downloads the real pinned verifier and
  upstream `cosign_checksums.txt` / `cosign_checksums.txt.sigstore.json`. The real
  bundle verifies for `keyless@projectsigstore.iam.gserviceaccount.com`, issuer
  `https://accounts.google.com`; changed bytes, wrong issuer and wrong identity
  fail. The production Windows installer rejects that valid foreign publisher
  with its own exact GitHub workflow identity. Public bundle/certificates stay in
  temporary/ignored artifacts, not the repository.
- Native CI runs this network acceptance on Linux/macOS/Windows. Repository OIDC
  signing and installation from its published bundle still require a hosted run;
  fixture signers do not establish that acceptance.
- Owner confirmed no native signing credentials. Authenticode and Apple
  notarization remain open; no credentials or self-signed substitute are created.

Signature-guard falsification removes only the verifier call from disposable
installer copies. Both refusal suites fail at `expected refusal: fixture signature
refused`; the production files remain intact. Full local Windows validation:
3,228 workspace tests, six existing opt-in ignores, formatting, strict Clippy,
dependency policy, six documentation tests and link/workflow lint pass.

The release workflow also runs `test-published-install.mjs` after publication on
four native platforms. It installs the real signed public archive into a fresh
home, hides development tools from PATH, runs both versions and unconfigured
profile diagnosis, and proves a repeated first install preserves the executable.
Intel macOS remains cross-built. This harness does not claim clean-host or
live-provider onboarding. Windows subprocesses explicitly hide windows and retain
PATHEXT/ComSpec in the isolated OS environment. Its first local prototype lacked
PATHEXT and failed before curl; no passing install is claimed from that attempt.
The corrected harness is pending hosted acceptance; do not run interactive native
installer probes on the owner's desktop while they use another application.

## Hosted follow-up (2026-10-18)

[CI 39073460721](https://github.com/props-nothing/jarvis-core/actions/runs/39073460721)
passed for `70cfc6e`, including real upstream verification on all three native OSes.
[Release 39074537468](https://github.com/props-nothing/jarvis-core/actions/runs/39074537468)
built all five packages and successfully signed, verified and published the twelve
assets with the repository workflow identity. The public release is
[ci-70cfc6eecb4eb2c69e227fc4599e73234b4a630d](https://github.com/props-nothing/jarvis-core/releases/tag/ci-70cfc6eecb4eb2c69e227fc4599e73234b4a630d).

Post-publication acceptance failed: both Linux architectures and native macOS
installed the signed archive and passed both versions, then the harness supplied
a nonexistent `--root` to doctor and attempted to parse empty output. Windows
timed out in the initial PowerShell application-discovery probe, before install.
These failures do not establish complete installation acceptance. The harness now
creates an empty portable root, reports empty diagnostic output clearly, and
checks Windows PATH/PATHEXT filesystem entries without PowerShell module discovery.
Native rerun acceptance remains open; no local installer launch is used to disturb
the owner's foreground application.

CI now also runs the current bootstrap against that exact pinned public release
on all four installation targets. This compatibility fixture catches harness and
installer regressions without rebuilding a release first; it does not substitute
for Release's post-publication check of the newly built executable. Both paths
are read-only toward GitHub and use the same production signature verification.
