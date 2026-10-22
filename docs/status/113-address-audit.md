# 113 - Address-citation audit tool

Issue: #113 (`type:infra`, `area:infra`). Branch: `acu-113-address-audit`.
Files: `tools/verify_cited_addresses.py`, `tools/test_cited_addresses.py`.

## Result

Audits every VA and hex guard quoted in the docs against a real image of the build, and
classifies each citation. Run on the 2026-10-22 live image, 65 files, 513 citations:

- 195 land exactly on a `.pdata` function entry
- 80 land mid-function (hook sites, or errors - not decidable by tooling)
- 102 land in a non-executable section
- 91 are live pointers outside the module (correctly so)
- 42 are the module base itself
- **0 land in `0xCC` padding**

No demonstrable wrong address.

## Method and its limits

A guard is verified only when the pairing is unambiguous: the guard follows the VA on the
same line, or the VA line ends with "bytes" and the guard starts the next line. Anything
else is reported `va-only` rather than guessed.

That rule exists because the first version produced ten false mismatches: a guard on the
previous line was applied to the wrong VA, four address/guard pairs sharing one line got
the first guard each, `0x142e768c0` had the guard of the vtable target it points at, and
`XINPUT1_3.dll` entry guards were reported "absent" from ACU's image. All four failure
modes are now documented in the module docstring.

Deliberately not done: proposing corrections. The tool measures; a human decides.

## Commands

```powershell
python tools/verify_cited_addresses.py `
  --image local/34-live-hash-20261022-1146/live/image.bin `
  --output local/cited-addresses.json
python -m unittest discover -s tools -p "test_*.py"
```
