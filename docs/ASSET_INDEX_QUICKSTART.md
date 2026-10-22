# Asset index: quickstart (agents read this first)

A local SQLite index lists every resource in all 63 Forge archives: ID, name, type, archive,
entry, offsets, hashes, and candidate links between resources. **Query it before you scan or
parse archives.** It contains metadata only, no game bytes.

## Do this at the start of a session
```
python tools/index_status.py            # ready? if not, it says what to run
python tools/index_status.py --fetch    # download (progress bar), verify, unpack
```
If it says READY, use the commands below. If it cannot download, say so; do not fall back to
scanning archives for something the index answers.

## Ask it things
```
python tools/asset_query.py info                         counts, version
python tools/asset_query.py where 0000000f18b7e1cf       which archive/entry holds this ID
python tools/asset_query.py find Wooden_Sword            resources by name (substring)
python tools/asset_query.py find Arno --type Material    filter by type ID or label
python tools/asset_query.py types                        resource types, labels, confidence
python tools/asset_query.py archive DataPC_ACU_Paris.forge
python tools/asset_query.py walk <name or ID> --depth 2  candidate links down (add --up for parents)
python tools/asset_query.py sql "SELECT ..."             any read-only SELECT
```
Options (`--limit N`, `--json`, `--db PATH`) work before or after the command. Output is capped
at 25 rows by default; raise `--limit` if it says more exist.

## Tables (for `sql`)
`archive` `entry` (one per archive entry = a package) `resource` (envelopes inside a package;
`file_id`, `name`, `type_id`) `texture` `link` (candidate references) `type_label` `loose_file`
(audio/video/other non-Forge files) `meta`. `link_view` and `fan_in` are views over `link`.

## Read the evidence levels (do not overstate)
- **A link is a candidate**: eight bytes in a resource equal another resource's ID. It is not a
  decoded reference. `kind='weak'` links point at IDs below 2^25 and are noise; `walk` skips them.
- **A type label has a confidence**: `observed-decode` (the Rust parser decoded it as that
  family), `crc-name` (the class name's CRC-32 equals the type ID), `unknown`.
  Categories are a browsing aid, not game data.
- Names are the resource's own name field; about 121k resources have none.
- The same ID can exist in several archives (patches, other levels). `where` shows every copy;
  which one the game loads is **not established** (issue #35).
- One archive, `DataPC_ACU_Paris.forge`, was rescanned because it changed after the first scan.
  Older notes quoting "24,681 declared" or "4 bad Paris entries" describe the old file.

## Stale or broken?
`python tools/audit_asset_db.py local/asset_index.sqlite` checks the index for holes.
Full guide: `docs/ASSET_INDEX.md`.
