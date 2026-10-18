# ACU-RE-0010 — Complete simple skeletons and embedded bones

2026-10-18 / Codex / ACU-005. State: observed simple variant; humanoid
modifiers and solver classes remain unsupported.

## Provenance

Original install archive DataPC.forge, entry 110, file ID `0000000000000800`,
record offset 3342, stored offset 2438262, stored length 48507518.
Stored SHA-256:
`7d5f500049eeb22158005bbd74fb8c22b84b84501d226aa6bfc66c4b0aa2334f`.
Decoded package data length 112976083; SHA-256:
`dc857aaed9c39456ef246fab5231a512583f9143acd587469b73a0e4c9242400`.
Decoded metadata length 208266. Resource identity is retained separately from
the containing package identity. No native VA/RVA claim is made.

The package's two TOC-mode frames were independently decoded in memory with
Python ctypes and `lzo1x_decompress_safe` from
`C:/Users/PC/Desktop/ATK/Libs/lzo.dll`, SHA-256
`6407c266e7a954e58a3ffff0ea4303020dde3bd58997789d662a131bf1c9a7b9`.
All stored-block checksums are checked as zero-seed Adler-32. No extracted
asset files are used. Scripts `local/skeleton_native_child.py` and
`local/skeleton_validate_child.py` retain the experiment; structural results
are in `local/skeleton-native-proof-child.json` and package/resource range
identities in `local/skeleton-candidates-child.json`.

Resource type `24aecb7c` is CRC-32 of ASCII `Skeleton`; embedded Bone class
`95741049` is CRC-32 of ASCII `Bone`. Names and field hypotheses were checked
against read-only AnvilToolkit.dll 1.3.7.0 (identity/provider evidence in
[ACU-RE-0002](ACU-RE-0002-runtime-framing.md)), targeted ilspycmd 8.2.0 output:
`local/reference-atk/Skeleton.cs` ReadOld; `Bone.cs` ReadOld; ObjectPtr, Link,
Reference, FileReference and ScimitarClass. Toolkit Unity branches guide
hypotheses; complete original resource consumption tests establish support.
The toolkit catches certain exceptions internally, so its lack of an exception
is not used as an oracle. No ACU execution or bone-motion observation is claimed.

| Name | Resource ID | Payload range in decoded package | Length | Payload SHA-256 |
| --- | --- | --- | ---: | --- |
| W_WeaponSimple_Root | 00000015bcf63f5c | 36260820..36261033 | 213 | 60538e90ad851840f7dbdc6647d7a8b4ac2128215d28e7d971ceff1789e15864 |
| Weapon_Collapsible | 0000001256216467 | 36320990..36321327 | 337 | 6bd78565ba21d6594ddbce64df6c4de860e0e23e90a9cd8f87b35487be7ee5c3 |
| ACU_BaseSkeleton_Male | 00000006a522da4d | 42822599..42841885 | 19286 | 28cf74f0bc1ae0ad05504f4c91350914a28c30eb69ee7a51b73f4ca1e8da4ba1 |

## Result

The supported complete resource is little endian: byte 1, u64 self ID, u32
Skeleton class hash, u32 skeleton type, u32 bone count. Every bone has byte 0,
u64 self ID, u32 Bone class hash, then these fields:

1. u32 name hash; parent and mirror pointers.
2. Four groups of four float32 values: global position, global quaternion,
   local position, local quaternion. Preserve native component order and values.
3. u8 solving priority, u32 mirroring type, u32 modifier count. This variant
   requires zero modifiers; nonzero counts fail explicitly.
4. u32 dependency count and u32 dependency name hashes; u32 wrinkle category,
   float32 wrinkle factor, u16 index and u16 children count.

Pointer tags 1 and 2 carry u64 IDs (retained as distinct ID/link variants);
tag 3 is null. Tags for inline/managed classes fail explicitly.

After bones: root pointer, four float32 root-local position components,
four float32 root-local quaternion components, body-part file reference,
u32 pose-group count (requires zero), IK pointer, COM pointer, look-at file
reference, reflex-system pointer, u32 skeleton key, u32 hierarchy key.
File references retain u8 tag, u8 global marker and u64 ID. Full payload
consumption is mandatory. All IDs serialize as 16-digit hexadecimal strings.

The native-LZO experiment and Rust reader agree: 13 of 28 Skeleton resources
in this package parse completely. These include W_WeaponSimple_Root (one bone),
Weapon_Collapsible (two), GFX_Bombs_Ballistic_Curve (26), CHR_W_ChainBlade (two),
several weapon addon skeletons, CN_P_Arno_HiddenBlade_Left_A (four), and
GFX_RigidBodies_Rock (two). The other 15 remain unsupported, explicitly failing
on modifiers or pose groups. This is a bounded original-data slice, not a full
installation census or a completed Arno character importer.

W_WeaponSimple_Root bone ID is `00000015bcf63f5d`, name hash 3066451557,
index/children count 0/0, and both skeleton keys 224028746. Its quaternion W
component is 0.9999999403953552; no normalization is applied. Weapon_Collapsible
bone 0 has one child and bone 1 has index 1 and points to bone 0 as parent.

`skeleton::parse(payload, expected_resource_id, limits)` returns a complete typed
Skeleton only for the supported variant. `parse_bone` is composable at the
embedded u64 ID/u32 class header and leaves caller-owned boundaries intact.
The full parser checks resource identity, finite transforms/scalars, count and
input bounds, duplicate bone IDs/indices, self-parent links and internal graph
cycles. Cycle checking is iterative and linear. External parent links in additive
skeletons are preserved unresolved; no external graph validity is asserted.
Allocation limits are defensive choices, not recovered native constants.

## Validation and remaining work

From `acu-rs/`:

```powershell
cargo test --locked -p acu_assets --test skeleton
$env:ACU_GAME_DIR="D:/Games/Assassin's Creed Unity"
cargo test --locked -p acu_assets --test skeleton -- --ignored
cargo clippy --locked -p acu_assets --all-targets
```

All pass. Two authored tests cover scalar/transform values, expected identity,
an internal parent chain, self-parent and multi-bone cycles, unresolved additive
parents, every prefix truncation, trailing bytes, nonfinite transforms,
unsupported modifiers and count limits. The explicit install test matches
native-LZO stored/package/payload hashes and expects exactly 13 full successes
and 15 explicit unsupported results in the original package.

`python local/skeleton_validate_child.py` independently reproduces the 13/15
split through the native LZO provider. ACU_BaseSkeleton_Male declares 109 bones
and first reaches unsupported modifiers at payload offset 2353. ACU_Base
declares 68 bones and reaches modifiers at offset 369. Generic_Head_Male_SKELETON
reaches a nonempty pose-group list at offset 14700. These are next schema
boundaries; their remaining bytes are never counted as parsed. Names, reference
resolution, modifier/pose/IK/reflex behavior, unit/axis conventions, inverse bind
matrices and animation ownership remain unknown. No visual/parity claim applies.
