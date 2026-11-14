# ACU-RE-0053 — Patch precedence: how the game mounts Forge archives and resolves a file ID (static)

Date: 2026-11-14. Related: #35, [ACU-RE-0038](ACU-RE-0038-patch-duplicate-ids.md),
[ACU-RE-0001](ACU-RE-0001-forge-compatibility.md), area assets-containers.
State: partially established. The mount order and the lookup order are read from code (static);
one live same-chain case agrees ("Live result").
Live confirmation for one known duplicate is planned below and not yet run.

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, image
  base `0x140000000`. RVA = VA − `0x140000000`.
- Mapped image `E:/ReverseEngineering/Projects/Unity/local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin`,
  SHA-256 `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520` (manifest: disk SHA-256 as
  above). Used for strings, vtables, rel32/pointer references and disassembly.
- Disassembly: Python 3.12 + capstone 5.0.3 over the mapped image (no REA/IDA session). The key mount
  function `0x142737350` has **no corpus file**; the corpus has `142737310.c` (59 bytes, then int3
  padding) and next `142737E20.c`, so everything about `0x142737350` below is from disassembly.
- Corpus files (`E:/ReverseEngineering/Projects/Unity/ACU - Decompiled/`): `142737310.c` (lines 8–17),
  `142725C60.c` (11–32), `142733330.c` (31–61, 120–129), `142727110.c` (19–75), `142727010.c`
  (19–54), `142733870.c` (64–74, 247–255), `14272C3C0.c` (11–16), `142719630.c` (4–18),
  `141D53540.c` (42–44, 96–106), `142757050.c` (20).
- Asset index (`python tools/asset_query.py`, index version 2026-10-22.1, READY), table `entry`.
- Search: corpus concatenated to one scratch file and searched with grep; image scanned for every
  rel32 and 8-byte pointer to the cited VAs.

## Result

### Observations

1. **Strings** (mapped image): `"Data%s.%sforge"` `0x14363BD28`, `"_assets"` `0x14363BF90`,
   `"_v%d"` `0x14363BF98`, `"_patch_%02u"` `0x14363BFA0`, `"\\"` `0x143015C88`,
   `"Data%s.forge"` `0x143024910`, `"_gi"` `0x143024908`, `"*.forge"` `0x143020588`,
   `"::Engine::DLC::AddPackage"` `0x143020590`. The only code references to `_assets`, `_v%d`
   and `_patch_%02u` are inside `0x142737350`.
2. **File-system manager.** `0x14272C3C0` allocates 280 bytes, constructs it with `0x142719630`
   (vtable `0x14363BD50`, `"::System::File"` profiler scope) and stores it in `qword_14525BDB0`.
   `0x142719630` zeroes the archive list pointer `+0xC4` and count area `+0xCC`, and sets a lock at
   `+0xD0` (`142719630.c` 10–12).
3. **Mount function `0x142737350` = vtable `0x14363BD50` slot `+0xB0`** (pointer at `0x14363BE00`;
   slot `+0xA8` `0x142737310` is a wrapper that calls slot `+0xB0`). Arguments as used:
   `rcx` manager, `rdx` 16-byte extra data, `r8` level name (char*), `r9d` flags, stack
   `[+0x28]` out group id, `[+0x30]` directory prefix. Sequence (disassembly addresses):
   - `0x1427373ED`–`0x14273750A`: builds `<level>_assets`, formats `Data<name>.forge`, tests the
     file (call through `0x145A5FA1D`, result compared with −1), and if present **recursively
     calls slot `+0xB0` for `<level>_assets` first** (`0x142737504`).
   - `0x142737549`–`0x1427376FD`: version probe. Starting at 1, tests `<level>_v<N+1>` and keeps
     the highest consecutive N that exists; if N > 1 the base name becomes `<level>_vN`
     (`0x142737702`–`0x142737757`).
   - Loop `r15 = 0, 1, 2, …` (`0x142737770`): name = `[prefix] + "\\" + base` plus
     `"_patch_%02u"`(r15) when r15 > 0; lowercased (`0x1428F738C`).
     - If an already-mounted entry has the same name (string compare over the list at `+0xC4`,
       `0x142737867`–`0x1427378B5`) → return that entry's id, increment its use count `+0x2C`, stop.
     - For r15 > 0, if `Data<name>.forge` does not exist → stop (`0x142737A31`). Patches must be
       consecutive from `_patch_01`.
     - Otherwise: `++manager+0xC0` is the new group id; a 0x40-byte entry (name `+0`, id `+8`,
       archive `+0x10`, use count `+0x2C`, child id list `+0x30`/count u16 `+0x3A`, flag `+0x3C`)
       and a 0x348-byte archive object (`0x142718900`) are created. The `rdx` extra data is copied
       into the archive (`+0x2B8`, `+0x2C0`) only for the base (r12 is zeroed when r15 == 1,
       `0x142737782`).
     - First entry of the chain (base): its id goes to the out parameter; the `_assets` group id
       (if any) is appended to its child list. Each later patch: its id is appended to the **base
       entry's** child list and the patch entry gets `+0x3C = 1` (`0x142737C5A`–`0x142737CD3`).
     - **Insert at the front:** `0x142725C60(list, 0, 8, 8, alloc)` then `[list+0] = entry`
       (`0x142737CF1`–`0x142737D01`). `142725C60.c` 27–32 moves the existing elements up by one
       and inserts at index 0. The archive path is copied to archive `+0x40` (≤ 0xFF bytes) and a
       hash of the file name part is stored at `+0x2F4`.
4. **Global resolver `0x142733330` = slot `+0x160`** (pointer at `0x14363BEB0`):
   `(manager, fileId, &groupId, &archive, &u64, &u32, usedGroups)`. It walks the list at `+0xC4`
   **from index 0 upward** and stops at the first archive whose index (`0x14277AA00` over
   `archive+56 → +96`) contains the file ID (`142733330.c` 32–61). It returns that entry's group id,
   archive, the qword at the 24-byte index record (`+80` table) and the dword at record `+16`
   (`142733330.c` 120–129), and records the group in `usedGroups` (use count `+44`).
5. **The resource read path uses that resolver.** `0x142733870` calls `[qword_14525BDB0]->+0x160`
   with the request's file ID (`*a2`) and outputs into the request (`a2+0x18` group id, `+0x10`
   archive, `+8` qword, `+0x48` dword) at `0x1427338DE`, and again at `0x142733CBD` for a second
   record (`142733870.c` 64–74, 247–255). Its only caller site is `0x14273377E`.
6. Other per-group lookups exist: `0x142727110` (slot `+0xD0`) first checks the requested group's
   own archive and only then scans the list front-to-back for any other group; `0x142727010`
   (slot `+0xD8`) checks a group and then its child list in append order. Their callers were not
   traced.
7. **All mount routes found go through slot `+0xB0`:** world GI `0x141D53540` builds
   `<world>_gi` and calls `0x142757050` (`141D53540.c` 44, 100), which calls slot `+176`
   (`142757050.c` 20); DLC routes `0x141D15370` / `0x141D25560` also call slot `+176`.
8. Asset-index duplicates usable as live markers (`entry`, stored bytes):
   - `00000007c2448f42` (Mesh `FRV_LM_Sainte-Chapelle_Window_Ornement_01_LOD0`, observed-decode):
     `DataPC_ACU_Paris_assets.forge` offset 1962512371 (`0x74F98FF3`), stored length 67020;
     `DataPC_ACU_Paris_assets_patch_02.forge` offset 207293 (`0x329BD`), stored length 67045.
   - `0000001499e2ce88`: `DataPC_ACU_Paris_gi.forge` offset 615786076 (`0x24B4265C`), 3044891;
     `DataPC_ACU_Paris_gi_patch_01.forge` offset 3471 (`0xD8F`), 3060518. Two more GI pairs:
     `00000016f1663468`, `0000001799470ba6`.
   - Base/patch shared IDs per Paris archive: assets patch_01 45, patch_02 2, patch_03 12, gi
     patch_01 3 (same as ACU-RE-0038).

### Inferences and alternatives

- **Inference (static, high):** for any lookup that goes through `0x142733330`, the most recently
  mounted archive that contains the ID wins. Within one chain the base is mounted first and each
  `_patch_NN` after it, each inserted at the front, so **the highest-numbered patch beats lower
  patches and the base**. An `_assets` chain is mounted (recursively) before its level's own chain,
  so the level's own archives come before its `_assets` archives in the list. Across levels/DLC,
  the level mounted last comes first.
- This is "latest mounted wins", not "latest archive name wins": it depends on runtime mount order
  (which levels are mounted, and when). ACU-RE-0038's warning stands for cross-level duplicates.
- Alternative not excluded: callers that use the group-scoped lookup `0x142727110` with the base
  group id would get the **base** copy when the base contains the ID. Which callers use it is
  unknown.
- The qword returned from the index record is inferred to be the entry's data offset (the
  on-disk 20-byte record starts with the u64 offset); not verified.

### Unknowns / incomplete coverage

- `0x142737350` is not in the corpus; field meanings rely on disassembly only. Calls into
  `0x145xxxxxx` thunks (file-existence test) are obfuscated; their API identity is not established.
- Unmount/reorder paths (e.g. world unload, DLC removal) were not traced; a later re-mount would
  move a chain to the front again.
- No live observation yet.

### Independent comparison / falsification test

See the live plan. Falsified if, for `00000007c2448f42` or `0000001499e2ce88`, the read path
returns the base archive while the patch archive is mounted, or if the live list order is not
newest-first.

### Reproduction steps

Disassemble `0x142737350`–`0x142737E20` and `0x142733870`–`0x142733F30` from the mapped image (VA −
`0x140000000` = file offset) with capstone in 64-bit mode; read the corpus files cited above.

### Implementation implications

- `acu_assets` may model a mount list with **insert-at-front** and a **first-hit** lookup, fed by
  an explicit mount sequence (base, `_v` selection, consecutive `_patch_NN`, `_assets` chain before
  the level chain). Keep it behind a "static, unverified live" flag until the plan below passes.
- Do not derive precedence from file names alone; record the mount sequence as data.

## Live result (run `precedence-1`, 2026-11-14)

- Setup: pid 86540, Paris world, during a hop scan (`hop-paris-2`, many cells streaming in).
  Profile `tools/native_probe/patch_precedence.profile.json` armed for 90 s; expected bytes matched.
- `forge_read_resolved` + `forge_read_archive_path` paired by thread and sequence: **672 resource
  reads**. Every (file ID, archive name, offset) triple exists in the local asset index (672/672).
  Archives read: `DataPC_ACU_Paris_assets` 531, `DataPC_SharedGroup_01` 44, `_00` 32, `_02` 24,
  `DataPC_ACU_Paris` 31, `DataPC_ACU_Paris_gi` 7, `DataPC_extra` 2,
  `DataPC_ACU_Paris_assets_patch_03` 1. `forge_mount` did not fire (no world was mounted then).
- 372 reads (364 IDs) were of IDs present in several archives. 371 were cross-world copies
  (Paris `_assets` vs Versailles / LGS `_assets`); the game read the copy in the world it had
  loaded (Paris).
- **One same-chain case:** `00000029969cf6b8` is in `DataPC_ACU_Paris_assets.forge` and
  `DataPC_ACU_Paris_assets_patch_03.forge`; the game read the **patch_03** copy. This agrees with
  the static result (newest mounted archive first), but it is one sample.
- Not yet observed live: the mount order itself (`forge_mount` during a world load), and a base
  copy beating a patch copy (the `0x142727110` alternative). Next: arm the profile across a fast
  travel, and look up both markers in Result.
- Derived data: `local/native-probe/runs/precedence-1/duplicate_reads.json` (machine-local).

## Live verification plan

Profile: `tools/native_probe/patch_precedence.profile.json` (4 probes). The two post-call probes are
mid-function instruction sites (allowed by the probe README as separately verified sites).

| Probe | VA (RVA) | First 16 bytes (mapped image) | Snapshot | Confirms / falsifies |
| --- | --- | --- | --- | --- |
| `forge_mount` | `0x142737350` (`2737350`) | `48895c242048894c2408555657415441` | `r8` +0, 64 bytes (level name) | Order of mount calls; expect `<level>_assets` (recursive) right after `<level>` |
| `forge_read_resolved` | `0x1427338E5` (`27338e5`) | `4d8b5c24104d8b9338030000418b8340` | `r12` +0, 80 bytes: `[0]` file ID, `[8]` qword, `[16]` archive, `[24]` group id, `[72]` dword | For `00000007c2448f42`: qword `0x329BD`, dword 67045 → patch_02 wins; `0x74F98FF3` / 67020 → base wins (falsifies) |
| `forge_read_archive_path` | `0x1427338EA` (`27338ea`) | `4d8b9338030000418b8340030000c1e0` | `r11` +0x40, 128 bytes (archive path) | Same event pair: path must end `_assets_patch_02.forge` |
| `forge_read_resolved_2` | `0x142733CC4` (`2733cc4`) | `448b0571d2b20265488b042558000000` | `rsi` +0, 32 bytes (second record: `[8]` qword, `[16]` archive, `[24]` group) | Same rule for the second lookup |

Also do once, with `READ` only (no hooks): `[0x14525BDB0]` → manager M; `M+0xC4` (u64 list) and
`M+0xCE` (u16 count); for each element E: `[E+0]` name, `E+8` id, `[E+0x10]+0x40` path,
`E+0x3A` child count. **Confirms** if the Paris entries read newest-first
(`…assets_patch_03`, `_02`, `_01`, `…assets`) and the base entry's child list holds the patch ids.

Procedure: launch with the pinned save (ACU-RE-0049 route), arm the profile, then fast travel to
Paris (GI and assets for Paris load). The read path is hot: expect ring drops; filter events by
`[0]` file ID ∈ the 62 Paris base/patch IDs (asset-index query in ACU-RE-0038). A single matching
event with patch path and patch offset/length confirms the inference for that chain; record the
event, archive path and list dump in this finding.

## Development handoff

- **Confidence:** static-high for mount order and the first-hit resolver used by `0x142733870`;
  live unverified; caller coverage of the group-scoped lookups unknown.
- **Established (static):** insert-at-front mount list (`0x142737CF1`), first-hit global resolver
  `0x142733330`, consecutive `_patch_NN` mounting, `_vN` selection, `_assets` chain mounted first.
- **Not established:** live order, unmount/remount behaviour, which code uses `0x142727110`.
- **Interface:** `MountList::mount(level) -> GroupId` (insert front), `resolve(file_id) ->
  (GroupId, archive, entry)` first hit; mount sequence is input data.
- **Follow-up:** run the plan; trace callers of slots `+0xD0`/`+0xD8`.
