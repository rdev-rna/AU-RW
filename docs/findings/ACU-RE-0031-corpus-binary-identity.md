# ACU-RE-0031 - Corpus and export binding to the installed executable

Date: 2026-10-22. Related task: ACU-003.
State: partially established. The mapped image and the function export are bound to the
installed executable by PE headers and file-backed resources. Code identity is
**unavailable**, not verified: the installed file is packed and carries no code bytes.

## Provenance

- Installed `ACU.exe`
  `C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games\Assassin's Creed Unity\ACU.exe`,
  SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`,
  30,907,608 bytes, image base `0x140000000`, SizeOfImage `0x7825000` (125,980,672),
  COFF timestamp `1423661094` (`0x54db5826`), 10 sections.
- Mapped runtime image `C:\Users\Administrator\acu-unity-re\dump\image.bin`,
  125,980,672 bytes, SHA-256
  `406d47f6f428f6f130be0501166b8798688406bdb7719f44b943a039a5b2a319`, offset == RVA.
- Reconstructed PE `C:\Users\Administrator\acu-unity-re\dump\dump.exe`, 125,979,136 bytes,
  SHA-256 `3723c9109aa4aaa0e0db26739b98e40be64252fbdba4d63739ba357cd466fc6d`, one page
  (`0x600`) shorter than SizeOfImage because the trailing padding is omitted.
- Export `ghidra_functions.csv`, 294,030 rows, header `address,name,size`, addresses are
  VAs (image base + RVA), Ghidra 12.1.4 headless per `acu-unity-re/MODLOG.md`.
  Rows: 0 unsorted, 0 duplicate, 0 malformed, 0 outside an executable section,
  0 auto-name/address disagreements (after the comma-in-name and symbol-name cases are
  handled - see "Corpus defects found").
- Capture provenance for the dump is documented, not re-executed:
  `acu-unity-re/MODLOG.md` section 1-2 and `Dump.cs` - live `ReadProcessMemory` of the
  unpacked process, polled at `0x140001000` until the MSVC prologue appeared.
- IDA session `a6d69650`, database `dump.exe.i64`, module `dump.exe`, imagebase
  `0x140000000` (IDA Pro 9.3 via the ida MCP server, headless). Bytes read with
  `get_bytes` at nine addresses; every one equals the same RVA in `image.bin`.
- Tool: `tools/bind_corpus.py` (new, Python standard library only, no file modified),
  regression `tools/test_corpus_binding.py` (18 tests). Python 3.14.6.
- Exact command:

  ```powershell
  python tools/bind_corpus.py `
    --executable "C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games\Assassin's Creed Unity\ACU.exe" `
    --image C:\Users\Administrator\acu-unity-re\dump\image.bin `
    --dump C:\Users\Administrator\acu-unity-re\dump\dump.exe `
    --export C:\Users\Administrator\acu-unity-re\ghidra_functions.csv `
    --corpus C:\Users\Administrator\acu-unity-re `
    --mutable-section .UBX1 `
    --output local/corpus-binding.json
  ```

  Exit status 0, verdict `bound_without_code_comparison`.

## Result

Observations:

- Header anchors all agree between the installed file and the mapped image: bytes
  `0x0-0x600` are identical, as are image base, SizeOfImage, COFF timestamp, section
  count, and every `(name, RVA, virtual size)` triple.
- File-backed resource sections match byte-for-byte: `.rsrc` 563,697 bytes (SHA-256
  `26b8e0447354fb6c...`), `.tls` 273 bytes (`4738696c8e1c97b4...`).
- On disk, `.text` `0x2e677af`, `.rdata` `0xb55f5a`, `.data` `0x18bfa68`, `.pdata`
  `0x304734`, `.shared`, `_RDATA` and `.UBX0` all have RawSize 0. Only
  `.tls` + `.UBX1` + `.rsrc` carry bytes: 30,907,608 of 125,980,672 bytes, a
  disk-backed fraction of 0.245275. This is why code identity is unavailable.
- `.UBX1` differs between disk and memory in four runs - `0x890000`,
  `0x1a95000`, `0x1aa3000` (one 4 KiB page each) and the tail
  `0x1cee000-0x1cee398` - totalling 13,208 of 30,335,896 bytes (0.04 %). Its first
  bytes are identical on both sides, and the first 64 bytes of `.text` occur nowhere in
  either `.UBX1` copy, so the payload is not stored in plaintext.
- Reconstructed PE vs mapped image: identical except the header page (`0x0-0x1000`) and
  a 1536-byte length difference; section layout is equal.
- Export binding: the image's own exception directory (`.pdata`, RVA `0x527f000`, 263,663
  `RUNTIME_FUNCTION` entries) resolves against the export as 161,187 exact starts
  (61.1368 %), 101,601 starts that lie inside an exported function body (chained unwind
  fragments that Ghidra merges), and 867 absent starts (0.3288 %). Resolved coverage
  `0.996712` against a required `0.99`.
- Five address-named decompile exports resolve, are exported, and are unwind starts:
  `0x1404a4580` (size 20,502, body SHA-256 `09cb4727e606c3c2...`),
  `0x1404ef310` (26,644, `841462e66e5560f9...`), `0x140ae2f20` (829,
  `a056757125544a0a...`), `0x140af1010` (495, `94dc0c31e9c2c82e...`),
  `0x142062630` (1,462, `3f2b65212b0cb6a0...`). Four more project-critical addresses
  (`0x142e77580` vtable, `0x140045240`, `0x140177c40`, `0x14017f8e0`) read
  non-zero and identical between IDA and `image.bin`.

Inferences and alternatives:

- The header and resource agreement, together with the documented dump method, is
  evidence that `image.bin` is a runtime image of *this* installed build rather than a
  different one. It is not proof: a build could preserve headers and resources while
  changing code.
- The `.UBX1` differences are consistent with runtime in-place patching by the loader
  after unpacking, because they are three isolated pages plus the section tail. Whether
  they are loader fixups, anti-tamper state or something else is unknown and unclaimed.
- Because `.text` has no disk bytes, no byte comparison of code is possible. The
  strongest statement available is `bound_by_headers_and_resources`, and the tool refuses
  to emit `bound_by_code` in that situation.

Unknowns / incomplete coverage:

- Whether the unpacked code was produced from this exact `.UBX1` payload cannot be
  established without the packer's key or a signed build artifact.
- The 867 unwind starts that appear in no exported function are unexplained; they may be
  leaf functions Ghidra left unnamed, unanalysed fragments, or region boundaries. Their
  VAs are in `local/corpus-binding.json` (`export_binding.absent_sample` lists the first
  ten).
- The three patched `.UBX1` pages are not identified.
- Full identity of `dump.exe.i64` beyond the nine probed addresses is not established;
  the probes are a sample, not a whole-database hash (IDA does not expose one).

Independent comparison / falsification test:

- Positive control: `.rsrc` and `.tls` must match exactly. A different build would
  almost certainly change `.rsrc`.
- Negative control: if any probed function's IDA bytes differed from `image.bin` at the
  same RVA, the binding would fail. All nine agreed.
- The export test is falsifiable: substituting a differently built export drops resolved
  coverage below the `0.99` gate, which the regression
  `test_image_that_does_not_match_the_export_is_unbound` exercises.
- `test_packed_code_section_is_unavailable_never_matched` pins the rule that a RawSize-0
  section is reported `unavailable` and never as a match.

Reproduction steps:

1. Run the command above. Expect exit 0, `verdict = bound_without_code_comparison`,
   `code_identity = unavailable_on_disk`, `resolved_coverage = 0.996712`.
2. Run `python -m unittest discover -s tools -p "test_*.py"` (45 tests, including the 18
   new ones).

Implementation implications and finding-linked constants:

- `image.bin` SHA-256 `406d47f6...`; `dump.exe` SHA-256 `3723c910...`; installed
  `ACU.exe` SHA-256 `748abdc6...` - all three must be recorded together whenever a
  finding cites corpus code, since none of them alone identifies the build.
- VA = `0x140000000` + RVA for every corpus address; the export CSV and the IDA database
  use VAs, `.pdata` and `dump.exe` section tables use RVAs.
- `.UBX1` is declared mutable in the binding record. Any *other* file-backed section
  difference makes the tool report `unbound`.
- Claims that depend on code behaviour may now cite the corpus as belonging to this
  installed build, but must not claim the code was byte-verified.

## Executable identities recorded in the docs

- Installed `ACU.exe` `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`
  - hashed directly from the install path above, and cited by every finding from
  ACU-RE-0001 to ACU-RE-0030.
- Mapped runtime image `406d47f6...` and reconstructed PE `3723c910...` - both hashed
  here; neither is the installed file.
- `40ded8de3629e875c504f50fb896922d4f1e1de478a60afca4d5ebc324ac362c` appears in
  `docs/STATUS.md:443`, `docs/VALIDATION.md:52` and `PROGRESS.md:14`, always inside the
  same sentence: "Both explicit install regressions pass ... Executable SHA-256:".
  It is **not** the installed `ACU.exe`, not `image.bin`, not `dump.exe`, and no file
  under `acu-rs/target` (123 `.exe`/`.dll` files hashed) reproduces it. It appears in no
  finding. `tools/review_arno.ps1` is the only tool in the repo that records an
  `executable_sha256`, and it records the viewer - but that is a consistent lead, not
  proof, and a rebuild would not reproduce the hash anyway.
- Consequence for the VA corpus: no finding depends on `40ded8de`. Every VA citation is
  anchored to `748abdc6` (installed file) plus the base/`VA = RVA + 0x140000000` mapping
  established above, so the unresolved hash does not weaken it. The three doc mentions
  are annotated as unidentified rather than deleted, so the history stays readable.

## Corpus defects found while binding

Both were in the tool, not the corpus, and are fixed in `tools/bind_corpus.py`:

- Demangled function names in `ghidra_functions.csv` contain unquoted commas
  (e.g. `_Med3_unchecked<SC*___ptr64,bool_(__cdecl*)(SC_const&___ptr64,...)>`), so naive
  `split(",")` reports good rows as malformed. The size is always the last field.
- Auto-names are `FUN_<VA>`, but the export also carries real symbol names
  (`ReadData`, `thunk_FUN_...`), so an address/name disagreement may only be flagged for
  `FUN_`-prefixed names.

## Note on the accepted decompile-join tool

`docs/NATIVE_DECOMPILE_XREF.md` and `tools/decompile_xref.py` state that their join "is
an address join, not binary identity proof" and that correspondence to the installed
executable "remains unverified". This finding fills that gap for the artifacts present on
this machine and keeps the same boundary: identity is established for the image and the
export, not for the semantics of any function.
