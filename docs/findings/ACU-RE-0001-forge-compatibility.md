# ACU-RE-0001 — Plaintext Forge container compatibility

Date: 2026-10-18. Investigator: Codex / root. Task: ACU-001.
State: partially established (container and stored-byte boundaries verified;
compressed decode and patch resolution unresolved).

## Provenance

Target install: configured ACU game directory. Executable identity from preparation:
SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`,
PE image base `0x140000000`. This investigation reads archives, not code;
VA/RVA and pseudocode correspondence are not applicable evidence here.

Provider: Python 3.12.10 standard-library binary reads, struct and hashlib;
`tools/forge_probe.py`. No game code executed. REA CLI 6.1.0
`inspect-artifact` rejected the explicitly selected
`DataPC_extra_patch_01.forge` as unsupported binary format; no REA Evidence IDs
were produced. Shell observations below are not REA engine output.

FH reference, read-only:
`E:/ReverseEngineering/Projects/ForHonorRustRewrite/fh-rs/crates/fh_assets/src/forge.rs`;
SHA-256 `3a96420b8554ef7787b8a5051eed05a440255bd56c4c5174cae8a283f784dae7`;
last source commit reported by Git `fc6febdf7e7af4d264317af2624a68eedc12572f`.
Reviewed Forge::open, block, raw, decode_entry and companion scripts/forge_extract.py.
No sibling files changed. The probe implements reviewed field locations; it does
not vendor the FH Rust source or assign it new licensing/distribution terms.

Local report: `local/forge-compatibility.json` (reproducible, ignored).
Identities below hash exact byte ranges, not whole archives. All offsets are
archive-relative decimal byte offsets. File IDs are hexadecimal strings.

## Observations

| Archive | Length | Declared entries | Reachable entries | Duplicate IDs within reachable table |
| --- | ---: | ---: | ---: | --- |
| DataPC.forge | 312606720 | 1620 | 1620 | 0 |
| DataPC_patch_01.forge | 48529408 | 3 | 3 | 0 |
| DataPC_ACU_Paris.forge | 3850403840 | 24681 | 24677 | 0 |
| DataPC_extra_patch_01.forge | 163840 | 22 | 22 | 0 |

All four have plaintext `scimitar\0`, version 27, data-header offset
1050, first table 1094 and first entry array 1142. The table next pointer is
`ffffffffffffffff` in these samples. Every reachable 20-byte entry record
(`u64 offset, u64 ID, u32 stored length`) falls within its archive.
The probe records the header pointer as u64; FH reads its low u32. High bits
are zero here, so width beyond these inputs remains untested.

Paris has a four-entry count discrepancy. Its provenance is not assumed pristine:
`Backups/DataPC_ACU_Paris.forge` exists, sampled stored bytes include textual
2023 timestamps, and an extracted candidate name includes `modified`.
Those facts do not establish which tool changed which bytes or explain the count.
Do not silently repair the count or call this full index coverage.

### Structural range hashes

**DataPC.forge**

| Kind | Offset | Length | SHA-256 |
| --- | ---: | ---: | --- |
| header | 0 | 1050 | `42d0b13d169cf066e78521912a5deb9b6ada51125140b957a476a19ed201a8a4` |
| data_header | 1050 | 44 | `506d87a6c8cde34a8fe238cdb6e81108777f10dcf2ed6c05883900ebb341fce0` |
| table | 1094 | 48 | `5b99ad09bc5a0482d5dd2dd6aba85cd6276305375954af5e0a6edb8343708518` |
| entry_array | 1142 | 32400 | `ad543dcd8ba4d52edfae083af5d373cbaa2e8615cb9620be0fc12c459b56964c` |

**DataPC_patch_01.forge**

| Kind | Offset | Length | SHA-256 |
| --- | ---: | ---: | --- |
| header | 0 | 1050 | `42d0b13d169cf066e78521912a5deb9b6ada51125140b957a476a19ed201a8a4` |
| data_header | 1050 | 44 | `5f3bb82d5b6c58207ba7b9556b1cf5a71f6f3f47698f50762d542a439b7ba758` |
| table | 1094 | 48 | `a9a0bde4d0db6a6ffe7ce591b45256889cc0c7538a369254b2584669f962a024` |
| entry_array | 1142 | 60 | `c87cd295c10057179076d1e4d3206d09655fff1aa0bf97ca8798ec178af0a5f5` |

**DataPC_ACU_Paris.forge**

| Kind | Offset | Length | SHA-256 |
| --- | ---: | ---: | --- |
| header | 0 | 1050 | `42d0b13d169cf066e78521912a5deb9b6ada51125140b957a476a19ed201a8a4` |
| data_header | 1050 | 44 | `d2db42e875479973dcc462bb04746738507d57e9cc40da50f373a6b1da161c06` |
| table | 1094 | 48 | `ac9704c8e97077a7b38fd2e650cc7a1a19bed7032e9500f0f5f827e0a3b42058` |
| entry_array | 1142 | 493540 | `58843f50b6d36397da7dab440b624edb24d180f6f5e701a6141ac1b9df284314` |

**DataPC_extra_patch_01.forge**

| Kind | Offset | Length | SHA-256 |
| --- | ---: | ---: | --- |
| header | 0 | 1050 | `42d0b13d169cf066e78521912a5deb9b6ada51125140b957a476a19ed201a8a4` |
| data_header | 1050 | 44 | `96508cb8d75595c5b6fd55db08f0317c8debb56275f0df9934a595e284cd28a5` |
| table | 1094 | 48 | `afe675ce4b997476b7ca03726b9e8efdf3202ed93a8a98a22d6ccfe102128aad` |
| entry_array | 1142 | 440 | `735d8d52def23a9a5cea4ad90511c015cbee958470036badfe0a0a69a37eac9f` |

### Sampled stored entries

Lengths and SHA-256 here describe stored bytes only. Decoded lengths/hashes are
unavailable; no compressed decoder was run.

| Archive / index | File ID | Offset | Stored length | SHA-256 |
| --- | --- | ---: | ---: | --- |
| DataPC.forge / 0 | `0000000000000010` | 630091 | 42100 | `30fff3eef19744b637187964d8ad253884c39c62d6233ec63d43953a3fb7f7c3` |
| DataPC.forge / 1 | `0000000000000008` | 672191 | 164 | `9d647e486d3f2287821d5173d8da2190929d6bec7908c447b51a7ccd8679ac5b` |
| DataPC.forge / 2 | `0000000000000009` | 672355 | 164 | `47bf162405dffd794a1a962d574a0296153677ce001429c31742b6bff2239026` |
| DataPC_patch_01.forge / 0 | `0000000000000800` | 2695 | 48499190 | `12f47a478eff8e84c7e31d110afcfe99c45dcb39a2e5f5dc4a5bb364aa9b8dfa` |
| DataPC_patch_01.forge / 1 | `0000000000000010` | 48501885 | 280 | `402e0cf67b126696f781287aa0e253eb3952041e80653c01a7dfb1743f2f4892` |
| DataPC_patch_01.forge / 2 | `0000000000000091` | 48502165 | 6318 | `e5738b33ded6a1aeb5173e82ca6d18a29134c274822390b1229015902c1dca49` |
| DataPC_ACU_Paris.forge / 0 | `0000000000000010` | 6112666 | 280 | `e593c2b69aed5b82d44fcaa33c22813c7c671a49c9f187dd54716692609fc7de` |
| DataPC_ACU_Paris.forge / 1 | `00000023a0fbf0fb` | 6112946 | 25283456 | `e2ff6ab733d597f31bbb09a050d1678805a8f303b9a35ba28605bff81244004d` |
| DataPC_ACU_Paris.forge / 2 | `00000023dab179da` | 31396402 | 38234332 | `b581ce169e0d0add562c3fa2bcdf89c07b873498814b3b1c3055286928d6dc40` |
| DataPC_extra_patch_01.forge / 0 | `00000014af51413a` | 10067 | 835 | `7a624195a8399b8e88e02a741f6ac5867849444c3ca976063f4a24b4dc877ab9` |
| DataPC_extra_patch_01.forge / 1 | `0000001eef19026a` | 10902 | 752 | `c882af8f7e431119150f8c12ef4447cb8480e456bda43e830a78e4a8aae2d5fd` |
| DataPC_extra_patch_01.forge / 2 | `0000001eef190365` | 11654 | 485 | `e7509865f395a9fedce5edf9dc74c4b3bd58738a4ce05d221c7e16ca14b301c7` |

### Compatibility matrix

| Boundary | FH reader | ACU observation / result |
| --- | --- | --- |
| Magic/header/tables/records | XXTEA key discovery and per-block decrypt | Same reviewed field positions work as plaintext; omit crypto |
| Entry framing | Oodle trailer or alternative header, optional final crypto flag | Sample compressed entries instead start `33aafb5799fa0410`; FH decoder cannot be reused unchanged |
| Terminal byte | decode_entry pops last byte outside recognized trailer | Preserve all stored bytes; no ACU encryption-flag interpretation established |
| Raw entry | raw reads exact size | Reusable operation with bounds/allocation limits |
| Compression provider | oo2core_7_win64.dll | Not justified for ACU. No validated decoder/provider selected yet |
| Tables/counts | Follows next pointer, ignores final count mismatch | Same walk works; surface Paris mismatch explicitly |
| Duplicate IDs / patches | Separate install/index assumptions | No within-archive duplicates in reachable records; cross-archive ID 0000000000000010 occurs; precedence unknown |

The new frame header has candidate fields version 1, algorithm 1 and word
`80008000` in most compressed samples. Patch entry ID `0000000000000091`
instead starts with candidate algorithm 0. Byte meanings are provisional.
A read-only comparison with local
`E:/ReverseEngineering/assassins-creed-modding-kit-main/tools/rogue/anvil.py`
found the same magic and an LZO1X-based Rogue format (algorithm 0), but that
source is only a hypothesis generator: its count width differs from the ACU
candidate and it has not been run on ACU. Do not infer algorithm 1 or copy its
unbounded decoder. LZO1X is a candidate to validate, not an established ACU codec.

### Independent extracted-file comparison

Three extra-patch files and one Paris metadata file match their stored archive
ranges byte for byte:

- `DataPC_ACU_Paris.forge`, index 0: `0_-_GlobalMetaFile.MetaFile`; length 280; SHA-256 `e593c2b69aed5b82d44fcaa33c22813c7c671a49c9f187dd54716692609fc7de`.

- `DataPC_extra_patch_01.forge`, index 0: `0_-_UI_Icons_New_MapDesc.data`; length 835; SHA-256 `7a624195a8399b8e88e02a741f6ac5867849444c3ca976063f4a24b4dc877ab9`.

- `DataPC_extra_patch_01.forge`, index 1: `1_-_UI_Icon_New_MapDesc.data`; length 752; SHA-256 `c882af8f7e431119150f8c12ef4447cb8480e456bda43e830a78e4a8aae2d5fd`.

- `DataPC_extra_patch_01.forge`, index 2: `2_-_UI_Icon_Locked_MapDesc.data`; length 485; SHA-256 `e7509865f395a9fedce5edf9dc74c4b3bd58738a4ce05d221c7e16ca14b301c7`.

The Paris index-2 filename candidate differs in length and was rejected as a
stored-byte match. Filenames' leading numbers are candidate table ordinals,
not file IDs. Matching compressed stored bytes establishes the boundary and
extraction correspondence only; it is not independent decompression validation.
Extracted tools/build identities remain unknown.

## Inferences, unknowns and falsification

Removing XXTEA is sufficient for the sampled container table path. It is
insufficient for FH decode_entry because the frame bytes differ. This makes a
small container adaptation justified without importing XXTEA or FH's Oodle wrapper.
No statement about all 59 top-level archives, DLC, payload encryption elsewhere,
resource schemas, patch order or decoded gameplay data follows from these samples.

Falsify the table interpretation with any record outside the archive, a loop,
unjustified count, or an independent entry reference that disagrees with the
recorded ID/range. Validate the compression hypothesis using bounded frame parsing,
exact consumed input/output lengths, checksum interpretation and independently
identified decoded resource bytes. A codec returning bytes alone is insufficient.

## Reproduction and validation

Run from project root:

```powershell
python tools/forge_probe.py DataPC.forge DataPC_patch_01.forge DataPC_ACU_Paris.forge DataPC_extra_patch_01.forge --output local/forge-compatibility.json
python -m unittest discover -s tools -p test_forge_probe.py
```

Probe succeeds, validating 26,322 reachable records and hashing 12 stored samples.
Three authored fixture tests pass, including independent expected ID/range,
unsupported version, huge header/count, array/entry overflow, table cycle and
truncated header. These are structural tests, not original-game decode parity.

## Bounded implementation plan

1. ACU-002 may bootstrap Rust and port only plaintext container/index/raw access
   into acu_assets. Verify then-current official Rust/Bevy compatibility and pin
   versions before workspace creation. Keep this Python probe as comparison evidence.
2. Preserve IDs/ranges, use checked arithmetic and file-length checks before reads;
   cap tables at 4096, records at 1,000,000 and sampled/stored allocation at 64 MiB.
   Reject unsupported variants, detect cycles and expose the Paris count mismatch.
   These are defensive probe choices, not recovered native limits.
3. Resolve compressed-frame headers/codec 0 and 1 before exposing decoded reads.
   Select a reviewed independent decoder or identified ACU native routine; use a
   small base entry and extra-patch entry as fixtures. Report compressed reads as
   unsupported until independently verified. No dependency/provider acquisition
   is required for the current investigation.
4. Resolve patch precedence separately; do not implement filename sorting as
   recovered behavior. Check pristine/backup Paris bytes before full-index claims.

ACU-001 compatibility investigation is complete with compressed decoding an
explicit unresolved implementation boundary, not a support claim.
