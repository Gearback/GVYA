# Getting Started with GVYA

This guide is the shortest path from a fresh source checkout to running Studio, using the canonical CLI, creating a bot source tree, and building a `.gvya` artifact.

For architecture, use `README.md`. For release certification, use `RELEASE.md`. This guide does not redefine either contract.

## 1. Prerequisites

GVYA intentionally pins its development boundaries.

- **Node.js:** version 24 or newer, as declared by the root `package.json`.
- **Rust:** the toolchain declared in `rust-toolchain.toml` (currently Rust 1.85.0, including the required formatting/lint components and WASM target).
- **Git:** for normal source control workflow.

A compliant Rustup installation will read `rust-toolchain.toml` when commands are run from the repository.

## 2. Install JavaScript dependencies

Use the committed lockfile:

```bash
npm ci
```

The repository uses fail-closed engine requirements. If the Node version is too old, installation should fail rather than silently continuing with an unsupported toolchain.

## 3. Validate the clean source boundary

```bash
npm run validate:source
```

This validates repository/source integrity, canonical documentation layout, generated-file hygiene, command registration, and source manifests. It is structural validation, not the full executable test suite.

For the bounded executable source suite:

```bash
npm run test:source
```

Do not substitute these commands for release certification. The complete release gates are defined in `RELEASE.md`.

## 4. Run GVYA Studio

```bash
npm run dev:studio
```

Studio is the visual human-authoring surface. It edits canonical GVYA source and uses the bundled canonical Engine for simulation; it is not a second semantic implementation.

## 5. Build the canonical CLI

```bash
cargo build -p gvya-cli --locked
```

During development, the CLI can also be invoked without referring to a platform-specific binary path:

```bash
cargo run -p gvya-cli -- --help
```

## 6. Create a bot source tree

The canonical scaffold command is:

```bash
cargo run -p gvya-cli -- init bot ./support-bot \
  --project-id support \
  --bot-id assistant \
  --languages en-US,fa-IR \
  --enabled-languages en-US \
  --default-language en-US
```

The generated directory is ordinary GVYA source. Studio and external agents operate on the same source model.

Validate it:

```bash
cargo run -p gvya-cli -- check ./support-bot
```

Inspect it:

```bash
cargo run -p gvya-cli -- inspect ./support-bot --json
cargo run -p gvya-cli -- analysis ./support-bot --json
```

The complete CLI command surface and machine-readable contracts are defined in `MACHINE_AUTHORING_ARCHITECTURE.md`.

## 7. Author a change safely

GVYA's machine-authoring workflow treats the last accepted source snapshot as immutable and evaluates a candidate snapshot separately.

```bash
cargo run -p gvya-cli -- author-step ./accepted-bot ./candidate-bot --json
```

The returned state/actions are deterministic. The human or external agent remains responsible for making source edits; `author-step` does not host an LLM, mutate the accepted baseline, or invent a second source format.

For the complete vertical-slice workflow, mechanic-proof requirements, and promotion boundary, read:

- `MACHINE_AUTHORING_ARCHITECTURE.md`
- `PACKAGE_AUTHORING_RECIPE.md`

## 8. Build a portable artifact

```bash
cargo run -p gvya-cli -- build ./support-bot --output ./support.gvya
```

The output is a portable `.gvya` runtime artifact. Its byte/container contract is defined in `ARTIFACT_FORMAT.md`; do not hand-edit artifact bytes.

## 9. Integrate the runtime

GVYA keeps runtime effects host-owned. A host may use the available SDK/adapter surfaces to load the compiled artifact, submit user input, receive a deterministic response or capability proposal, execute an allowed capability externally, and return the result to GVYA.

Start with:

- `RUNTIME_ARCHITECTURE.md`
- `RUNTIME_WIRE_PROTOCOL.md`
- `CAPABILITY_ARCHITECTURE.md`
- `../packages/runtime-sdk/README.md`
- `../adapters/godot/README.md`

## 10. Before contributing

Read `../CONTRIBUTING.md` and `../AGENTS.md`. GVYA favors small, bounded changes, one semantic authority path, executable proof at the affected boundary, and broad certification only at milestone/release closure.
