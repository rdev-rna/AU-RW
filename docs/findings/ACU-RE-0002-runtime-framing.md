# ACU-RE-0002 — Runtime entry framing

2026-10-18, Codex, runtime decoder task. State: partially established.

Input provenance starts with the stored entry ranges and full SHA-256 values in
[ACU-RE-0001](ACU-RE-0001-forge-compatibility.md). No native VA/RVA claim is made.
Experiments use the explicit ACU install and standalone Rust lzokay 2.0.1.

Hypothesis under test: ACU frames with magic 1004fa9957fbaa33 have u16 version,
u8 algorithm, u32 flags, u32 block count, followed by u16 decoded/stored block
length pairs, each block preceded by a u32 checksum. LZO1X is tested only as a
candidate for observed algorithm 0/1; do not infer successful resource parsing
from decompressing a frame. Checksums are measured before enforcing semantics.
The related Rogue reader uses a different block count width and is not imported.

Reproduce with `cargo run --bin acu-probe -- ARCHIVE ENTRY_INDEX` from acu-rs/.
Initial targets: DataPC.forge entry 1 and DataPC_extra_patch_01.forge entry 0;
small, independently identified table records, no extracted-folder dependency.
Report hashes, consumed bytes, decoded lengths and checksum comparisons.

## Established framing and independent check

Rust lzokay 2.0.1 agrees byte for byte with the independent native
`lzo1x_decompress_safe` provider in
`C:/Users/PC/Desktop/ATK/Libs/lzo.dll`, SHA-256
6407c266e7a954e58a3ffff0ea4303020dde3bd58997789d662a131bf1c9a7b9.
Reproduce: `python tools/validate_lzo.py C:/Users/PC/Desktop/ATK/Libs/lzo.dll`.
Output hashes/lengths only; neither the validator nor runtime extracts assets.
The DLL is a validation provider only, not a runtime dependency.

| Archive / frame-relative offset | Decoded length | SHA-256 |
| --- | ---: | --- |
| DataPC.forge / 0 | 16 | d27e6552283faa527d4279b6693a99bb09a30a3e606ebd0dff9da8dc848342ec |
| DataPC.forge / 40 | 229 | 9fe2ca94dd611b45eb33a25d6c29ef104dfd3aeb29546f2649eb9a790e73b9a0 |
| DataPC_extra_patch_01.forge / 0 | 30 | 242b9a48540731c91f28044b6d9383f26f795d4081bf7845a3fb923e5d060dad |
| DataPC_extra_patch_01.forge / 53 | 4532 | d58914b7f91d2b9ab99075d4efd1116180e1599317c0fd5529bbe0ad49824b52 |

Stored entry identities/offsets are those from ACU-RE-0001: base offset 672191,
length 164, ID 0000000000000008; patch offset 10067, length 835,
ID 00000014af51413a. Metadata is the first frame; resources are the second.

The per-block checksum is Adler-32 with initial a=0, b=0 over stored block
bytes, not the common zlib initial a=1. It is enforced before decompression.
Output lengths must match exactly. Table-of-contents mode has a u32 block count
for ACU, u16 length pairs for compression version 1 and u32 pairs for versions
0/2/3. Default mode consists of blocks with byte 0 (u32 length, raw bytes) or
byte 1 (u32 stored length, u32 decoded length, checksum, compressed bytes),
ending at EOF or before the next frame. Decoded allocation and record/table
counts are bounded. The runtime uses an explicit ACU format boundary, no XXTEA.

Additional format interpretation was checked against locally installed
AnvilToolkit.dll 1.3.7.0, SHA-256
940374775553172b06d55537c4db78048bc3206b9b2f72bcc61bacfdcdb57636.
REA CLI 6.1.0 managed-static identity evidence:
ev_01f1308aac08023bb993cc29cf977fa03e4a7076b1985e3d9d02ee25a0c7db1a;
provider rea-dotnet-static version 1, .NET 9 CIL, module MVID
e37db2ad-a6cd-4088-940e-18243e142619. This is a tooling target, not the ACU
executable. No game-code equivalence or execution is claimed from it.
Targeted ilspycmd 8.2.0 output retained under ignored `local/reference-atk/`:
CompressedFileData.ReadTOCMode/ReadDefaultMode, CompressionInfo.Read,
DataBlock.Read, CompressionManager.GetCompressionAlgorithm and DataFile.Deserialize.
The CLI identity preview was truncated after the complete identity section;
no unshown metadata is treated as inspected. No toolkit code is vendored.

Resource metadata begins u16 count followed by observed u64 ID/u32 record
length/u16 flags records. Flags 0 are supported. Unknown metadata extensions
are preserved in memory and reported as incomplete. Resource envelope fields
are u32 type/length/name length, name bytes, and header byte 0, or header byte 1
with three additional bytes, u32 reference count and count*12 bytes. Payload
length and metadata record length must agree exactly; the entire resource data
frame must be covered. These headers are not typed asset content.

## Coverage limitations

An initial full sweep enumerated 63 original top-level/DLC archives and 180,245
reachable entry records, identifying at least 171 resource type hashes. That
baseline predates variable resource-header/default-frame support; its failures
are preserved in `local/coverage-full/`. New runs use separate report directories.

Global metadata entries (ID 0000000000000010), index/control entries (often ID
0000000000000091), Paris count mismatch and
typed assets still need work. A preserved metadata extension is explicitly not
full metadata parsing. Never report resource envelopes or compressed frame
success as model/map/animation/texture success.

### Additional metadata record variant

The revised scan isolated 6,367 packages with a metadata u16 field equal to 1.
Independent native-LZO inspection of BelleEpoque entry 38 (offset 63986486,
stored length 165761, SHA-256
92e337e803c4169ed2a0d807bcb48d88f5b678d6e903c1b634344d7b85fba11f)
finds 375 metadata records and four such fields, at record indices
35/41/271/293. Each is followed by one additional u16 (zero in this vector),
accounting exactly for the eight-byte difference from count*14+2. Skipping eight
bytes per flagged record was tested and rejected: the next ID/length becomes
invalid. The two-byte variant instead consumes the entire 5,260-byte metadata
frame and is checked against resource envelope sizes/identities.
Metadata frame SHA-256 b332dedc90adc063d5ab3f00e82032691e7f51d83191acd06b4324a85a536cb4;
resource frame decoded length 473778, SHA-256
60e30c88d97db214d1e08796038004002216d0837fbae2b564dff5a98f1dd702.

Implementation interprets the field structurally as a count and preserves its
u16 elements in Resource.metadata_tags with length/bounds checks. The tags' native
semantic meaning remains unknown; no streaming/filter interpretation is assigned.
This supersedes the earlier flag-0-only limitation; it does not establish typed
contents of these resources.

A second independent native-LZO vector validates counts 0/1/2/3: BelleEpoque
entry 287, offset 112110974, length 8712408, SHA-256
806d537f922bf349d38f3651c21774d5bc3738b7cae75e1efaf52c4bc868680f.
Its 2,004 metadata records have count distribution 194/1703/98/9, consuming
exactly 31,910 bytes, SHA-256
0c0ed40cff5299dc5e39f97257d960df9a24fc26a8560506ffa7958d6844b5cc.
All record sizes consume exactly the 23,765,675-byte resource frame, SHA-256
5558afc4642877d6b7d8e1e42ec2458bf53d7da58c8f33ad32b8795f6025b3d1,
with root class IDs cross-checked where present. No asset bytes were written.

### Completed installed-archive audit

`local/coverage-complete-audit/summary.json` reports scan_finished=true across
63 original top-level/DLC archives: 180,245 reachable entries, 180,115 decoded
and resource-indexed packages, 3,289,051 resource envelopes and 484 type hashes.
No decoded package leaves a metadata extension. The remaining 130 entries are
unsupported control entries; Paris retains its declared/reachable discrepancy.
Typed success is limited to 39,302 inline 2D textures. The scanner exits 1 and
both all-assets success flags remain false. No assets were extracted.

Reproduction uses ACU_GAME_DIR and acu-scan as documented in acu-rs/README.md.
Report SHA-256: 15ec05a80d5fc959ee1323328222d0eac5ecabe3992b6e6c6785e0ab4dc043b1.
Scanner executable SHA-256: 88fd8c686ef02aca840cb0f56bf04ef6fa1f4a6b2ded42ca27afb212131ed4f6.
These counts establish structural coverage, not native rendering or behavior parity.
