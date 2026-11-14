# ACU-RE-0060: What does the runtime class reflection name in the corpus?

Date / investigator / related task: 2026-11-14 / Claude (Opus 5.5), offline / symbol index for corpus navigation
State: observed (layout and counts in one capture); names are proposals with stated confidence

## Provenance

Binary path / SHA-256 / image base / VA / RVA: mapped runtime view
`E:/ReverseEngineering/Projects/Unity/local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin`,
SHA-256 `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`, 125,980,672 B, laid out
by RVA (`manifest.json` `layout: main-module-rva`). Disk `ACU.exe`
`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`. Image base `0x140000000`.
Pointers inside the capture are absolute at this base.
Corpus path and lines / export identity status: `ACU - Decompiled/<VA>.c` (217,041 files), listed
by filename only. The corpus binds to this build by headers and resources, not by code
([ACU-RE-0031](ACU-RE-0031-corpus-binary-identity.md)). Before the change, 3,000 random headers
were sampled and all read `// 0x<VA>  sub_<VA>  size=N`.
Archive identity / byte range / range hash / file ID (hex string): not applicable (no Forge data).
Stored and decoded lengths / SHA-256: not applicable.
Tool/provider/version / evidence IDs or saved output: `tools/symbol_index.py` (stdlib only),
`tools/symbol_query.py`, `local/symbol_index.sqlite`. `meta.coverage` holds every count below.
Scratch disassembly used Capstone 5.0.3 only to look around; the builder does not depend on it.
Curated names: `data/symbol_names_findings.json` (SHA-256
`055f85f2d00787071c61e67794ab38f4b89a7ee1e3349b14cad042ab7ecb732f`). It holds 289
entries, read from 54 findings on `origin/feat/parkour-candidate-trace`; every cited line was
checked to contain its VA.
Exact commands and search boundary: `docs/SYMBOL_INDEX.md` "Build". The scan covered `.text`
(getter, lea and E8/E9 patterns), `.rdata` and `.data` (qword vtable slots, pointer globals and
identifier strings) and `.pdata` (function ranges). No process was opened and the game was not
touched. Two builds gave the same table-content digest,
`bc1c4a34133ad4b6fe2c625620c009012b06e489bad11ccf3c3b3920a48431fb`. The digest is SHA-256 over the
sorted `repr` of every row of every table. It includes `meta`, so it changes with the tool or
findings hash.

## Result

Observations:

1. **Getter and vtable mechanism.** The class-name getter is `48 8B 05 rel32 C3`, with the
   class info at `[X]`. A vtable is a qword array whose slot 5 (`+0x28`) is such a getter.
   The scan found 6,577 getters, one class info each. There are 8,966 vtables (6,448 classes;
   50 classes have more than one; 169 have none found). Every one of the 8,966 vtable starts is
   also a `lea r, [rip+X]` target in `.text`. That is consistent with constructors installing
   them, and with slot 5 being the right anchor for the start. Example: Entity `0x143017D60` →
   getter `0x141CD02B0` → class info `0x144E6ABB0` "Entity".
2. **Class-info header**, checked across the whole scan:

   | offset | content | check |
   | --- | --- | --- |
   | `+0x00` | property-descriptor array | 5,335 classes non-null |
   | `+0x08` | nested enum descriptors? | `{hash,value}` pairs seen for AtomAnimComponent and Entity; not decoded |
   | `+0x18` | name C string | |
   | `+0x20` | parent CRC-32 | equals the parent's `+0x24` in 6,454 / 6,454 linked pairs |
   | `+0x24` | own CRC-32 of the name (the Forge type id, e.g. Entity `0984415E`) | `crc32(name)` in 6,516 / 6,617 classes |
   | `+0x28` | object size (u32) | child ≥ parent in 6,454 / 6,454 pairs |
   | `+0x2C` | flags (u32) | meaning unknown |
   | `+0x38` | parent class info | 6,439 pointers; 15 more linked only through the parent CRC |
   | `+0x48` | u32 index? | meaning unknown |
   | `+0x4C` / `+0x4E` | isA interval lo/hi (u16) | nested inside the parent's interval in 6,439 / 6,439; `0x7FFF` (49 classes) appears to mean unassigned |
   | `+0x50` | function pointer (factory?) | meaning unknown |

   Example chains: BipedComponent (size `0x9B0`) → BipedComponentBase → CharacterController →
   PhysicComponent → Component → BaseComponent → ManagedObject → Object. AtomAnimComponent →
   IAnimationComponent. SkeletonComponent → Component.
3. **Property / field metadata exists.** Each descriptor is 0x38 bytes:
   `u32 flags, u32 name CRC-32, u32 type CRC-32, u32 type word (code in the high 16 bits),
   u32 packed, u32 ?, 4 × accessor pointer`.
   - There are 31,910 descriptors in 5,335 classes.
   - No count field has been identified. Arrays share pools, so each array is bounded by the
     next known class-info or array address (2,897) or by the first entry that fails validation
     (2,438). In 5,323 of 5,335 classes, every counted entry also has its own `&props[k]`
     pointer global in `.data`, which independently corroborates the counts (31,898 / 31,910
     entries).
   - **Offset.** When the low 16 bits of `packed` equal the type code, `offset = packed >> 18`.
     This gives 26,526 offsets.
   - Of the one-instruction accessors (`mov/movzx/lea/movss [rcx+disp]; ret`), 555 decode. Of
     those, 548 touch the same offset and 7 differ: AmbMatchmakerComponent ×3, all shifted by
     `+0x78`, plus AmdModule, HUDNotifyPCComponent, ImmunityDataItem and
     InventoryItemContainerSettings.
   - Example: Component `0EA1C978` (type Entity, code `0x14`) is at offset `0x10`, and setter
     `0x141F3C130` is `*(a1+16)=a2`.
   - **Names.** Property names are CRC-32 only. 3,553 (11 %) resolve against identifier strings
     in the image (e.g. `Active`, `LocalPosition`, `GlobalPosition`, `Radius`, `Offset`).
   - In BipedComponent itself the array is null; its parent chain holds the fields.
4. **Coverage of the corpus.**
   - Vtable slots reach 38,674 distinct functions; 38,664 are corpus files.
   - With accessor and propagation names added, 50,280 of 217,041 corpus functions (23.2 %)
     get at least one proposal. Distinct VAs by source/confidence:

     | source / confidence | VAs |
     | --- | --- |
     | `reflection-vtable/high` | 37,492 |
     | `reflection-vtable/medium` | 1,036 |
     | `reflection-vtable/low` | 146 |
     | `reflection-property/medium` | 501 |
     | `reflection-property/low` | 4,096 |
     | `propagation/low` | 7,411 |
     | `capture-finding` (68 live, 205 static, 16 inferred) | 289 |

   - Method rows: 52,439 own, 102,046 inherited, 2,548 new-slot and 2,512 unknown (no parent
     vtable).
   - Call graph: 585,043 E8/E9 candidate edges.
5. **The acceptance query resolves.** `0x141CE1FF0` is `Entity::vf16` (slot `+0x80`), own in
   Entity `0x143017D60` and inherited by EntityGroup and WorldEntityGroup. ACU-RE-0056 names it
   `Entity__set_world_matrix` (live).

Inferences and alternatives:
- `vf<slot>` assumes slot 5 is the class getter in every reflected vtable. All 8,966 starts are
  lea-referenced and slots 0–5 are executable, but an unreflected subclass that inherits the
  getter would be filed under its base class.
- `high` means all users lie on one inheritance line. `medium` usually means the linker folded
  identical code. `propagation` relies on candidate edges: 6,936 of the 7,411 helpers have
  exactly one caller.
- In the 10 pool cases where another class's array pointer lands on this array's last slot,
  the shorter count was kept.

Unknowns / incomplete coverage:
- Slot numbers are not method names.
- Non-reflected classes (no getter at slot 5) and free functions not called only from one
  class remain unnamed: 166,761 corpus functions have no proposal.
- 101 classes fail the name CRC (99 of them reached by a getter; e.g. `BhvHorse`,
  `AssassinateEvent`). The hashed name may differ from the string.
- 163 classes have no parent link.
- The meanings of `+0x08`, `+0x2C`, `+0x48`, `+0x50` and the descriptor `flags` are unknown.
- The `0x60C0` type-code descriptors (475 in 9 classes, e.g. Entity) use a different `packed`
  layout and get no offset.
- 15 offsets are at or beyond the class size, mostly embedded-struct code `0x16`.
- The vtable end is heuristic (no RTTI), so `new-slot` counts depend on it.
- E8/E9 scanning is linear, so some edges come from bytes inside other instructions.
- Everything here comes from one capture. Runtime-only state, such as the 49 unassigned isA
  intervals and 15 CRC-only parents, may differ in other runs.

Independent comparison / falsification test:
- **Reflection vs findings.** 39 curated finding names overlap reflection names, and all agree
  in kind and class. The eight pose-modifier `apply` entries from ACU-RE-0028 are all `vf11`
  of the expected classes: BallJoint (coat), HingeVector, MaxLookAt, OrientationConstraint,
  PositionConstraint, Reflex, SpringBox, and Wrinkle (vtable `0x142e77e20`, which was
  unported). This resolves 0028's "Hinge?" to `HingeVectorBoneModifier`. The ACU-RE-0056
  world-matrix writers are `Entity::vf16` / `BaseEntity::vf16`. The physics transform sync
  is `CharacterController::vf120`.
- **CRC, isA and size checks** in observation 2 are independent of the getter scan.
- **Unit tests.** `tools/test_symbol_index.py` uses a synthetic mapped PE. It covers malformed
  getters (target outside the image, `[X]` outside the image, a non-ASCII name), a vtable with
  a non-code slot, out-of-range VA reads, a chained `.pdata` fragment, accessor decoding
  (including the r9 base and a missing `ret`), and the CLI end to end.

Reproduction steps: run the `docs/SYMBOL_INDEX.md` build command, then
`python tools/symbol_query.py info`. To run the unit tests:
`cd tools; python -m unittest test_symbol_index`.

Implementation implications and finding-linked constants:
- This is a navigation aid, not evidence of behavior.
- Field offsets marked `match` are the strongest layout observations. Cite the table and this
  finding when using them in `acu-rs`.
- Constants: getter pattern `48 8B 05 rel32 C3`; class info `+0x18` name, `+0x20`/`+0x24` CRCs,
  `+0x28` size, `+0x38` parent, `+0x4C`/`+0x4E` isA; descriptor stride `0x38`,
  `offset = packed >> 18`.
