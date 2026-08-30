# GVYA Studio

GVYA Studio is the optional visual editor and simulator for human authors. Its persisted model separates Shared Packages, Project-owned Packages, and Bots. Shared and Project scope may each own Standard or Fallback Packages. Every Bot owns exactly one structural Standard Bot Package. It is created with the Bot, cannot be detached or shared with another Bot, and is deleted only with that Bot. A Bot may add Shared/Project Standard Packages, and may select at most one Shared/Project Fallback Package; Fallback Packages are never overridden. Packages have stable IDs and explicit dated folder ZIP downloads rather than author-facing versions or automatic recovery history. Each selected Bot resolves to one ordinary GVYA compile target and compiles to one `.gvya`.

Machine authors do not automate Studio or configure a provider inside it. They read and write the same canonical GVYA source and consume the canonical CLI/compiler contracts directly. Studio remains the human editor and does not mirror the machine-facing `gvya.cli.author-step/1` or `gvya.cli.check-change/1` reports. Model choice, credentials, source edits, planning, retries, and promotion remain external host/human responsibilities.

Studio architecture and persistence: `../../docs/STUDIO_ARCHITECTURE.md`.

Machine authoring boundary: `../../docs/MACHINE_AUTHORING_ARCHITECTURE.md`.

Release policy: `../../docs/RELEASE.md`.
