# GVYA Documentation

Each current concern has exactly one authoritative document. Other README files may provide local entry points, but must link here instead of redefining architecture or release policy. Audit history, change reports, handoffs, ADR history and iteration notes do not belong in the source package.

| Concern | Source of truth |
|---|---|
| Public orientation and first-run workflow | `GETTING_STARTED.md` |
| AI-first development philosophy, responsibility and transparency | `AI_FIRST_DEVELOPMENT.md` |
| Architectural invariants and permanent non-goals | `CONSTITUTION.md` |
| Canonical domain vocabulary and ownership | `DOMAIN_MODEL.md` |
| Repository layers and dependency direction | `REPOSITORY_LAYOUT.md` |
| Authoring source files and source parsing | `SOURCE_FORMAT.md` |
| Executable JSON Schema subset | `SCHEMA_PROFILE.md` |
| Semantic matching, evidence, profiles and optional external semantic resolver | `SEMANTIC_ARCHITECTURE.md` |
| Conversation lifecycle, responses, state and templates | `CONVERSATION_ARCHITECTURE.md` |
| Capability contracts, binding, policy, confirmation and result lifecycle | `CAPABILITY_ARCHITECTURE.md` |
| Package composition, audit, Why and authored tests | `PACKAGE_ARCHITECTURE.md` |
| Conversation-design-first vertical-slice authoring, precision boundaries, repair tuning, recovery quality, mechanic proof and blind/quality evaluation recipe | `PACKAGE_AUTHORING_RECIPE.md` |
| Compiler authority, canonical IR, determinism and signing boundary | `COMPILER_PIPELINE.md` |
| `.gvya` container bytes, paths, integrity and bounds | `ARTIFACT_FORMAT.md` |
| Runtime loading/execution and SDK/adapter boundary | `RUNTIME_ARCHITECTURE.md` |
| Runtime JSON/ABI wire shapes and budgets | `RUNTIME_WIRE_PROTOCOL.md` |
| Human Studio source ownership and UX authority | `STUDIO_ARCHITECTURE.md` |
| Single bundled Engine WASM ownership, integrity and reuse | `ENGINE_ASSETS.md` |
| External agent authoring, canonical CLI command surface, deterministic repair/promote loop and provider-neutral machine interface | `MACHINE_AUTHORING_ARCHITECTURE.md` |
| Release/freeze certification policy | `RELEASE.md` |
| Active-development validation cadence and agent working rules | `../AGENTS.md` |

Open work is tracked only in `/TODO.md`.

Durable end-to-end external-agent fixture execution is under `validation/authoring-e2e/`; it validates the documented machine-authoring loop without defining another source or policy contract.
