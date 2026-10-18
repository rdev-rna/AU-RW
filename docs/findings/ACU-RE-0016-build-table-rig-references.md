# ACU-RE-0016 — BuildTable rig references

Date: 2026-10-19. Task: ACU-005 / movement skeleton stage.
State: partially established; serialized references and head-rig inputs decoded.

## Provenance and scope

Archive-byte investigation for installed ACU.exe SHA-256
`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`,
preferred image base `0x140000000`. No native VA/RVA or export correspondence
claim. Target: BelleEpoque entries 334 (Arno avatar) and 679 (current Fearless
outfit), with selected head resources traced separately.

Third-party layout corroboration: AnvilToolkit.dll 1.3.7.0, SHA-256
`940374775553172b06d55537c4db78048bc3206b9b2f72bcc61bacfdcdb57636`,
ilspycmd 8.2.0.7535. Reviewed Unity branches of BuildTable, BuildColumn,
BuildRow, PropertyPath, BuildTag(s), RowSelector, DynamicProperty,
PropertyRegistry, Reference, ObjectPtr and Handle. Decompiled reference files
remain ignored under `local/rig-references-20261019/`; none are distributed.
Provider: existing Rust archive reader and independent Python byte cursor;
no native analysis session or game installation mutation.

## Observed layout / bounded implementation

The selected tables begin with a resource marker, ID and class `22ecbe63`.
Columns and rows carry class headers, counts and dynamic properties. Reference
properties have type `001c0000`, a data-type hash, an additional u32 and a
ten-byte tag/scope/ID reference. Preserve data-type hashes rather than assigning
semantics from byte matches. Resolve column numbers through their definitions.

The diagnostic supports only sequentially decoded reference/handle tables
with empty PropertyPaths and position-selection lists. Other variants must fail
explicitly, including unsupported object-pointer tags, property kinds, and trailing
bytes. It must retain selected row tags, defaults and subtables rather than
pretending that serialized references prove runtime selection or composition.

Acceptance: decode the selected original tables to exact payload end; preserve
payload identities and reference offsets; trace typed skeleton references to
identified rigs; test truncation, counts, classes and unsupported variants.
This slice does not claim additive root transforms, skinning or animation.

The head row referencing the existing head package `000000313db95d16` also
references Skeleton `00000012ff1479d5` (`CN_U_Arno_Head_Rig`), BelleEpoque
entry 334 resource 66, payload `[822961,920084)`, length 97123, SHA-256
`1c3f7c7919d641f7fcf0fdfcd5814a28b53f17a03446dbbaafc7548e1dcbe341`.
An independent sequential Python/native-LZO diagnostic reaches the exact end:
134 bones and 106 `d12b891d` modifiers. AnvilToolkit identifies this class as
WrinkleBoneModifier: after the common modifier prefix, a counted ObjectPtr
neighbor list, center ObjectPtr, three f32 lengths and a boolean flip.
This supports a typed decoder extension; modifier evaluation is not established.

The original-install Rust test then reached the existing pose-group rejection.
The independent cursor identifies one `c8f8abff` group, 147 `209cf072` poses,
and 2526 `071cee50` pose-bone records. Layout: group header and pose count;
each pose has a header, u32 pose ID and bone count; each bone has a header,
u32 bone ID, three i16 position fields and three i16 rotation fields.
These packed values are preserved without assuming unpack scales or evaluation.
The third-party SkeletonPose/SkeletonPoseBone Unity branches corroborate the
storage widths; no animation interpolation or compression semantics are claimed.

The existing skin audit matches 110/116 active head mesh bones to this rig
(versus 5/116 to male alone). Column-major `S*B` max deviation is 3.213e-6;
the three alternatives exceed 1.07. Six bones remain unresolved against this
rig alone. This numerical result does not establish composed runtime skinning.

## Reference graph and remaining questions

VisualMaster row 0 links Heads (`00000026a4c11760`), Body_All
(`000000145368098e`) and another table (`00000012ca6b42bf`, unresolved here).
Body_All row 0 links Skeleton (`0000001453680995`) and Body_Custom
(`0000002dfa9960d6`). The skeleton table's row 0 explicitly carries:

| Column | Skeleton ID | Resolved resource name |
| --- | --- | --- |
| 2 | `00000005a1017f95` | ACU_Unisex_ArmsSimple |
| 3 | `00000014b28a3e45` | ACU_Arnaud_Armpits |
| 4 | `0000000fffabbc8e` | ACU_Unisex_LegsRollbones |
| 5 | `00000017e3c5102f` | ACU_Unisex_Dress_Simple_HV_Col |
| 7 | `00000014556880aa` | ACU_U_Arno_IconicWeapon |
| 8 | `000000070d1bd4fc` | ACU_Unisex_Pecks |
| 9 | `0000000c94a16e93` | ACU_Arnaud_Pistols |
| 10 | `00000014f694d496` | ACU_Avatar_ForeArmTwist |
| 11 | `0000000fb9ce2b35` | ACU_Male_ArmsFlat_Upper |
| 12 | `0000000034a01f8e` | Addon_Human_Sword_Tag |
| 13 | `0000001240bfbded` | Addon_Human_PlayerWeapon |

All eleven column passes are stored as 5; pass execution order/meaning remains
unverified. Resource names are from the existing typed scan, not name guesses.
The scan contains 1169 occurrences of these eleven IDs across packages; it is
not a patch precedence resolver. Six candidates occur in avatar entry 334
(resources 116–121). The other IDs have candidates in DataPC entries 110,
968 and 1006. Resolve actual payload identities before composing these rigs.

The Heads row tagged `50f833f0` associates the current head package and the
specific rig above. It also carries other visual references. Neither that tag's
human-readable meaning nor default/native row selection is established.
Its rig is different from Generic_Head_Male_SKELETON (`0000000d497adb6a`).
Only two of the six remaining head hashes also occur in the male rig; do not
claim that combining these two rigs resolves the head.

Hoods, UpperBodies, Belts and Pants each decode 43 tagged rows. Their tag-to-
equipment-name mapping remains unknown, so this does not select Tailored gear.
The Hood_Position table (resource 128) deliberately fails at byte 535 on a
nonempty PropertyPath. No complete BuildTable family support is claimed, and
this diagnostic does not alter the Rust coverage scanner's typed counts.

## Byte identities

BelleEpoque entry 334: stored offset 129748603, length 497250, ID
`000000070cf6709b`, stored SHA-256
`42935733c7319be475e358525ef99582a31dbf9b375825c8802244b9d4229137`;
decoded resource frame length 1760809, SHA-256
`91533887418e19f6aba4d002ab73203df1dd75a0877067703e89751c5abfb6eb`.
Entry 679 identities are unchanged from finding 0015.

| Entry/resource | Table | Payload bytes | Payload SHA-256 |
| --- | --- | ---: | --- |
| 334/3 | VisualMaster | 4469 | `1f83d9b51a2c863d47ae9910eb5ad36d75532b848fde7249dc5012d351b5b7f5` |
| 334/12 | Heads | 1399 | `fe0030d571d8a00a81bf27b6b6e723ebbb929c1f0c2bb500fbe2d9b4d359cd68` |
| 334/13 | Body_All | 4282 | `8d379fffef8a3ae3f8725bcdcbca55e433c124ffb335885b3d20884e9b366ea7` |
| 334/71 | Skeleton | 1325 | `130e473cc756ea6d7b77ff8464971c1b31727fc78761e2b6c478ba5b9fecaa73` |
| 334/72 | Body_Custom | 4817 | `e703a7a03c91ed34be8f72541c5d24ea6e6f1b66484f6e691213baa8ca03b842` |
| 334/122 | Hoods | 8252 | `dcf14a345863b643478ab1aaabeedef1837a1e72c5ddf2809a98298fa447835b` |
| 334/123 | UpperBodies | 8548 | `bb80cefe1d9af92b9fdac74aa5b12964f999be72521496f89bcb3f3fa96a14ee` |
| 334/124 | Belts | 6238 | `5c40b98de04faeae56d9c9c5129739cfef548049eedd48e4f16d258e843af7aa` |
| 334/125 | Pants | 4428 | `337dafc2679505be53401fcda04f7f8752f933fcc2ad8437db18efb299fd67db` |
| 679/0 | Fearless_Body | 1218 | `30e87ff6d6a2661c4f6f60eed1517e1489fb025ddda763bb2078dac0d6d8b2bb` |

Exact table payload ranges/IDs, reference offsets, source-report hashes and
decoded row data are retained in `local/rig-references-20261019/*parsed.json`.
The head diagnostic is `head-probe.json`; numeric comparison is
`head-skin-audit.json`. The independent cursor/native-LZO diagnostic extends
the prior finding 0014 probe with the wrinkle layout above; its script and
third-party layout excerpts are retained locally for audit.

## Reproduction and validation

With the configured game directory in `ACU_GAME_DIR`, run from the project root:

```powershell
$archive = Join-Path $env:ACU_GAME_DIR 'DataPC_ACU_LGS_BelleEpoque.forge'
& acu-rs/target/debug/acu-mat.exe $archive 334 71 --hex --out=local/table.json
python tools/build_table_probe.py local/table.json --output local/table-parsed.json
python -m unittest discover -s tools -p test_build_table_probe.py
```

Repeat with the entry/resource pairs in the table. All ten reach exact payload
end with their recorded hashes. Four authored Python tests cover reference and
handle values, 64-bit identities, scope 4, every truncated prefix, trailing
bytes, wrong classes/counts/booleans, nonempty paths, unsupported property kinds,
undefined columns and report identity mismatches. Rust's authored wrinkle vector
checks values, every truncated prefix, excessive counts, NaN and invalid booleans.
The install test is explicit:

```powershell
cd acu-rs
cargo test --locked -p acu_assets --test skeleton original_arno_head_rig -- --ignored --nocapture
```

It checks original stored/payload hashes, 134 bones, 106 typed wrinkle inputs,
one group / 147 poses / 2,526 packed bone records and an independent first-record
vector. The bootstrap install test also passes 20/28 rigs, now including
Generic_Head_Male_SKELETON (previously rejected on pose groups); eight failures
remain explicit. The authored pose vector tests signed i16 extremes, multiple
poses, exact consumption, every truncated prefix, each count and class boundary.
The first install run failed at the old pose-group rejection and prompted this
extension; only the final rerun establishes support.

Final validation passes: Cargo fmt, clippy with `-D warnings`, all 42 authored
workspace tests (15 install-dependent tests ignored), and workspace build.
The two explicit skeleton install tests pass separately. Workflow checks and
all 14 Python tests pass in the research and source-only checkouts. Final logs:
`local/rig-references-20261019/{clippy-final,workspace-tests-final,skeleton-install-final,build-final}.log`.

No solver or renderer code changes; no new
visible scene, deformation, animation or native execution parity is claimed.
