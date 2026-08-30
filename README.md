# GVYA

**Deterministic conversation compiler for AI-authored, inspectable bots.**

GVYA turns explicit conversation source into a portable `.gvya` brain that can match user language, manage conversation state, recover from imperfect input, and propose typed host capabilities without making a generative model the runtime authority.

**🤖 [Talk to GVYA — live demo](https://gearback.github.io/gvya/)**

**AI-first, not AI-dependent.** Humans and AI agents can author the same canonical source. The compiler and runtime remain deterministic, bounded, inspectable, and host-controlled. No LLM is required to run a compiled GVYA bot.

> **Status:** public-preview source. GVYA is pre-1.0. A freeze/release claim is made only after the complete certification process in `docs/RELEASE.md` passes on the exact release archive.

## Why GVYA exists

There is a useful space between hand-authored pattern systems and fully generative bot runtimes.

AIML-style systems make authored behavior explicit and predictable. LLM-driven systems can generalize broadly, but they also move more runtime authority into a probabilistic model. GVYA explores a different tradeoff: keep knowledge and behavior authored, add stronger semantic and conversation machinery, and compile the result into a deterministic runtime artifact.

GVYA therefore combines:

- explicit, reviewable conversation source;
- semantic evidence plus deterministic structural patterns;
- multilingual Language and Matcher Profiles;
- follow-up, repair, repeat, fallback, state, and scenario mechanics;
- typed capability proposals with host-owned execution;
- deterministic compiler/runtime semantics in Rust;
- a portable `.gvya` artifact;
- human authoring in GVYA Studio;
- machine authoring through the same canonical source and CLI contracts.

This is not a claim that GVYA is universally better than AIML or an LLM runtime. They solve different problems. Quantitative comparisons should be reproducible and should publish the authored inputs, held-out cases, methodology, raw results, and failure cases rather than relying on headline claims.

## Reproducible benchmark

GVYA publishes the inputs and failures behind its comparison claims. **Test 1 — Equal Authored Evidence** compares GVYA with ChatScript 14.1 and AIML 2.0 / Program-Y 3.6 using exactly 96 authored semantic examples per engine and a frozen 288-turn evaluation corpus.

| System | In-domain accuracy | OOD false-positive |
|---|---:|---:|
| **GVYA** | **49.17%** | **0.00%** |
| ChatScript 14.1 | 42.08% | **0.00%** |
| AIML 2.0 / Program-Y 3.6 | 40.00% | **0.00%** |

This is a narrow engine-behavior result, not a claim of universal chatbot superiority. The benchmark also publishes GVYA's three wrong-intent cases and the negative result that none of the three systems recovered the unseen-paraphrase track under this intentionally sparse evidence budget.

See [`benchmarks/test1-equal-authored-evidence/`](benchmarks/test1-equal-authored-evidence/) for Test 1. **Test 2 — Equal Authoring Budget** gives each engine exactly 115 user-language evidence rows on a new fictional domain. GVYA reaches **35.83% positive held-out coverage**, ChatScript **21.67%**, and AIML **13.33%**; ChatScript is the most conservative on near-domain confounders (**86.7% rejection** vs **60.0%** for GVYA and AIML). The full sources, budget ledgers, frozen corpus, raw predictions, failures, and runtime/result locks are published under [`benchmarks/test2-equal-authoring-budget/`](benchmarks/test2-equal-authoring-budget/). These are bounded benchmark results, not universal chatbot-quality claims.

## AI-first development

GVYA was also developed with an AI-first engineering workflow.

Software development is moving toward a future in which AI will perform a growing share of implementation, analysis, testing, maintenance, and iteration. GVYA deliberately embraces that direction rather than hiding it. AI agents were used extensively during development, while human direction remained responsible for product intent, architectural choices, constraints, evaluation, and acceptance decisions.

The important boundary is authority: generated code is not accepted because an AI produced it, and human-written code would not be accepted merely because a human produced it. Contracts, deterministic behavior, executable tests, review, and release gates are the evidence.

That philosophy also appears inside the product. An external AI agent can inspect and author ordinary GVYA source, but it does not become a hidden semantic authority and GVYA does not embed a model provider, model credentials, or an autonomous agent runtime.

Read [`docs/AI_FIRST_DEVELOPMENT.md`](docs/AI_FIRST_DEVELOPMENT.md) for the development philosophy, division of responsibility, and the concrete mechanisms used to keep AI-assisted work inspectable.

## How it works

```text
Human author                 AI author
     |                           |
     +-------- canonical source -+
                    |
              GVYA Compiler
                    |
                 .gvya
                    |
              GVYA Runtime
               /          \
          response      capability proposal
                              |
                         host application
                              |
                       real-world effect
```

The canonical authority path is:

```text
GVYA source -> Rust compiler -> .gvya -> Rust runtime
```

Host applications remain the only executors of declared capabilities. GVYA may propose an allowed capability action; it does not silently perform the effect itself.

## Product surfaces

- **GVYA Studio** — visual human authoring and simulation.
- **GVYA Compiler** — canonical source-to-artifact compiler.
- **GVYA Runtime** — deterministic portable runtime.
- **GVYA SDK** — host integration for Browser/Node, C/native hosts, and Godot Web/WASM.
- **`.gvya`** — compiled portable bot artifact.
- **GVYA CLI** — canonical inspection, validation, machine-authoring, testing, and build surface.

## Quick start

Requirements:

- Node.js **24 or newer**;
- Rust **1.85.0** with the toolchain declared by `rust-toolchain.toml`;
- Git.

From a fresh checkout:

```bash
npm ci
npm run validate:source
npm run dev:studio
```

To build and use the canonical CLI:

```bash
cargo build -p gvya-cli --locked
cargo run -p gvya-cli -- init bot ./support-bot \
  --project-id support \
  --bot-id assistant \
  --languages en-US,fa-IR \
  --enabled-languages en-US \
  --default-language en-US
cargo run -p gvya-cli -- check ./support-bot
cargo run -p gvya-cli -- build ./support-bot --output ./support.gvya
```

See [`docs/GETTING_STARTED.md`](docs/GETTING_STARTED.md) for a guided source, Studio, CLI, and validation walkthrough.

## Validation and release discipline

GVYA separates the normal development loop from release certification.

```bash
npm run validate:source   # source/package integrity and repository policy
npm run test:source       # bounded executable source tests
```

Broad release gates are intentionally not normal edit-loop commands. The complete fail-closed release process, including Rust workspace checks, native/WASM parity, Studio production build, browser acceptance, dependency security audit, Engine verification, behavioral tests, packaging, and fresh-archive certification, is defined in [`docs/RELEASE.md`](docs/RELEASE.md).

GitHub uses two separate automation boundaries: [`.github/workflows/ci.yml`](.github/workflows/ci.yml) runs ordinary source/Rust regression checks for pushes and pull requests, while [`.github/workflows/release-certification.yml`](.github/workflows/release-certification.yml) is a manual full certification job that builds and re-certifies the exact source ZIP from a fresh extraction. A green ordinary CI run is not a release/freeze claim.

The active agent/development working rules live in [`AGENTS.md`](AGENTS.md).

## Architecture documentation

[`docs/README.md`](docs/README.md) is the documentation map and identifies the authoritative document for each architectural concern. Important starting points include:

- [`docs/CONSTITUTION.md`](docs/CONSTITUTION.md) — architectural invariants and permanent non-goals;
- [`docs/DOMAIN_MODEL.md`](docs/DOMAIN_MODEL.md) — canonical vocabulary and ownership;
- [`docs/SEMANTIC_ARCHITECTURE.md`](docs/SEMANTIC_ARCHITECTURE.md) — matching and semantic evidence;
- [`docs/CONVERSATION_ARCHITECTURE.md`](docs/CONVERSATION_ARCHITECTURE.md) — conversation mechanics;
- [`docs/CAPABILITY_ARCHITECTURE.md`](docs/CAPABILITY_ARCHITECTURE.md) — typed capability boundary;
- [`docs/MACHINE_AUTHORING_ARCHITECTURE.md`](docs/MACHINE_AUTHORING_ARCHITECTURE.md) — deterministic external-agent authoring loop;
- [`docs/ARTIFACT_FORMAT.md`](docs/ARTIFACT_FORMAT.md) — `.gvya` artifact contract.

## Permanent boundaries

The full authority is `docs/CONSTITUTION.md`. In short, GVYA intentionally has:

- no compatibility layer or importer for predecessor systems;
- no independent JavaScript or GDScript semantic runtime;
- no direct host capability execution inside GVYA;
- no hidden authoring-client authority over deterministic semantic policy;
- no built-in LLM/provider registry, credentials, token accounting, or model-hosting autonomous agent runtime.

## Contributing and security

Contributions are welcome. Read [`CONTRIBUTING.md`](CONTRIBUTING.md) before changing architecture or behavior. Security issues should follow [`SECURITY.md`](SECURITY.md) and should not be disclosed through a public exploit report.

## License

GVYA is licensed under the **Apache License 2.0**. See [`LICENSE`](LICENSE).

## Name

**GVYA** is a stylized rendering of **gooya**, from Persian, chosen for its association with speaking and expressing meaning.

Project direction and system design: **Ali Pournasseh**. AI agents were used extensively for implementation, analysis, testing, documentation, and review; the development model is documented openly rather than presented as hand-written implementation.
