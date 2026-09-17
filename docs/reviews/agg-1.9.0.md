# agg 1.9.0 development-tool review

Assessment date: 2026-09-17. Scope: render the README demo locally from a trusted recording. The renderer is not part of the shipped binary or release build.

Independent review checked the [upstream release](https://github.com/asciinema/agg/releases/tag/v1.9.0), downloaded asset, tagged source, locked dependency closure, and advisory output. The Linux musl asset SHA-256 is `ddcbf6ca044c8ac3a434dcb9ee89fb9e3be87209982b7c2adb55f782e8f0f390`, matching GitHub's asset digest. The release was published 2026-05-29T18:13:31Z, beyond the 48-hour quarantine; its source tag points to `26ca84c02523973198fca28533369edcfc7ed929`. The asset is a static PIE executable. The tag is unsigned, no GitHub attestation was available, and the hash does not establish correspondence between binary and source.

The review reported 228 reachable crates on the musl target, including 26 dependency build scripts and eight derive procedural macros; no build script exists in agg itself. Source inspection found URL input networking through reqwest and local output-file writes. The binary was not executed during that dependency review.

The reviewer reported five advisory findings: RUSTSEC-2026-0285 (rustls TLS), RUSTSEC-2026-0190 (anyhow unsoundness), RUSTSEC-2026-0204 (crossbeam unsoundness), and unmaintained notices RUSTSEC-2026-0192 (ttf-parser) and RUSTSEC-2026-0206 (rustybuzz). Adoption is limited to trusted local cast input and installed fonts, a cleared environment, disabled networking, read-only inputs, and a scratch output mount. This scoped acceptance is not a clean advisory result or approval for remote input. Reassess before changing the renderer or its scope.

The renderer and its dependencies include GPL-3.0-or-later and AGPL-3.0-or-later components. No renderer executable is distributed with ask. The [demo guide](../development/demo.md) records the pinned font and reproduction commands; `scripts/demo.py` verifies the approved executable and font hashes before rendering.
