# ACUFixes / ACU-RE — reviewed external reference

Reviewed 2026-11-14 for the movement slice (see `MOVEMENT_PLAN.md`). This document
evaluates the ACUFixes modding library as a **lead source**. Nothing was imported
from it. Every offset below that is marked *verified* was re-derived against this
project's installed build; everything else is an unverified claim by a third party.
The verified items are recorded as
[ACU-RE-0041](findings/ACU-RE-0041-acufixes-layout-spot-checks.md).

## Identity

| Item | Value |
| --- | --- |
| Repository | <https://github.com/NameTaken3125/ACUFixes> (default branch `master`) |
| Reviewed commit | `e20228a0d775ca18b16d45000847ee0f0aa0f372` (committed 2025-08-24T11:53:34Z; latest on `master` at review time) |
| Reviewed subtree | `CommonLibACU/ACU-RE/` (250 files: ~200 headers in `inc/ACU/`, 25 sources), plus the consumers that hold hook sites: `ACUFixes/src/ParkourDebugging/`, `ACUFixes/src/AnimationTools/`, `ACUFixes/src/Raycasting/`, `ACUFixes-PluginLoader/src/PluginLoader_VariousHooks/` |
| Local review copy | Blobless clone in this session's scratch directory, outside every repository. Not distributed, not built, not executed. |
| Stated target | Root `README.md`: "Addon/fixes for Assassin's Creed Unity 1.5.0"; requirements section repeats "Assassin's Creed Unity 1.5.0". No executable hash, timestamp or build ID appears anywhere in the tree (searched `*.h`, `*.cpp`, `*.md`). |

### License and reuse terms

- The GitHub API reports `license: null` for the repository. There is **no**
  `LICENSE`/`COPYING` file at the root, in `CommonLibACU/`, or in `CommonLibACU/ACU-RE/`.
- The only license files in the tree belong to bundled third-party components:
  `CommonLibACU/DearImGui/inc/imgui/LICENSE.txt`,
  `ACUFixes-PluginLoader/src/external/HwBpLib/LICENSE`, and
  `ProxyOfTheVersionDLL/LICENSE`. They cover those components only; their text was
  not reviewed because none of them is relevant here.
- Consequence: with no license grant, the ACU-RE headers and sources are
  all-rights-reserved by default. **Cite only.** Do not copy headers, struct
  declarations, enum tables, function bodies or comments into this project.
  Individual offsets and addresses are facts about the game binary; record them only
  after re-deriving them from this build, and credit ACUFixes as the lead (as
  ACU-RE-0041 does). ACUFixes' member/function names are the author's recovered
  names unless a CRC-32 match shows otherwise; keep them labelled as such.

## Version match with the installed build

| Item | Installed build (this project) |
| --- | --- |
| `ACU.exe` | `D:/Games/Assassin's Creed Unity/ACU.exe` (via `local/project.json` `game_dir`), SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, 30,907,608 bytes. Same hash as ACU-RE-0031 records for its (different) install path. |
| COFF timestamp | `1423661094` (2015-02-11 UTC) |
| Version resource | **None.** The resource directory holds only types 3 (icon), 14 (group icon) and 24 (manifest); no `VS_VERSIONINFO`, and no `1.5.0`/`FileVersion` strings in the file. The version cannot be read from the executable. |
| Code bytes | Packed on disk (ACU-RE-0031). Code checks use the hash-bound mapped images below. |

Verdict: **consistent at every checked address; build identity inferred, not proven.**
ACUFixes states a version string; the installed file carries none, so the match cannot
be made by version metadata. Instead, eight independent ACUFixes absolute addresses
(vtables, TypeInfo objects, a data table and five function entry points) resolve in
this build's mapped image to exactly the structures ACUFixes says they are (see
ACU-RE-0041). A different build would almost certainly move at least one of them.
Treat any ACUFixes address as a candidate that still needs a byte check here.

Mapped images used (both runtime captures of the installed build; base `0x140000000`,
offset == RVA):

- `local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin`
  (in the main `Unity` checkout), SHA-256
  `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520` — the image
  ACU-RE-0030 already uses.
- `local/native-probe/bone-trace-20261019/main-image.mapped.bin`, SHA-256
  `07afa161c61064659c033827ff8d91b1e3c7a0759a46286e810962f312f02137` — used only to
  compare hook-site prologues.

### Leftover ACUFixes files in the game folder

The game directory still holds an `ACUFixes/` folder (plugins, configs, logs and the
AssetOverrides `Better LOD` mod) from an earlier install the user has since removed.
Nothing loads it: there is no `version.dll` or `ACUFixes-PluginLoader.dll` beside
`ACU.exe`. As a consistency check, the three ACUFixes full-replacement sites
(`0x140133B00`, `0x1401D1CF0`, `0x1401D1E50`) and the neighbouring functions used below
(`0x1401A7000`, `0x1401A2100`, `0x1401A7620`) hold identical, non-jump prologue bytes in
both mapped images. Nothing in the game folder was changed by this review.

## Coverage and mapping to open questions

Status key: **verified** = re-derived against this build (ACU-RE-0041);
**consistent** = structural check only, semantics not tested; **unverified** = claim
not checked. Line numbers refer to the reviewed commit.

| Area | What ACUFixes provides (cited, not copied) | Project question | Status |
| --- | --- | --- | --- |
| Object model | `ManagedObject`/`Object` with vtable slot 5 = get-TypeInfo; static `GetTI()`/`GetVTBL()` addresses on ~60 classes; `SmallArray`, `SharedPtrNew`, `ACUHashmap`; "fancy" virtual tables (`inc/ACU/FancyVTable.h`) — 0x20-byte entries `{nameHash, staticIdx, nestedness, fn@+0x10}` used by parkour actions and human state nodes | Prerequisite for reading any live object | Fancy-vtable entry layout: **verified**. GetTI slot for `Animation`: **verified**. Other `GetTI` addresses: unverified. |
| Entity / components | `BaseEntity` world matrix at +0x20 (position = +0x50), `World*` at +0x60; `Entity` component array at +0x78, size 0x170, component-index bytes at +0x157/+0x15F; `Component::entity` at +0x10; `Entity::GetTI` `0x144E6ABB0` | Player transform/state capture for scenario comparison | unverified |
| Skeleton / bones | `Skeleton` (bones array +0x34, root bone +0x40, IK/COM data), `Bone` (global/local pos+rot, father/mirror, `BoneID` at +0x80 noted as CRC-32 of the name, index +0x88), `SkeletonComponent` vtable `0x142E76630` | Bone pose capture (ACU-RE-0022/0025) | vtable slot 5 is a TypeInfo getter (consistent); layouts unverified |
| Animation resources | `Animation` (vtable `0x142E7ACC0`, TypeInfo `0x1439CD110`, size 0x70; `TeleportAtTime`, event tracks, `AnimTrackData*` +0x40, `Length` +0x48, `AnimationKey` +0x4C, `Additive` bit +0x50, raw tracks +0x60); `AnimTrackData` (size 0x20); ctor `0x140053270`; track-codec descriptor table at `0x145137A40` (72 pointers to 0x30-byte descriptors, decode callback at +0x10, self-index at +0x21) | Codec coverage (ACU-RE-0018, ACU-RE-0040) | `Animation`/`AnimTrackData` storage offsets and `Animation` vtable/TypeInfo: **verified**. Names `Length`/`AnimationKey`: unverified. Codec table: consistent (72/72 descriptors self-index to their slot). |
| Animation graph | ~30 `Atom*` headers (`AtomGraph` root state machine/post-process/error state, RTCP graph variables, state/layering/transition nodes, conditions) with TypeInfo addresses; `AtomAnimComponent` (vtable `0x142E7F780`, graph evaluation +0xD0, skeleton component +0xD8, biped +0xE0, human back-pointer +0xC58, size 0xC70); graph-variable hook sites in `AnimationGraphEvaluationPatches.cpp` (`0x1400B5060`, `0x1400A29A0`, `0x141942CB0`, `0x1401C0ADA`, init reference `0x140123630`); `DumpAnimationGraph.cpp` | **Native animation selection** — the graph is where clip choice and transitions live | unverified; highest-value lead for graph structure |
| Human state machine | `HumanBase::humanStates` at +0x710; `HumanStatesHolder` with `FunctorBase` state nodes (enter/exit at +0xE0/+0xE8, fancy table of 0x357 entries at +0x18); `LoggingTheHumanStates.cpp` (~130 addresses); animation-start / cinematic-time helpers in `MyAnimationPlayer.cpp` (`0x141AC3CA0`, `0x141B1AD30`) | Movement state ownership; which state requests which clip | unverified |
| Parkour / traversal | `AvailableParkourAction` (base ~0x290: anchors src/dest, hand targets, `heightDifference`/`horizontalDifference`/`distance` at +0xC0/+0xC4/+0xC8, min/max ranges +0x1D0–0x1E4, `fitness` +0x204, expected-motion parameters +0x210–0x23C); `EnumParkourAction` (~125 values) and `EnumParkourMode` (7); `ParkourTester` (input direction, stick magnitude); `WhileOnWallSystem` sub-testers (climb, corners, facade, off-wall, overhang, window entry, ladder); selection `0x140133B00`; filters `0x1401D4DE0`/`0x1401D3000`; factories `0x1401CC990`/`0x1401D1FC0`; create-and-test `0x1401D1CF0`/`0x1401D1E50` | **Traversal target selection**; ACU-RE-0030 owner of the scanner | Selection routine, fancy indices for fitness/distance/filters/enum, and the `fitness`/`distance` field offsets: **verified**. Rest unverified. |
| Collision / raycast | `Raycasting.cpp`: ray-query object ctor `0x1425EC5A0`, cast `0x1425FD7D0` (world, origin, direction, length, two flags), result sort `0x1425DF220`; `PhysicComponent` (size 0x50) | Collision queries for the debug view and obstacle sensing | unverified |
| World | `World` (player entity +0xD8, world components +0x118, three clocks +0x7D8/+0x830/+0x888 — one carries the slow-motion timescale, time-of-day manager +0x980); `EntityGroup`, `BuildTable`, `LODSelectorInstance` | Fixed-tick / timescale questions; runtime world ownership | unverified |
| NavMesh / on-disk world formats | **No coverage.** Zero hits for `NavMesh` or `Havok`; nothing decodes `GridCellDataBlock`, `NavMeshManager`, `CollisionMaterial` or other ACU-RE-0034 types | ACU-RE-0034 world/NavMesh leads | none |
| Root motion | **No explicit coverage.** Zero hits for root motion. One note in `inc/ACU/Entity.h` (end of file) says a flag bit (`flags88` bit 58) exempts animations from "bendtime" while movement speed is still scaled — a hint that animation time and displacement are scaled separately | **Root-motion ownership** | unverified lead only |
| Input | `ActionKeyStates`, `InputContainer`, `Enum_ActionKeyCode`, input hook in the plugin loader | ACU-RE-0023 XInput routing | unverified |
| Out of scope | Camera, HUD/menus, sound, missions, gear/inventory, weather, AI reactions | — | not reviewed |

## Spot checks (summary; details in ACU-RE-0041)

1. **Parkour action selection** — agrees. Corpus `140133B00.c` removes candidates
   whose fitness magnitude is ≤ 1e-5, insertion-sorts the rest by
   `fitness × tester weight` (descending), then returns the first index that passes
   both final filters, or -1. The call offsets (`+0x470`, `+0x510`, `+0x9D0`,
   `+0xED0` into the fancy table at object +8) equal ACUFixes' fancy indices
   `0x23`, `0x28`, `0x4E`, `0x76` under the 0x20-byte entry layout. In the mapped image
   the action fancy table `0x143A6DFB0` carries ACUFixes' exact name hash and static
   index at all five checked slots; its fitness getter reads `float [rcx+0x204]`, its
   distance getter reads `float [rcx+0xC8]`, and its enum getter returns 51, the same
   value the factory switch uses to construct that class.
2. **Animation** — agrees. Ctor `0x140054D60` installs vtable `0x142E7ACC0`, initializes
   small arrays at +0x18/+0x24/+0x30/+0x60, clears qwords at +0x40/+0x48 and a bitfield
   byte at +0x50. Vtable slot 5 loads TypeInfo pointer `0x1439CD110`.
3. **AnimTrackData** — agrees. Ctor `0x1400530D0` sets +0 to `-1`, +4 to 0, initializes
   a small array at +6 and clears the pointer at +0x18.

Only `GetDistance` among the checked fancy names is CRC-32-confirmed
(`crc32("GetDistance") = 0x8EB3C7D3`, matching ACUFixes' own `CONFIRMED_NAME` marker).
`GetFitness`, `FinalFilter1/2`, `GetEnumParkourAction` and
`GetDefaultWeightForAction` do not hash to their table values; those are ACUFixes'
descriptive names, not original ones.

## Connection to ACU-RE-0030

ACU-RE-0030 found the scanner caller `0x1401A7000` only as raw pointers in
"non-vtable-like method tables". All 11 such pointers sit in fancy-table entries with
the same name hash `0x518DDA9D`, static index `0x7A` and nestedness 0. The 11 tables
(124–125 sequential entries) and the parkour-action table (120 entries) share slot-0
name hash `0x090A5560`. Inference: `0x1401A7000` is fancy virtual `0x7A` of a family of
parkour-action subclasses that extend the 0x78-entry base table. Its target stores at
+400/+416 fall on the base-class vectors ACUFixes leaves unnamed (+0x190/+0x1A0),
and its +128 store falls on the base vector at +0x80 that subclass ctors also write.
This is a strong ownership lead for ACU-RE-0030, not a semantic result; the class
names and the meaning of index `0x7A` are open.

## What it does not settle

- Native clip choice: ACUFixes describes graph *structure* and how to edit it, not the
  conditions that select locomotion clips for given input.
- Root-motion ownership, world units, axes and tick rate.
- Any on-disk world, collision or NavMesh format.
- Meaning of most `EnumParkourAction` values and of the expected-motion fields; the
  names are the author's interpretations.

## Other repositories by the same author

The user pointed at the author's whole account (<https://github.com/NameTaken3125>).
On 2026-11-14 it held five public, non-fork repositories. The GitHub API reports
`license: null` for every one, and none has a license file of its own (only the vendored
Dear ImGui `LICENSE.txt`). The same **cite-only** rule applies to all of them. Each was
shallow-cloned to scratch, read, and not built or run.

| Repository | Reviewed commit | Stated target | Unique content | Value here |
| --- | --- | --- | --- | --- |
| [ACUFixes](https://github.com/NameTaken3125/ACUFixes) | `e20228a0d775ca18b16d45000847ee0f0aa0f372` (2025-08-24) | ACU 1.5.0 | Reviewed above; the newest `CommonLibACU/ACU-RE`. | Main source. |
| [AssetOverrides-ACUnity](https://github.com/NameTaken3125/AssetOverrides-ACUnity) | `d9df25225dd201c4e2deefa3186743009d0f1c3d` (2025-06-10) | ACU v1.5.0, plugin loader v0.9.0 | A plugin that loads modded `.data` datapacks and loose resource files at runtime by hooking the engine's forge/datapack reader and deserializer. Names ~25 functions, e.g. forge file open `0x142764C90`, datapack prologue skip `0x1426EBBF0`, linked-object descriptor fill `0x1426EBF50`, create object for type hash + handle `0x1426EB3B0`, deserialization stream/cycle `0x14270B510`/`0x1426E0B50`, prerequisite parsing `0x142746FA0`/`0x1427081B0`, CRC-32 string hash `0x142759CF0`, allocators `0x1426F3760`/`0x1426D9A10`. | **High for the asset reader.** Native entry points for the paths our offline reader re-implements: datapack framing, handle → object creation, and load order (ACU-RE-0038 duplicate IDs / patch precedence). Consistency only: corpus `1426EBBF0.c` calls the three prerequisite functions AssetOverrides names (`0x142746FA0`, `0x14276E780`, `0x1427081B0`) and `0x142749E60`; `142759CF0.c` takes a string length and then hashes. That it is CRC-32 is untested. Everything else is unverified. |
| [Halzoid98CPP](https://github.com/NameTaken3125/Halzoid98CPP) | `cdb32cc9b7d7171778ee91ef2cac7192d5abcb29` (2025-07-03) | ACU 1.5.0 | Colorset editor and a 60 fps cloth-physics patch for default outfits (`PhysicsPatch/`, soft-body descriptions); first-person-view experiment. | **Medium.** Implies the stock cloth/soft-body simulation has a fixed step tuned below 60 fps; a lead for cloth and fixed-tick questions in movement Stage 6, not evidence. Colorsets touch material/tint work. |
| [QuickEyeAdaptation-ACUnity](https://github.com/NameTaken3125/QuickEyeAdaptation-ACUnity) | `1b7d56a747e8248118e22924261ebdac4ad16e4a` (2025-08-26) | ACU | Eye-adaptation duration patch; its ACU-RE copy adds `HDRLightingSettings`, `PostEffect`, `TextureBase` and `TextureSelector` headers that ACUFixes master lacks; ships an original Cheat Engine table. | **Medium for rendering.** Runtime layouts of HDR/exposure settings and of the `TextureSelector` type that ACU-RE-0034 names. Unverified. |
| [ACUPluginLoader-ExamplePlugins](https://github.com/NameTaken3125/ACUPluginLoader-ExamplePlugins) | `50198e90f5b8c0356782837b1b70447de588cd44` (2025-06-17) | ACU | A minimal example plugin; a slightly older ACU-RE copy. | Low. Shows the plugin ABI only. |

Every repo except ACUFixes vendors an older snapshot of `CommonLibACU/ACU-RE`. Each also
adds `Enum_ReactionHash_ReactToWhat.h`, which ACUFixes master does not have. Prefer
ACUFixes master for any class it has, because some layouts were corrected later (the
reviewed commit fixes `BaseEntity`'s size).

## Suggested next tasks

1. **Parkour candidate trace (Stage 4 prep).** Build a read-only native-probe profile
   on `0x140133B00` entry/exit that records each candidate's enum (fancy `0x45`),
   fitness (+0x204), distance (+0xC8), height/horizontal differences (+0xC0/+0xC4)
   and the selected index, for one scripted vault approach. Byte-guard the prologue
   bytes above. This gives the movement plan its first native candidate/decision trace.
2. **Resolve ACU-RE-0030 ownership.** Enumerate all fancy tables with slot-0 hash
   `0x090A5560`, map each to its ctor and enum value, and identify which action types
   own `0x1401A7000` at index `0x7A`.
3. **Codec table vs ACU-RE-0040.** Test whether the 72-entry codec table is indexed by
   the track `type` field (codec = type / 3 gives 24 codecs × 3); compare the
   callbacks at slots 9–11, 21–23 and 24–26 with the decoded byte layouts of codecs 3, 7
   and 8. Native confirmation for a finding that currently has none.
4. **Animation graph layout.** Verify `AtomAnimComponent` (+0xD0/+0xD8/+0xC58) and the
   `AtomGraph` root fields against the corpus, as groundwork for native animation
   selection.
5. **Raycast entry point.** Check the cast at `0x1425FD7D0` and its result layout, as a
   candidate probe site for obstacle sensing.
6. **Native datapack path (AssetOverrides leads).** Check the forge-open, datapack
   prologue and object-from-type-hash entry points against `acu_assets` framing. This
   would give the offline reader and patch precedence a native cross-check.
