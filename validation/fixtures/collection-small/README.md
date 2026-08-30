# Small collection fixture

This intentionally small Bot is the normal development loop for partial Meaning and
multi-turn value collection. It covers number and built-in entity collection, a
deterministic custom entity, cancellation, and capability handoff without loading the
full `gvya-bot` corpus.

Run the focused checks from the repository root:

```powershell
cargo run -q -p gvya-cli -- check validation/fixtures/collection-small
cargo run -q -p gvya-cli -- test validation/fixtures/collection-small --json
```

Use the full authored Bot only for the final milestone/regression pass or when a change
genuinely crosses its content boundary.
