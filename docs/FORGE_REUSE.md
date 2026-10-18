# For Honor runtime reader reuse

Runtime update: `acu-rs/crates/acu_assets` implements the plaintext container
boundary with allocation/table checks and an ACU-specific LZO decoder;
see [ACU-RE-0002](findings/ACU-RE-0002-runtime-framing.md). Native-library
base/patch comparisons now succeed. This supersedes the early codec unknowns
below for the verified variants. Extraction is not required or performed by
the runtime reader. Complete typed resource coverage remains ACU-005.

Update 2026-10-18: [ACU-RE-0001](findings/ACU-RE-0001-forge-compatibility.md)
validates the FH container field positions without decryption against four ACU
archives (26,322 reachable entries). Removing crypto works for those tables.
Sampled compressed entries use different framing from FH's Oodle path; preserve
the last byte and implement an explicit ACU decoder boundary. Paris has a four-entry
count discrepancy. The early observations below remain historical context.

The user identified the existing Rust runtime reader as a starting point. The
following files were inspected read-only on 2026-10-18 under
`E:/ReverseEngineering/Projects/ForHonorRustRewrite/`:

| Relative path | Role / observed assumptions |
| --- | --- |
| `fh-rs/crates/fh_assets/src/forge.rs` | `Forge::open`, `raw`, `read`, `decode_entry`; XXTEA container blocks and flagged entry prefix; trailer and alternate framing |
| `fh-rs/crates/fh_assets/src/xxtea.rs` | Dependency named by the runtime reader; algorithm/key compatibility with ACU not inspected yet |
| `fh-rs/crates/fh_assets/src/oodle.rs` | Loads For Honor's `oo2core_7_win64.dll`; checks exact decompressed length |
| `fh-rs/crates/fh_assets/src/index.rs`, `install.rs` | Candidate discovery/index boundaries; Unity-specific behavior still to inspect |
| `fh-rs/crates/fh_assets/tests/forge_install.rs` | Ignored install test compares known lengths/hashes against Python reader fixtures |
| `scripts/forge_extract.py`, `scripts/forge_crypt.py` | Companion reference tools; not executed or independently validated during preparation |

## Concrete early difference

The first 32 bytes of `DataPC.forge`, `DataPC_patch_01.forge` and
`DataPC_ACU_Paris.forge` in the ACU install were read. All three begin with literal
ASCII `scimitar` (`73 63 69 6d 69 74 61 72`); bytes 9–12 are
`1b 00 00 00`. The For Honor reader documents an encrypted magic/header and calls
`xxtea::find_key` before decrypting header blocks (`forge.rs`, `Forge::open`).

Observation: these ACU prefixes are plaintext. Inference to test: the ACU
container path may need a plaintext header/table implementation rather than
For Honor's encrypted path. Unknown: payload framing, encryption elsewhere,
codec, table layout and the meaning of bytes 9–12. Do not infer the entire
archive format from these prefixes or assume that disabling XXTEA is sufficient.
No top-level `oo2core_7_win64.dll` appeared in the inspected ACU listing; codec
availability and deeper install contents remain to investigate.

## Compatibility gate (ACU-001)

Build a matrix covering base archives, patches and each newly encountered variant:

1. Magic/version/header offsets, table chaining, counts and entry bounds.
2. Plaintext/encrypted container and entry paths; distinguish encryption from
   compression and nested resource framing using actual bytes.
3. Codec identification, output sizes, failure behavior and available decoder.
   Do not require or borrow the For Honor Oodle DLL without evidence it applies.
4. Duplicate file IDs, patch order and resource-reference semantics. Unity patch
   filenames differ from assumptions in For Honor tests; naming alone is no proof.
5. Independent payload comparison with identified extracted files where possible.
   Names such as `10000_-_Cell08155_DataBlock.data` are not yet proven file IDs.

Import only the smallest justified source set into `acu_assets`, preserving
origin path, source SHA-256/revision and applicable notices. Inspect source
license/ownership before choosing distribution terms; no new license is assigned
by this preparation. Avoid importing the entire `fh_assets` crate: its manifest
also depends on `fh_native_math` and unrelated resource formats.

Retain a For Honor reference vector for any unchanged shared algorithm, then add
independent Unity vectors. Add offset/size/overflow checks, allocation limits,
decompression limits and table-cycle detection before archive-wide scanning.
Unsupported variants must fail explicitly instead of returning plausible bytes.
Do not alter the sibling project or run its full game-dependent tests here.

Record results as a finding before claiming compatibility. No Rust parser was
copied, compiled or run on ACU during preparation.
