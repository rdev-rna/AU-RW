# 33 - ACU-003 corpus and export binding to the installed executable

Issue: #33 (ACU-003, `area:binding`, epic #1). Branch: `acu-33-corpus-binding`.
Finding: `docs/findings/ACU-RE-0031-corpus-binary-identity.md`.

## Goal

Satisfy ACU-003: identify the export source/build, bind an analysis session by
executable hash, verify exported function locations against native bytes, and document
image base, VA/RVA mapping and gaps without renaming or regenerating the corpus.

## Result

Bound. The mapped image and the function export belong to the installed build, anchored
on PE headers and file-backed resources. **Code identity is unavailable, not verified:**
the installed file is packed, so `.text`, `.rdata`, `.data` and `.pdata` have RawSize 0
and no code byte could be compared. The tool cannot return `bound_by_code` in that
situation by construction.

| Artifact | SHA-256 | Size |
| --- | --- | --- |
| installed `ACU.exe` | `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40` | 30,907,608 |
| mapped runtime image | `406d47f6f428f6f130be0501166b8798688406bdb7719f44b943a039a5b2a319` | 125,980,672 |
| reconstructed PE | `3723c9109aa4aaa0e0db26739b98e40be64252fbdba4d63739ba357cd466fc6d` | 125,979,136 |

- Header bytes `0x0-0x600`, image base `0x140000000`, SizeOfImage `0x7825000`, COFF
  timestamp `0x54db5826` and the section table are identical between the installed file
  and the mapped image.
- `.rsrc` (563,697 B) and `.tls` (273 B) match byte-for-byte. These are the only
  file-backed, non-code sections.
- 30,907,608 of 125,980,672 bytes are disk-backed (fraction 0.245275).
- `.UBX1` differs in 13,208 of 30,335,896 bytes: `0x890000`, `0x1a95000`, `0x1aa3000`
  (one 4 KiB page each) and the tail `0x1cee000-0x1cee398`. Declared mutable in the
  binding record; every other file-backed difference would make the tool report unbound.
- Export `ghidra_functions.csv`: 294,030 rows, 0 malformed, 0 unsorted, 0 duplicate,
  0 outside an executable section, 0 auto-name/address disagreements.
- Export vs the image's own exception directory (RVA `0x527f000`, 263,663
  `RUNTIME_FUNCTION` entries): 161,187 exact starts (0.611368), 101,601 chained fragments
  contained in an exported function, 867 absent (0.003288). Resolved coverage 0.996712
  against a 0.99 gate.
- IDA session `a6d69650` (`dump.exe.i64`, imagebase `0x140000000`): the nine probed VAs
  agree with `image.bin` at the same RVA. The five address-named decompiles resolve,
  are exported and are unwind starts: `0x1404a4580`, `0x1404ef310`, `0x140ae2f20`,
  `0x140af1010`, `0x142062630`; plus `0x142e77580`, `0x140045240`, `0x140177c40`,
  `0x14017f8e0`.

VA mapping for every corpus claim: `VA = RVA + 0x140000000`. The export CSV and the IDA
database use VAs; `.pdata` entries and reconstructed section tables use RVAs.

## Commands run and actual results

```powershell
# build (toolchain pin is invalid upstream; override on every invocation)
$env:RUSTUP_TOOLCHAIN = 'stable'
cargo build --locked --workspace          # Finished dev profile, exit 0
```

```powershell
# scene smoke test, headless offscreen capture (Vulkan, RTX 3060 Ti)
powershell -NoProfile -ExecutionPolicy Bypass -File .\tools\run_arno.ps1 `
  -Capture local/captures/headless-idle.png -Scenario idle
# CPU skin mean over 60 frames: 5.286 ms; capture saved after 60 frames; exit 0
```

Scenarios captured: idle, walk (60 ticks), run (60), reversals (300), jump (40). Arno
loads from the configured install with textures, shadows and posed bones; `reversals`
shows a mid-turn cross-step. This is a scene smoke test only.

```powershell
python tools/acu.py inventory --output local/inventory.json   # exit 0, 49 corpus files
python tools/bind_corpus.py `
  --executable <install>/ACU.exe --image <corpus>/dump/image.bin `
  --dump <corpus>/dump/dump.exe --export <corpus>/ghidra_functions.csv `
  --corpus <corpus> --mutable-section .UBX1 --output local/corpus-binding.json
# verdict bound_without_code_comparison, code_identity unavailable_on_disk, exit 0
```

```powershell
python -m unittest discover -s tools -p "test_*.py"   # 45 tests, OK
python tools/acu.py check                             # Workflow check: PASS (21 files)
```

`tools/test_corpus_binding.py` adds 18 tests: packed section reported unavailable and
never matched, byte-matched and byte-mismatched code, declared-mutable tolerance, base
mismatch naming the anchor, export parsing (unquoted commas in demangled names, symbol
vs `FUN_` auto-names), exact/contained/absent unwind classification, end-to-end verdicts.

## Coverage and remaining uncertainty

- Unavailable by construction: code bytes in the installed file. Do not claim the corpus
  code was byte-verified against it.
- Unexplained: the 867 unwind starts that appear in no exported function (first ten in
  `local/corpus-binding.json`, `export_binding.absent_sample`); the purpose of the three
  patched `.UBX1` pages; and which packer output produced them.
- Not attempted: re-executing the game to re-dump. The existing dump's provenance is
  documented in `acu-unity-re/MODLOG.md` and was not re-verified beyond the probes.
- No function semantics, ownership or behaviour is established here.

## Follow-ups for maintainers (not edited here)

- `docs/STATUS.md`, `docs/TASKS.md`, `docs/FINDINGS.md`: summary lines for this slice are
  left to maintainers per `docs/PROJECT_BOARD.md`. Suggested text: ACU-003 done (bound by
  headers and resources; code identity unavailable).
- `docs/AREAS.md` on `board-protocol` still says binding is "none" and "unverified until
  ACU-003"; that row needs a one-line update once the protocol lands.
- Issue #34 (`40ded8de…`): the three gate-sentence mentions in `STATUS.md`,
  `VALIDATION.md` and `PROGRESS.md` are still unannotated because those files are
  maintainer-owned. Evidence is in ACU-RE-0031: the hash is not the installed executable,
  the mapped image or the reconstructed PE, and none of 123 `.exe`/`.dll` files under
  `acu-rs/target` reproduces it. No finding depends on it.

## Locks

An IDA/REA session (`a6d69650`) was opened for this slice and closed afterwards; see the
`lock:ida-rea-session` issue for the acquire/release record. No game process was launched,
no input was injected, no run directory was created.
