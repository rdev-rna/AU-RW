# ACU-RE-0006 — ACU prefetch control index

2026-10-18 / Codex / ACU-005. State: observed structure; native runtime
policy and action meanings remain unresolved.

## Provenance

Explicit target: the 63 installed original ACU Forge archives, including DLC.
Control file ID is the uint64 string `0000000000000091`. No ACU executable
VA/RVA, corpus-function correspondence, or game execution is claimed.
The installed executable identity remains that in ACU-RE-0001.

Representative archive ranges (offsets decimal; identities are stored-entry
SHA-256, not whole-archive hashes):

| Archive | Entry | Offset | Stored bytes | Stored SHA-256 |
| --- | ---: | ---: | ---: | --- |
| DataPC.forge | 1619 | 312582772 | 15592 | 409ae7104a83cca0bf68bd7a46b2b0785221d7e6d3befef45a54fe8f56444db0 |
| DataPC_ACU_LGS_BelleEpoque_gi.forge | 255 | 606373515 | 1238 | 6ffe5972e0232dfd5e49c4f09f3d841da905f1a18de2c264916837e4d4bea4f7 |
| DataPC_ACU_Paris.forge | 24676 | 3841740120 | 8632917 | 022004e7d5f9f89388a89603409801acea997b47cf7280796258bf8e83c71ae5 |

Decoded outer-frame identities:

| Archive | Decoded bytes | Decoded SHA-256 |
| --- | ---: | --- |
| DataPC.forge | 33730 | 3b2c78136bb91a5161d4e31df11f5d629f42737a032be9539297ce5a54b1d8c9 |
| DataPC_ACU_LGS_BelleEpoque_gi.forge | 4076 | 533e3617b8144b04b40000d76d8e85336effe53f3bbd1dc9370b5a6afb34b42d |
| DataPC_ACU_Paris.forge | 10243896 | ea1a20678213e2d8bf3d443b1fe699c53531a868c61c54b6555ca0e210525e5e |

Independent decompression provider: native `lzo1x_decompress_safe` in
`C:/Users/PC/Desktop/ATK/Libs/lzo.dll`, SHA-256
`6407c266e7a954e58a3ffff0ea4303020dde3bd58997789d662a131bf1c9a7b9`.
Python 3.12 ctypes decodes every outer frame and nested frame; zlib verifies
Adler-32 with seed zero before decompression. Local experiment scripts are
`local/probe_prefetch.py`, `local/prefetch_validate_child.py`; complete per-entry
range identities and census are retained in `local/prefetch-validated-child.json`.
These scripts inspect ranges in archives and do not extract assets.

AnvilToolkit.dll 1.3.7.0 is the secondary structure-name reference, not an ACU
decoder oracle. Its identity and REA managed-static evidence are recorded in
[ACU-RE-0002](ACU-RE-0002-runtime-framing.md). ilspycmd 8.2.0 targeted classes
PrefetchingFileInfos, PrefetchFileInfoObject, PrefetchInfo, DataLayerFilter and
DataLayerAction; output is retained in `local/reference-atk/`. The toolkit
explicitly excludes Unity from PrefetchingFileInfos.SupportedGames. Its outer
formats must not be imported as ACU behavior. The subrecord hypotheses below
were tested against all installed ACU control entries.

## Result

Every observed 0x91 entry is one complete default-mode frame. Its decoded data
is little endian:

1. u32 row count, then that many 16-byte rows: u64 file ID, u32 size/flags,
   u32 blob-relative offset.
2. u32 blob byte length, followed by exactly that many bytes.
3. Row size low 31 bits select the stored byte range. High bit means that
   particular range contains one complete nested ACU frame. Clear bit selects
   raw dependency-block bytes. Offset zero is immediately after blob length.

Distinct stored ranges tile the complete blob, with no gaps or overlaps.
Many rows share exactly the same range. Distinct ranges are decoded once;
rows preserve their original ordering and identities. No ID-based override or
archive precedence rule is inferred. The apparent u64 blob length in the first
probe was actually a u32 length followed by the four-zero empty block.

Decoded dependency-block bytes contain:

1. u16 filter count.
2. For each filter: u16 action count/flags, high bit retained as boolean,
   low 15 bits action count; each action is u8 numeric action and u64 data-layer ID.
3. u16 dependency count; each dependency is u64 file ID, u8 numeric policy,
   u16 filter-index count, then u16 indices into this block's filter vector.

All bytes are consumed and all indices resolve in all 63 controls. The complete
census is 180,119 rows, 30,679 unique ranges (14,380 raw and 16,299 compressed),
19,816 filters and 2,344,758 dependencies counted once per unique range.
Paris alone has 24,675 rows, 7,910 ranges, 16,053 filters and 1,640,169 dependencies.
Policy values are preserved; their native load timing/priority meaning is unknown.
Filter action meaning, data-layer activation, dependency graph closure and patch
selection remain unverified. This finding establishes parsing, not game parity.

## Validation and reproduction

Rust `prefetch::parse` implements this observed ACU boundary independently from
the Python native-LZO experiment. `Limits` are defensive implementation choices.
It checks counts before allocation, bounds every range, caps aggregate decoded
unique-block bytes, deduplicates shared ranges, checks nested-frame consumption,
rejects conflicting compression flags and checks every filter index. JSON IDs
remain exact 16-digit hexadecimal strings. Unsupported/malformed variants fail.

From the project root, the native experiment is
`python local/prefetch_validate_child.py` (63/63 parsed). From `acu-rs/`:

```powershell
cargo test --locked -p acu_assets --test prefetch
$env:ACU_GAME_DIR="D:/Games/Assassin's Creed Unity"
cargo test --locked -p acu_assets --test prefetch -- --ignored --nocapture
```

Both pass: two authored tests cover exact shared records, high-bit filter flag,
64-bit IDs, explicit action/policy/index fields, every prefix truncation, range
gaps/overlaps, zero lengths, conflicting compression flags, nested trailing
bytes and decoded budget rejection. The explicit install test parses all 63
control entries, matches independent census totals and DataPC stored/decoded
hashes. Missing installation reports unavailable/error, never success.
