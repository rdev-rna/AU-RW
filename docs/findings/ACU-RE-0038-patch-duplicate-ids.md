# ACU-RE-0038 - Duplicate file IDs across Forge carriers (static scan)

Date: 2026-10-22. Related: #35 (reserved here), #36, area assets-containers.
State: partially established. Duplicates and their byte equality are observed. Which copy the
game uses is **unresolved**; nothing here recovers the runtime loader.

## Provenance

- Install: the 63 carriers found by `acu_assets::forge::discover` (59 top level + 4 in
  `dlc_*`); `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`.
  Baseline list per #37 (mod layer, Backups, Extracted excluded).
- Tool: `acu-rs/crates/acu_assets/src/bin/acu-dup-ids.rs`, built release, SHA-256
  `24b8be0d22557a1c8c66493a40b8461127ba7bfcfe1f380eedbd9423fd641efe` (the run used this
  binary; the source was then only reformatted with rustfmt).
- Command: `ACU_GAME_DIR=<install> acu-dup-ids local/35-dup-ids-20261022`
- Outputs (ignored, local): `summary.json` SHA-256
  `abfb7324f451ee0c4def983735fdd32d90b14c2481ebbbec836c11504e7deabe`; `duplicates.jsonl`
  `b855d0bf98a7652281ebb7672b73fba63d0835e2827a1e1698d09d1a52fa44d5`.
- Hashes are SHA-256 of the **stored** (still framed) entry bytes. Nothing was decoded.

## Result

Observations:

- 180,245 reachable entries in 63 carriers, no index errors. Each archive holds exactly one
  global-metadata (`0x10`) and one prefetch (`0x91`) control entry (63 each); they repeat by
  design and are excluded from the duplicate count. The entry total equals the 180,245 in STATUS.
- 150,064 distinct data IDs. 134,379 occur in one archive; **15,685 occur in several**
  (12,545 byte-identical in every copy, **3,140 differing**). No ID repeats inside one archive.
- Most overlap is between different levels/DLC (for example `Paris_assets` with
  `Versailles_assets`: 5,927 identical, 197 differing; with `Saint-Denis`: 3,993 / 912), not
  between a base and its patches.
- Patch archives are small (1 to 597 data entries). Every ID a patch shares with its own
  base differs in bytes in the Paris chains below (0 identical).
- **Paris** (the area in active use):
  - `DataPC_ACU_Paris.forge` has no patch archives. 1,069 of its IDs also occur elsewhere, all in
    other levels/DLC.
  - `DataPC_ACU_Paris_assets.forge` + `_patch_01/02/03`: **59 IDs** have a patch copy
    (45 / 2 / 12). Each of the 59 occurs in the base and exactly one patch, never two patches, so
    no patch-versus-patch ordering question arises for this chain. 58 of the 59 occur nowhere else.
  - `DataPC_ACU_Paris_gi.forge` + `_gi_patch_01`: 3 IDs, all differing.
  - Example: `00000007c2448f42` stored length 67,020 (base) vs 67,045 (patch_02).

Inferences and alternatives:

- A patch entry that shares an ID with its base and differs is a *candidate* replacement. The
  alternatives (base wins, the loader merges, or the patch is ignored for that level) are not
  excluded by this scan.
- The same ID differing between unrelated levels (3,140 total, mostly cross-level) may be
  level-local variants that are never mounted together. Not tested.

Unknowns / incomplete coverage:

- The runtime mounting order and override rule: needs the loader (IDA/REA, issue #35 step 2).
- Whether an entry with an ID is the same logical resource: ID equality only.
- Differing stored bytes could differ only in framing or checksum, not in decoded content.
- Patch archives were not decoded; identical-versus-differing is on stored bytes.

Independent comparison / falsification test:

- `tools/forge_probe.py` (a separate Python parser of the same index) re-parsed the Paris assets
  and gi chains and hashed the stored ranges itself. It reproduced all 62 base/patch pairs
  (59 + 3): same IDs, stored lengths and SHA-256 as the Rust report, 0 mismatches, 62 of 62 differing.
- Consistency: per archive, control + data entries equal reachable entries; the sum is 180,245.

Reproduction steps: build `acu-dup-ids` (release) and run the command above; filter
`duplicates.jsonl` by archive name.

Implementation implications:

- Do not assume "latest archive wins" in `acu_assets`; `discover` continues to imply no priority.
- For Paris, only 62 IDs depend on precedence, so the loader question is small and bounded.
  Readers should record which archive each resource came from.
- Next: find the loader's mount/override rule (needs the ida-rea-session lock), then check it
  against these 62 IDs.
