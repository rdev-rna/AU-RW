# ACU-RE-0043 — Parkour action types mapped to their classes

Date: 2026-11-14. Related: [ACU-RE-0041](ACU-RE-0041-acufixes-layout-spot-checks.md) (fancy-table
layout, enum getter), [ACU-RE-0042](ACU-RE-0042-parkour-candidate-trace.md) (live traces).
State: observed (static + live). Type numbers are tied to constructors, vtables and fancy tables.
**No action names**: names will come from the clips each type plays, not from external tables.

## Provenance

- Installed `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`,
  image base `0x140000000` (live base equal in every run used here).
- Corpus: `1401CC990.c` (SHA-256 `0df13100…2cd`) and `1401D1FC0.c` (`eabd7b80…3be`), the two
  factory switches; one constructor file per case.
- Mapped image `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520` (ACU-RE-0025).
- Tool: `tools/parkour_action_table.py --image <mapped image> --live <decoded run>…`; output
  `local/parkour-actions/table.json` (machine-local), SHA-256 `25626203…efeb4`.
- Live sources: decoded traces of runs `parkour-1`, `parkour-2-preset`, `parkour-5-runner`,
  `pinned-1a-putback-blocked`, `pinned-2a` (ACU-RE-0042 and this session).

## Method

1. Parse each factory's `switch`: allocation size (`sub_1426F3760(size, 16, …)`) and constructor.
   `0x1401D1FC0` uses character cases (`'3'` = 51 … `'8'` = 56).
2. In the constructor, the **last** `+8` store sets the fancy table: derived constructors call the
   base constructor (which stores the base table), then store their own. When the constructor
   ends by calling a helper with `a1`, follow it (depth ≤ 3).
3. In the mapped image, scan that function for `lea r64,[rip+X]` followed by `mov [base+8], r64`
   (per-register tracking; a byte-pattern scan, not a disassembler), take the last store.
4. Accept the table only if its enum getter (slot `0x45`, `mov eax, imm32; ret`, same resolver as
   `tools/parkour_trace.py`) returns a value; compare with the case number.
5. Cross-check against every live candidate's (fancy table, enum) pair.

## Result

| Quantity | Value |
| --- | --- |
| Factory cases | 112 (106 + 6) |
| Static fancy table whose getter returns the case number | 94 |
| Unresolved statically | 13 (types 17, 21, 34, 38, 41, 45, 57, 97, 100–103, 123) |
| Static table reports a different type | 10→9, 12→11, 33→65, 77→65, 78→65 |
| Distinct vtables / base constructors | 17 / at least 12 |
| Live fancy tables seen | 35: 31 agree with the static row, 0 disagree |
| Live tables not matched statically | 33 `0x1439EA7E0`, 77 `0x143A04A20`, 38 `0x143A3D130`, 57 `0x143A75170` |

Observations and inferences:

- Many types share one vtable and differ only in the fancy table (types 3, 4 and 5 all use vtable
  `0x142E88448`; fancy tables `0x143A46F80`, `0x143A3A2F0`, `0x143A62AB0`). Behaviour per type
  therefore lives in the fancy table, consistent with ACU-RE-0041.
- 10→9 and 12→11: the table each constructor ends with reports the neighbouring type. Whether a
  type-10 object really reports 9 at run time is not yet seen live.
- 33/77/78→65 are **static resolution errors** for 33 and 77: live objects report 33 and 77 with
  their own tables. Those constructors end in a shared helper (`sub_140172D90` for 33); the walk
  picks the wrong store. The live table is the evidence for these rows.
- Type 53 (2,199 live sightings, `0x143A58280`) comes from the second factory.

Limits: the scan is a heuristic and is only trusted where the enum getter confirms it. Rows
without live sightings are static-only. Names and meanings are open by design.
