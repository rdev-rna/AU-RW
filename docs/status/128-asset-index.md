# #128 Local SQLite asset index (area: tooling)

Branch `acu-128-asset-index`. Files: `tools/build_asset_db.py`, `tools/test_build_asset_db.py`, this note.

## What it is
`python tools/build_asset_db.py <scan dir> --output local/asset_index.sqlite` turns an `acu-scan`
`entries.jsonl` into a SQLite file with three tables: `archive`, `entry` and `resource`. It reads
the scan only (no archive is opened except to stat its size) and uses the standard library.
The database is derived and local (`local/` is ignored); it holds IDs, offsets, lengths, names
and hashes, never game bytes. Only the builder and its tests are committed.

Why it is safe to cache: the install has not been updated in years. `archive.size_bytes` is
recorded as a cheap guard against a swapped file; it is not a content check.

## Built from `local/coverage-typed-full2-20261018` (2026-10-18 scan)
- 63 archives; 180,119 data entries + 126 control entries = 180,245 (the reachable total).
- 3,289,051 resources, 484 distinct types. Both match the STATUS.md figures.
- Largest types by count: `51dc6b80` 531,502 (the LODSelector family), `0984415e` 470,427
  (Entity), `85c817c3` 442,966 (Material), `d70e6670` 403,060 (TextureSet), `415d9568` 203,190 (Mesh).
- ~121k resources have no name in the scan; they are NULL. 481k distinct names exist.
- A lookup by file ID returns in about 1 ms. `00000007c2448f42` returns the same two copies and
  stored lengths as ACU-RE-0038 (67,020 base, 67,045 patch_02).

## Example queries
```sql
-- where does this ID live?
SELECT a.name, e.idx, e.offset, e.stored_length
FROM entry e JOIN archive a ON a.id = e.archive_id WHERE e.file_id = '00000007c2448f42';
-- everything of one type in one archive
SELECT r.name, r.file_id FROM resource r JOIN entry e ON e.id = r.entry_id
JOIN archive a ON a.id = e.archive_id
WHERE r.type_id = '85c817c3' AND a.name = 'DataPC_ACU_Paris.forge';
-- find by name
SELECT r.name, r.type_id, a.name FROM resource r JOIN entry e ON e.id = r.entry_id
JOIN archive a ON a.id = e.archive_id WHERE r.name LIKE 'GFX_Fire%';
```

## Limits (read before relying on it)
- `entry.typed_status` is `unsupported` for every data entry in this scan; it is not meaningful.
  Use `resource` for anything about content.
- `resource.type_name` is NULL unless you pass `--type-names` JSON (`{type_id: name}`). The CRC-32
  vocabulary behind ACU-RE-0034 depends on a strings file that is not in this checkout.
- `archive.baseline` is NULL (unknown) until `tools/baseline.py` (#37, PR #122) is merged and the
  index is rebuilt; the builder picks it up automatically when it is present.
- The scan is from 2026-10-18. Rebuild after rerunning `acu-scan`; the archives themselves
  are assumed unchanged.
- It indexes what `acu-scan` recorded. It does not decode any resource.

## Tests
`tools/test_build_asset_db.py`, 10 tests on synthetic scan rows: counts, archive-name
normalisation, ID lookup, resource provenance and type names, controls, empty name is NULL,
unknown baseline is NULL, no overwrite without `--replace`, malformed input fails with the line
number and leaves no output.

## Follow-ups
- Resource-level index of *world* types (cells, GridCellDataBlock) is now a query once
  `--type-names` is supplied; ACU-RE-0034 named this as its next step.
- Optional: include the 62 Paris base/patch pairs as a view once #124 is merged.
