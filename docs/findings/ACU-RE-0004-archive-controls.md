# ACU-RE-0004 — Global archive metadata and control dispatch

Date: 2026-10-18. Task: ACU-005. State: partially established.

## Provenance

Install executable SHA-256
`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`,
PE image base `0x140000000`. This is asset-byte evidence; no native function
VA/RVA or corpus correspondence is asserted.

`DataPC.forge`, entry 0, file ID `0000000000000010`, offset 630091,
stored/decoded length 42100, uncompressed SHA-256
`30fff3eef19744b637187964d8ad253884c39c62d6233ec63d43953a3fb7f7c3`.
The archive identity is the indexed range and digest, not a whole-file hash.
Full scan provenance: `local/coverage-complete-audit/entries.jsonl` and summary
SHA-256 `15ec05a80d5fc959ee1323328222d0eac5ecabe3992b6e6c6785e0ab4dc043b1`.

Read-only reference: AnvilToolkit.dll 1.3.7.0, SHA-256
`940374775553172b06d55537c4db78048bc3206b9b2f72bcc61bacfdcdb57636`;
REA identity evidence `ev_01f1308aac08023bb993cc29cf977fa03e4a7076b1985e3d9d02ee25a0c7db1a`
from the prior investigation. Focused ilspycmd 8.2.0 decompilation of
`AnvilToolkit.FileTypes.AnvilNext.Containers.MetaFile`, `MetaFileAttribute`,
`MetaFileItemTypes.MetaItemType`, `STRING`, `U32`, `U64_ARRAY` and `ForgeEntry`
is retained in ignored `local/reference-atk/`. These are reference format
descriptions, not proof of native execution; no implementation text imported.

## Observations

The 130 previous package failures comprise 63 global metadata records (ID
`0000000000000010`), 63 single-frame prefetch records (ID
`0000000000000091`), and four other Paris records. IDs are hexadecimal:
decimal 10 and 91 identify ordinary resources and must not select control paths.

Independent Python structural parsing consumed every byte of all 63 metadata
records, including DLC. Layout: little-endian u32 version 1, repeated u32
attribute key / u32 value type / typed value, then a u32 zero-key terminator.
Observed value types are 0=u32, 1=u32 byte length / UTF-8 text / zero byte,
6=u32 **byte length** / u64 array. Array lengths must be divisible by eight.
Observed keys are 1,2,3,4,5,7,10,11,12,13,14,15,16,19,21,24,30,31,32,33,999.
Names come from the reference mapping; notably key21 content packages and key33
fast-load signatures are different concepts. Keep their u64 values lossless in
JSON as 16-digit hex strings; do not interpret signatures as file references.
Reproduction: `python local/probe_meta.py`; report `local/meta-structure.json`.
The runtime implementation will expose typed fields and reject unsupported types,
unknown keys, unsupported versions, missing terminators and trailing bytes.

The four other failures are Paris entries 1–4, IDs `00000023a0fbf0fb`,
`00000023dab179da`, `0000002347f7952c`, `000000232002f58a`. All lack the normal
first-frame magic. Their original bytes and stored hashes remain visible in
`local/control-failures.json`; they are not control records and must remain
failures. The same archive declares four more entries than its reachable table.
A relationship is possible but unproven. Do not silently synthesize missing
magic, adjust the denominator or modify the archive.

## Inferences, unknowns and validation

Dispatch by evidenced reserved ID distinguishes archive controls from two-frame
resource packages. ID0x91 is named PrefetchingFileInfos by the reference; its
semantics and internal framing need separate evidence (ACU-RE-0006).
Metadata primitive parsing does not prove native patch precedence, GUID meaning,
or runtime behavior. Unknown formats fail explicitly.

Independent structural report and Rust install tests can falsify field widths,
byte-count interpretation and termination. Malformed authored inputs must cover
truncation, nonmultiple array sizes, bad string terminators, unknown types/keys,
trailing bytes and allocation/count limits. No rendering or native execution
parity claim applies to this CPU reader.
