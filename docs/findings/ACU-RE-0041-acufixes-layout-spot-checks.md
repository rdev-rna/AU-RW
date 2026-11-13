# ACU-RE-0041 — ACUFixes layouts checked against this build: parkour action selection, Animation, AnimTrackData

Date: 2026-11-14. Related: movement plan Stage 4, ACU-RE-0030, ACU-RE-0018/0040.
Reference review: [ACUFIXES_REFERENCE.md](../ACUFIXES_REFERENCE.md).
State: observed. Storage offsets, vtable/fancy-table identities and the selection loop
are established for this build. Field *meanings* beyond what the code shows are ACUFixes'
interpretations and remain unverified.

## Provenance

- Lead source: ACUFixes `e20228a0d775ca18b16d45000847ee0f0aa0f372`, unlicensed (cite only).
  Claims taken from `ACUFixes/src/ParkourDebugging/AvailableParkourAction.h`,
  `ParkourTester.h`, `GenericHooksInParkourFiltering.cpp`,
  `CommonLibACU/ACU-RE/inc/ACU/Animation.h`, `FancyVTable.h`,
  `ACUFixes/src/AnimationTools/AnimationEditor.cpp`. No code was copied.
- Installed `ACU.exe` `D:/Games/Assassin's Creed Unity/ACU.exe`, SHA-256
  `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, image base
  `0x140000000`, COFF timestamp `1423661094`. Packed on disk (ACU-RE-0031), so code
  checks use runtime images.
- Mapped images (offset == RVA), main `Unity` checkout:
  - `local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin`,
    SHA-256 `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`
    (same image as ACU-RE-0030);
  - `local/native-probe/bone-trace-20261019/main-image.mapped.bin`, SHA-256
    `07afa161c61064659c033827ff8d91b1e3c7a0759a46286e810962f312f02137`.
  Both are 125,980,672 bytes. All checks give identical results on both.
- Corpus `E:/ReverseEngineering/Projects/Unity/ACU - Decompiled/` (VA-named files, the
  export bound in ACU-RE-0031). Files cited, with SHA-256:

  | File | SHA-256 |
  | --- | --- |
  | `140133B00.c` (112 lines) | `59d2f929e7e10855658f5b0aa08a9caecf432bdf651a0e8988cd9b174ca88c88` |
  | `1401D2640.c` | `ca83111bafb2538c49c3a9011d0d969ce1d966d9c7ffa4844aaae33128c1e821` |
  | `1401D3000.c` | `9f87eabebfeb8a5c29c97c72f1ea4a951015219a6dfef2f0be688aafdd820844` |
  | `1401D4DE0.c` | `67d146e874df0fd99f4bc84148c4d5ec3561342ef3979686124487cb514de2a4` |
  | `1401D1FC0.c` | `eabd7b804790145db2f3f6c850b2467dd8ef396a7a9a771e78f0dd3e616693be` |
  | `1401BCD30.c` | `5d79cf29ea1c61ef162c006776622cd631650912012136eedb051f8c5ab3f01f` |
  | `1401AF2D0.c` | `490a758c81cc832f2abd513443d442df9397a683d6cf26c305757517efd65e90` |
  | `140053270.c` | `c264d8a7737c2c43c7c8c87eb086426625de721eee12069ec1a3be7712f472f7` |
  | `140054D60.c` | `83c7efd4abf489fc8abaef3183651d5fe6d34afa2c2d362753684a8df06a6edb` |
  | `1400530D0.c` | `5c2736ce79fa6c30fc1e191927e4f6b73dce3645cfa145747ba4c580dfabedc4` |

- Tools: `tools/acufixes_spot_check.py` (new, standard library only; exact-byte checks);
  Capstone 5.0.3 for the initial disassembly of the getters; Python `zlib.crc32` for
  name hashes. No IDA/REA session: the only open IDA session was a For Honor database
  and was not used. No game process was run; no hook was installed.

## Result

### 1. Parkour action selection (`0x140133B00`)

Observations (corpus `140133B00.c:34-111`):

- Pass 1 (`:34-50`): for each candidate in the array argument, call `0x1401D2640`; if the
  absolute value is `<= 0.0000099999997`, remove the element (`0x142725F00`).
- A virtual call at receiver fancy-table offset `+720` runs before sorting (`:51-58`).
- Pass 2 (`:59-99`): insertion sort, descending, by
  `value(0x1401D2640) * receiver_fancy(+0x510)(receiver, candidate)`.
- Pass 3 (`:100-111`): return the first index whose candidate passes both
  `0x1401D4DE0(candidate, a2, a8, *(receiver+0x118))` and
  `0x1401D3000(candidate, *(receiver+0x118), a8)`; return `-1` when none pass.
- `0x1401D2640`, `0x1401D3000` and `0x1401D4DE0` are one-line thunks that call
  `*(obj+8) + 0x470`, `+0x9D0` and `+0xED0`.

Fancy-table layout. ACUFixes describes 0x20-byte entries with the function pointer at
+0x10. Under that layout the thunk offsets are indices `0x23`, `0x4E` and `0x76`, and the
receiver call `+0x510` is index `0x28`. These are exactly the indices ACUFixes gives for
`GetFitness`, `FinalFilter2`, `FinalFilter1` and `ParkourTester::GetDefaultWeightForAction`.

Concrete table. Ctor `0x1401BCD30` stores C++ vtable `0x142E88468` at +0 and fancy
table `0x143A6DFB0` at +8 (mapped bytes; the corpus shows the store at
`1401BCD30.c:17-18`). That table has 120 (`0x78`) entries whose `staticIdx` equals their
slot. At slots `0x18`, `0x23`, `0x45`, `0x4E` and `0x76` the name hash equals the value
ACUFixes records (`0x8EB3C7D3`, `0x7A9E7BE6`, `0x986EB60D`, `0x323150BF`, `0xF0455AE0`).
The three getters checked are:

| Slot | Function | Mapped bytes | Meaning |
| --- | --- | --- | --- |
| `0x23` | `0x1401A7620` | `f3 0f 10 81 04 02 00 00 c3` | `movss xmm0,[rcx+0x204]; ret` → candidate float at +0x204 |
| `0x18` | `0x1401A2100` | `f3 0f 10 81 c8 00 00 00 c3` | `movss xmm0,[rcx+0xC8]; ret` → candidate float at +0xC8 |
| `0x45` | `0x1401AF2D0` | `b8 33 00 00 00 c3` | returns 51 (corpus `1401AF2D0.c:4`) |

The factory `0x1401D1FC0` constructs this class through `0x1401BCD30` for action value
`'3'` = 51 (`1401D1FC0.c:32-38`). So the class's enum getter returns the same value the
factory dispatches on. Allocation sizes in that factory are 848–880 bytes
(`1401D1FC0.c:33-78`).

Name status: `crc32("GetDistance") = 0x8EB3C7D3`, so slot `0x18` is CRC-confirmed as
`GetDistance` (ACUFixes also marks it confirmed). The other four checked names do not
hash to their table values. They are ACUFixes' descriptive names.

### 2. `Animation` (vtable `0x142E7ACC0`, TypeInfo `0x1439CD110`)

Observations: `140053270.c` is a thunk to `0x140054D60`. That ctor (`140054D60.c:4-16`)
installs a base vtable `0x142E795B0`, then `0x142E7ACC0`. It initializes small arrays at
+0x18, +0x24, +0x30 and +0x60 (`sub_1423D0690`), masks the low nibble of byte +0x50,
zeroes qwords +0x40, +0x48 and +0x58, and increments a global counter at `0x145137A38`.
In the mapped image, vtable slot 5 (`0x1400724E0`) is `mov rax,[rip+…]; ret` loading
`0x1439CD110`, the TypeInfo address ACUFixes gives.

Agreement: ACUFixes' `Animation` places small arrays at +0x18/+0x24/+0x30/+0x60, a pointer
at +0x40, two 32-bit fields at +0x48/+0x4C (`Length`, `AnimationKey`), a 1-bit field at
+0x50 and size 0x70. The ctor's stores fit that layout. They do not establish that +0x48
is the clip length or +0x4C the key; those names are unverified.

### 3. `AnimTrackData` (ctor `0x1400530D0`)

Observations (`1400530D0.c:15-19`): small-array init at +6, dword +0 = `-1`, word +4 = 0,
word +0x10 = 0, qword +0x18 = 0. This agrees with ACUFixes' 0x20-byte layout: key at +0,
16-bit ref count at +4, small array at +6 (its count field lands at +0x10), back-pointer
at +0x18.

### 4. Owner of the ACU-RE-0030 scanner caller (inference)

Observations: the mapped image contains 11 raw pointers to `0x1401A7000`. Every one sits
at +0x10 of a fancy entry with name hash `0x518DDA9D`, staticIdx `0x7A` and nestedness 0.
Each containing table runs 124–125 sequential entries, and its slot 0 has hash
`0x090A5560`, the same as slot 0 of the parkour-action table `0x143A6DFB0`.

Inference: `0x1401A7000` is fancy virtual `0x7A` on 11 subclasses in the parkour-action
family that extend the 0x78-entry base table. Its stores at +400/+416 and +128
(ACU-RE-0030) would then be fields of the action object: +0x190/+0x1A0 are base-class
vectors that ACUFixes leaves unnamed, and +0x80 is a vector that subclass ctors also
write (`1401BCD30.c:20`). Unknown: class names, the meaning of slot `0x7A`, and which
enum values these 11 classes carry.

### 5. Codec descriptor table (structural only)

At `0x145137A40` the image holds 72 non-null pointers. Each points to a descriptor whose
byte +0x21 equals its slot index, and each descriptor has a code pointer at +0x10. This
agrees with ACUFixes' description. Whether the slot is the track `type` of ACU-RE-0018/0040
(codec = type / 3) is **not** tested.

## Unknowns / incomplete coverage

- Only one action class (enum 51) had its getters disassembled. Other subclasses may
  override `GetFitness`/`GetDistance` differently; the base field offsets are shown only
  for this class.
- The meaning of `fitness`, the tester weight, both filters and the `+720` pre-sort call
  are not decoded. The `+0x118` receiver field is passed where ACUFixes expects the player
  entity; not checked.
- Runtime images are captures, not the packed file; code identity follows ACU-RE-0031's
  `bound_without_code_comparison` limit.
- Other ACUFixes layouts (Entity, Bone, Skeleton, AtomAnimComponent, World, HumanStates)
  were **not** checked and are not covered by this finding.

## Independent comparison / falsification

- Two captures from different sessions (2026-10-19, 2026-10-20) give identical results.
- Every check is an exact byte or integer comparison. A different build, or a capture with
  ACUFixes' replacement hooks applied at these functions, would fail them. The three
  ACUFixes full-replacement sites (`0x140133B00`, `0x1401D1CF0`, `0x1401D1E50`) have
  identical non-jump prologues in both images.
- The CRC-32 test is a negative control on naming: four of five ACUFixes names fail it,
  so the script does not confirm names it cannot confirm.

## Reproduction

```powershell
python tools/acufixes_spot_check.py "<Unity>/local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin"
python tools/acufixes_spot_check.py "<Unity>/local/native-probe/bone-trace-20261019/main-image.mapped.bin"
```

Expect 12 `PASS` lines and exit status 0 for each. Read the corpus files above for the
pseudocode observations.

## Implementation implications

- A native probe for the first traversal decision trace can hook `0x140133B00` and read,
  per candidate, the enum (fancy `0x45`), the float at +0x204 and the float at +0xC8 through
  the verified getters, then record the returned index. Guard it on the prologue bytes.
- For the Rust reconstruction, the observed selection rule is: drop near-zero
  candidates, sort by product with a tester weight, take the first candidate passing two
  filters. That is native structure, not yet a tunable rule set. Do not add weights or
  thresholds until the fitness and filter functions are decoded.
- No constants from this finding enter `acu-rs/` yet.
