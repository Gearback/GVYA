# GVYA Release Policy

This document is the single source of truth for release and freeze certification. It records required policy and commands, not audit history, delivery status, or iteration reports. Open release work belongs only in `/TODO.md`.

## Toolchain

- Rust version, required components and release WASM target are defined only by `rust-toolchain.toml`.
- The minimum Node version is defined only by the root `package.json` `engines.node` contract.
- `tools/release_gate.py` reads those files directly; it does not maintain a second toolchain-version inventory.
- Release certification requires reviewed frozen `Cargo.lock` and `package-lock.json`.
- Certification is fail-closed; a missing tool, target, lockfile, browser, security auditor or executable gate is a failure, never an inferred pass.

## Development iteration is not certification

These release commands are intentionally broad and expensive. They are not the normal edit loop. During active development, follow [root AGENTS](../AGENTS.md): inspect impact, run only the smallest relevant type-check/test, and reserve broad suites, Engine rebuild/acceptance, manifest refresh, packaging and fresh-extraction proofs for the boundary that actually requires them or for one final baseline/milestone pass.

## Lock bootstrap

On the first connected compliant checkout only:

```text
npm run bootstrap:locks
```

The command may generate missing lockfiles but does not certify them. Review the lockfile diffs, keep the approved lockfiles in source, then refresh source manifests. Existing lockfiles must not be overwritten by bootstrap.

## Source validation

```text
npm run validate:source
```

Source validation checks package membership/integrity, clean-source hygiene, the canonical documentation layout, TODO locality, archive/reference exclusion, and release-script registration. It is not a substitute for executable tests.

## Fast executable source tests

```text
npm run test:source
```

This compiles the TypeScript projects and runs the SDK, Studio/human-authoring, and external-agent authoring E2E source/runtime contract suites. The broader GVYA Help behavioral suite is intentionally reserved for certification.

## Core certification

```text
npm run certify:preflight
npm run certify:core
```

Core certification must execute, under the pinned/frozen environment:

- `cargo fmt --check`;
- `cargo check --workspace --locked`;
- `cargo test --workspace --locked`;
- `cargo clippy --workspace --all-targets --locked --message-format=short -- --cap-lints warn` as an advisory `all`/`pedantic` diagnostic pass after `cargo check` has already enforced the workspace `deny(warnings)` rustc contract;
- wasm32 ABI build and native/WASM fixture + TypeScript SDK + Godot Web/WASM parity validation;
- canonical native `gvya` CLI build plus process-level external-agent `author-step` E2E proof;
- `npm ci`;
- TypeScript/SDK/Studio tests plus the external-agent authoring E2E source/runtime proof;
- the full GVYA Help AI behavioral contract suite;
- Studio production build;
- bundled Engine asset integrity verification and native/WASM acceptance.

## Release certification

```text
npm run certify:release
```

Release certification adds:

- rendered Chromium/Chrome acceptance against the production Studio bundle;
- Cargo dependency security audit;
- npm dependency security audit at the configured severity threshold.

## Final ZIP and fresh-extraction certification

After the checkout passes release certification, remove generated output, refresh the manifests if source changed, validate the clean source boundary, and build the final candidate outside the source tree:

```text
npm run clean
npm run refresh:manifests
npm run validate:source
npm run package:release -- D:\GVYA\gvya-source.zip
npm run certify:archive -- D:\GVYA\gvya-source.zip
```

`package:release` refuses to overwrite an existing ZIP and packages exactly the validated manifest members beneath one `gvya/` directory. `certify:archive` rejects unsafe or ambiguous archive paths, extracts to a short temporary root, and runs the complete fail-closed release gate from that extraction. On Windows, this avoids `MAX_PATH` failures caused by putting the ZIP under a long manual parent path. If the system temporary path is itself too long, set `GVYA_RELEASE_TEMP` to a deliberately short writable directory such as `C:\g`.

For manual Windows extraction, also choose a short destination such as `C:\g`; the deepest current source paths are not reliable when nested beneath a long parent directory.

## GitHub Actions

The repository keeps ordinary CI and release certification separate:

- `.github/workflows/ci.yml` runs clean-source validation, the bounded Node/Studio source suite, Engine asset verification, and Rust format/check/test gates on pushes and pull requests.
- `.github/workflows/release-certification.yml` is manual. It provisions Node 24, exact Rust 1.85.0 with the declared WASM target/components, a rendered Chromium/Chrome executable, and pinned `cargo-audit`; runs `certify:preflight` and `certify:release`; creates the source ZIP through `package:release`; then runs `certify:archive` against that exact ZIP from a fresh extraction and uploads the ZIP plus its SHA-256.

The workflow is an automation of this document, not a second release policy. Its result is meaningful only for the exact commit and uploaded archive that passed. Ordinary CI, a locally built ZIP, or a previously certified commit cannot substitute for the complete release workflow.

## Freeze claim

A GVYA source package may be called freeze-certified only when `npm run certify:release` passes and `npm run certify:archive -- <exact-final.zip>` passes for the same complete gate. Structural validation, static review, partial tests, previous runs, or unavailable tools cannot substitute for that proof.
