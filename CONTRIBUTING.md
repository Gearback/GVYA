# Contributing to GVYA

Contributions are welcome. GVYA is intentionally strict about semantic authority, source contracts, and validation because changes can affect compiler/runtime determinism and authored conversation behavior far beyond the edited file.

## Before you change code

Read:

1. `README.md` for project orientation;
2. `docs/CONSTITUTION.md` for architectural invariants and permanent non-goals;
3. `docs/README.md` to find the authoritative document for the concern you are changing;
4. `AGENTS.md` for the active development and validation workflow.

For a large architectural change, start with a GitHub issue or discussion before implementing it. Small bug fixes and clearly bounded improvements can go directly to a pull request.

## Development setup

The minimum Node version is declared in `package.json`; the Rust toolchain is declared in `rust-toolchain.toml`.

```bash
npm ci
npm run validate:source
```

See `docs/GETTING_STARTED.md` for Studio and CLI setup.

## Change principles

- Keep one canonical semantic/compiler/runtime authority path.
- Do not add a JavaScript/GDScript semantic fallback.
- Do not add hidden model/provider state to GVYA or Studio.
- Keep host effects host-owned; GVYA proposes typed capabilities but does not silently execute them.
- Prefer a clean contract change over a compatibility shim while GVYA remains pre-1.0, unless the project explicitly changes that policy.
- Reuse the existing owning model, helper, registry, and validator rather than adding parallel paths.
- Keep changes bounded. Do not combine unrelated cleanup with a focused fix.
- Update the authoritative documentation when a contract changes.
- Add permanent tests for durable behavior, safety, protocol, or architectural invariants; do not create tests merely to pin cosmetic detail.

## AI-assisted contributions

AI-assisted contributions are welcome. GVYA itself was developed with extensive AI assistance.

The contributor remains responsible for the submitted result. Review generated changes, understand the affected boundary, run the relevant tests, and do not submit code or text that you do not have the right to license under Apache-2.0.

A large generated diff without a clear purpose, owning contract, or validation evidence is not easier to review because it was generated quickly. Prefer small, explainable changes.

See `docs/AI_FIRST_DEVELOPMENT.md` for the project's AI-first engineering philosophy.

## Validation

During normal development, follow `AGENTS.md`: identify the affected boundary and run the smallest check that can falsify the change.

Common repository-level checks are:

```bash
npm run validate:source
npm run test:source
```

Do not run release certification as a ritual after every edit. Release/freeze commands are defined only in `docs/RELEASE.md`.

## Pull requests

A useful pull request should explain:

- the problem being solved;
- the chosen boundary and why it owns the fix;
- user-visible or contract-visible behavior changes;
- tests or validation that were run;
- any intentionally deferred work.

Keep generated build output, dependency directories, local caches, validation logs, ZIP archives, recovery backups, and unrelated historical reports out of the commit.

## Licensing of contributions

GVYA is licensed under the Apache License 2.0. By submitting a contribution to this repository, you agree that your contribution may be distributed under the repository's Apache-2.0 license and that you have the right to make that contribution.
