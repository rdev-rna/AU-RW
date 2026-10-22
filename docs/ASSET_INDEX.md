# Asset index (full guide)

A local SQLite database that lists every resource in the 63 original Forge archives, where it
lives, what it is called, what type it is, and which other resources it appears to reference.
It exists so nobody (human or agent) has to rescan multi-gigabyte archives to answer "where is
X" or "what is near X". Agents: start with [ASSET_INDEX_QUICKSTART.md](ASSET_INDEX_QUICKSTART.md).

It holds **metadata only**: IDs, names, offsets, lengths, hashes. It contains no game bytes.
The Rust parser (`acu_assets`) remains how data is decoded and shown in Bevy; the index only
tells you where to point it.

## Getting it
```
python tools/index_status.py           # status; says what to run
python tools/index_status.py --fetch   # download from the GitHub Release, verify, unpack
```
`data/asset_index.manifest.json` (committed, small) names the release asset and carries its
SHA-256. `--fetch` downloads with a progress bar and resumes an interrupted download, checks the
checksum, then unpacks to `local/asset_index.sqlite` (about 2 GB; keep it on a fast disk, queries
are random reads). `git pull` bringing a changed manifest is how you learn of a new version;
`index_status.py` then reports `NEW asset index version ... available`.
A session-start hook (`.claude/settings.json`) runs `index_status.py --hook`, which starts any
download in the background and never blocks the session.

## Tables
| Table | One row per | Notes |
|---|---|---|
| `archive` | Forge carrier (63) | `baseline=1` for every original carrier; `scan_size_bytes` vs `size_bytes` shows whether it changed after the scan |
| `entry` | archive entry (180,245) | a package (`kind='data'`) or a control entry (`'control'`, two per archive) |
| `resource` | resource envelope inside a package | `file_id`, `name`, `type_id`, `type_name`, byte ranges, `payload_sha256` |
| `texture` | TextureMap resource | raw descriptor fields (`map_type`, `category`, size, format); their meaning is **not** established |
| `link` | candidate reference | source resource, target ID, first offset, hit count, `kind` |
| `type_label` | resource type (484) | label, category, `confidence`, `basis`, counts, sample names |
| `loose_file` | non-Forge install file | audio `.pck`, video `.bk2`, localization, ...; mod layer, backups, logs excluded |
| `meta` | key | version, build date, merge report |

Views: `link_view` (links with source/target names and types), `fan_in` (how many distinct
resources point at each target; high counts suggest shared assets such as atlases).

## What each claim is worth
**Observed** (reproducible from the archives): every row in `archive`, `entry`, `resource`,
`texture`. The audit re-parses every archive's entry table and compares it with the index.

**Links are candidates.** `acu-links` decodes every resource payload and reports each place
where eight bytes (little-endian, any byte offset) equal a known file ID. A hit may be a real
reference, a different kind of value, or coincidence. A control experiment calibrated this: the
same scan against same-sized fake IDs matched about 0.002% as often as real ones from 25 bits
up, and as often as real ones below that. So `kind='candidate'` (ID >= 2^25) is strong evidence
that the bytes equal an ID, and `kind='weak'` is noise (those IDs look like ordinary integers).
Neither says what the field means. Typed decoders (for example Entity/LODSelector work in
progress under #135) are what turn a candidate into a named link. Handles, hashes and 32-bit
references are invisible to this method.

**Type labels carry a confidence.**
- `observed-decode`: the Rust parser decoded resources of this type into a family (TextureMap,
  Mesh, Skeleton). Strongest.
- `crc-name`: the type ID is the CRC-32 of a class name, and that name hashes to this ID
  (verified on the repo's anchors in ACU-RE-0002/0034). Names come from the findings and from
  string literals in the decompiled corpus; with about 9.5k candidate names against 484 types a
  chance match is expected about 0.001 times in total.
- `unknown`: nothing identifies it. About 440 of 484 types, though they hold a minority of rows.
Categories are a browsing aid chosen here, not recorded by the game.

## Known gaps and cautions
- **Which duplicate wins is unknown.** 15,685 IDs occur in several archives; `where` lists every
  copy. For Paris only 62 IDs depend on patch precedence (ACU-RE-0038, issue #35).
- **Contents of most types are not decoded.** The index says where a LODSelector, AtomGraph or
  Animation is, not what is inside it. Decoding is the work of the area issues.
- **Names**: about 121k resources have no name. Type names are missing for most small types.
- **Paris was rescanned.** The first scan (2026-10-18) read a 3,850,403,840-byte
  `DataPC_ACU_Paris.forge`; the file on disk now is 3,770,744,832 bytes and byte-identical to the
  `Backups` copy. Older notes ("24,681 declared", "4 bad Paris entries", 3,289,051 resources)
  describe the old file. This index uses the current one.
- **Non-Forge data** (audio, video, localization) is listed in `loose_file` with size and hash
  but not parsed.
- **The mod layer** (`ACUFixes` and its overrides) is excluded by name and never opened.

## Verifying it
```
python tools/audit_asset_db.py local/asset_index.sqlite --game-dir "<install>"
```
Without `--game-dir` it skips the fresh-parse comparison. A `FAIL` line is a hole; `INFO` lines
are documented gaps. The audit runs again inside the packaging step and blocks a bad release.

## For the maintainer: rebuilding and publishing
1. Scan (or reuse a scan): `acu-scan <dir>`; rescan any archive that changed.
2. `python tools/build_asset_db.py <scan> [--override <rescan>] --output local/asset_index.sqlite --replace`
3. `acu-links <dir>`, then `python tools/asset_graph.py merge local/asset_index.sqlite <dir>/links.tsv`
4. `python tools/annotate_asset_db.py local/asset_index.sqlite --game-dir "<install>" --scan <scan> --names <identifiers> --replace`
5. `python tools/package_asset_index.py --links-summary <dir>/summary.json --game-dir "<install>" --version YYYY-MM-DD.N`
   (audits first, scrubs local paths, writes `local/dist/asset_index.sqlite.gz` and the manifest)
6. Publish the archive as a release asset, then commit the manifest:
   `gh release create asset-index-<version> local/dist/asset_index.sqlite.gz --title "Asset index <version>" --notes "..."`
7. To withdraw it later: `gh release delete asset-index-<version> --cleanup-tag`. Clients then
   report that the release was removed; anyone who already unpacked it keeps working.

Disk note: the working index is large and heavily indexed. On a spinning disk, queries and the
audit are slow (random reads); copy it to an SSD first.
