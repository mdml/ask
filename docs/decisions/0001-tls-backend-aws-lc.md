# Accept the AWS-LC native build surface

Date: 2026-09-04

Status: accepted

## Context

Reqwest 0.13 with Rig's `rustls` feature uses the AWS-LC-backed Rustls provider. `aws-lc-sys` 0.45.0 brings a bundled native C and assembly build of about 69M unpacked and uses a CMake/`cc` build program. The dependency closure and build surface are documented in the [Rig 0.42.0 dependency-closure review](../reviews/rig-dependency-closure.md).

## Decision

On 2026-09-04, the owner accepted the AWS-LC native build surface for `ask`'s supported macOS and Linux arm64 and x86-64 targets.

## Consequences

Clean builds require the native C and assembly toolchain for their target and require CMake. The AWS-LC build program runs in every clean build. On 2026-09-04, a clean `cargo build --locked` of the complete dependency graph took 30.04 seconds on the Linux x86-64 review host. Revisit this decision if release reproducibility or startup measurements require it.
