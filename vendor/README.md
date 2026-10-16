# Temporary Office dependency patches

These are the official crates.io archives of `docx-rs` 0.4.22 (MIT) and
`rust_xlsxwriter` 0.99.1 (MIT OR Apache-2.0). The **only code/build change** in
each is `[dependencies.zip].version = "=9.0.4"` in its normalized `Cargo.toml`.
Original `Cargo.toml.orig` files and upstream source are retained for comparison.
The Word crate's omitted license file is supplied from its archive's recorded
upstream commit: https://raw.githubusercontent.com/bokuweb/docx-rs/f04cf8b47a925b2b6fff850dcb75069aae89dab4/LICENSE
(MIT, Copyright 2020 bokuweb). The Excel archive includes both license files.

The latest published writers still require ZIP 8, whose releases are yanked.
The root Cargo patch keeps the writer APIs fixed while using the supported ZIP
release. This does not relax `cargo deny`'s yanked/advisory policy. Remove both
patches when published writers accept ZIP 9 and the document regressions pass.

Archives verified on 2026-10-16:

| Archive | SHA-256 |
| --- | --- |
| https://static.crates.io/crates/docx-rs/docx-rs-0.4.22.crate | `7fdf00e8af6d0b3e92d4bbf9b76f773d8b84ea80f310324ad16cbdc2e653e02c` |
| https://static.crates.io/crates/rust_xlsxwriter/rust_xlsxwriter-0.99.1.crate | `34f2d2cda7223168a14fb86bc7411dbc3379bd7d49749ecc53b91ed98585b6b0` |

These packages are excluded from workspace membership. The JARVIS document
writer/reader and hostile-archive tests exercise their consumed APIs. See
[the research record](../docs/research/integrations/github-actions-rust-ci.md).
