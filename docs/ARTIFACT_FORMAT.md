# `.gvya` Container v1

`.gvya` has one meaning: a compiled, portable GVYA runtime artifact.

It is not a source project, ZIP bundle, demo package, SDK package, or migration package.

## Why a small custom container

The runtime only needs named immutable entries plus integrity metadata. ZIP would add timestamps, compression choices, archive extraction behavior, duplicate-entry semantics, and historical pressure to tolerate multiple layouts. GVYA instead uses a bounded deterministic table with raw payloads.

## Header

All integers are unsigned little-endian.

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | magic: `47 56 59 41 0d 0a 1a 0a` (`GVYA\\r\\n\\x1a\\n`) |
| 8 | 2 | container version = `1` |
| 10 | 2 | reserved flags = `0` |
| 12 | 4 | entry count |
| 16 | 8 | entry-table byte length |

No timestamp or machine metadata exists.

## Entry table

Entries are strictly sorted by UTF-8 logical path. Duplicate paths are invalid.

Each row contains:

| Size | Field |
|---:|---|
| 1 | entry kind |
| 1 | essential flag (`0` or `1`) |
| 2 | path byte length |
| 8 | absolute payload offset |
| 8 | payload byte length |
| 32 | SHA-256 of exact payload bytes |
| N | UTF-8 path |

Entry kinds:

1. Manifest
2. Program
3. Asset
4. Debug map
5. Signature envelope
6. Integrity manifest

After the table, raw payloads appear **contiguously** in the same sorted order. The first payload starts exactly at the end of the table, every next payload starts exactly after the previous one, and the final payload ends exactly at EOF. Gaps, overlaps, reordered payloads, and trailing bytes are invalid. There is no compression in v1. This is intentional: identical inputs produce identical bytes across implementations without compressor-version dependence. Compression can later be an explicitly versioned codec if measurements justify it.

## Required entries

Every artifact has these essential entries:

- `manifest.json`
- `program.json`
- `integrity.json`

Assets are also essential. `debug/source-map.json` and `signature.json` are non-essential adjuncts.

`debug/source-map.json` is emitted only when the Bot's `emit_debug_map` setting is on. It is purely additive: the executable `program.json` is byte-identical between a debug and a release build of the same source, so debug data can never define runtime semantics. Runtime load checks only that its presence and the manifest `debug_map` declaration agree; it never reads its content.

Kind/path/essential combinations are canonical, not advisory: Manifest must be essential `manifest.json`; Program essential `program.json`; Integrity essential `integrity.json`; Asset entries must be essential and live below `assets/`; DebugMap must be non-essential `debug/source-map.json`; Signature must be non-essential `signature.json`. A loader rejects any other combination.

## Integrity layers

1. Every table row has an exact SHA-256 payload digest.
2. `integrity.json` records the compiled program digest, asset digests/sizes, and canonical source-package digests.
3. `manifest.json` records the digest of `integrity.json` and `program.json`.
4. The artifact itself may be SHA-256 hashed for distribution/cache identity.
5. Optional signing uses a **content root** derived from the sorted set of essential `(kind, path, digest)` tuples. Signature entry bytes are excluded, so adding a signature does not change what is signed.

Hashes provide integrity, not authenticity. The optional signature envelope is separate and requires a host/application trust policy in runtime/SDK layer.

## Path rules

All paths are relative forward-slash paths. Reject:

- leading/trailing slash
- backslash
- empty segment
- `.` or `..` segment
- NUL/control characters
- path beyond configured byte bound
- duplicate path

A runtime reads payloads by offsets; it does not extract paths to the filesystem.

## Bounds

Default v1 compiler/loader contract:

- max 16,384 entries
- max 512 bytes per path
- max 64 MiB per entry
- max 512 MiB total artifact
- max 8 MiB `manifest.json`
- max 8 MiB `integrity.json`
- max 256 KiB `signature.json`
- max 16 MiB `debug/source-map.json`

Caller-supplied artifact limits may only **tighten** these canonical ceilings; they cannot relax them. Runtime metadata is also preflighted for bounded JSON nesting, structural-token count and string-token bytes before typed deserialization. These are hard failure bounds, not permission to silently truncate.
