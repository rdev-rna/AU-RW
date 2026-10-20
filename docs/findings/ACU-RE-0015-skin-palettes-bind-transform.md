# ACU-RE-0015 — Arno hardware palettes and stored bind transforms

Date: 2026-10-19. Task: ACU-005 / movement Phase 1.
State: partially established; numerical archive evidence, no runtime skinning.

## Provenance

Installed ACU.exe SHA-256
`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, preferred
image base `0x140000000`. No code VA/RVA, native execution or export-to-build
correspondence is established by this finding.

Scope: the eleven existing inspection meshes in
`acu-rs/crates/acu_bevy/src/options.rs::ARNO_PIECES`, all from
`DataPC_ACU_LGS_BelleEpoque.forge`: entry 679 resources
11/15/19/23/27/31/35/39/43, entry 351 resource 1 and entry 655 resource 1.
These remain the FearlessAssassin outfit, head and hood-up hair, not the
unresolved Tailored equipment preset. Original range identities and payload
hashes are in the retained reports under `local/skin-bind-20261019/`.

| Entry / file ID | Stored offset / length | Decoded length |
| --- | --- | --- |
| 679 / `00000023dab179e6` | 217493390 / 1628767 | 2252935 |
| 351 / `000000313db95d16` | 138048084 / 1030415 | 1571601 |
| 655 / `0000002318e0dea8` | 208199341 / 104320 | 165963 |

Stored SHA-256, then decoded resource-frame SHA-256:

- 679: `a17f3e13fbc3ded2486460e34067256ea6435891b990109d23278a38b1b6c680`,
  `a5870df5d63074759342587818aa580e45c3e153b30634ea3fc61f84780bd4bb`.
- 351: `48ee5eee987e224c50021e2c21e2afe0f66efd8f70033236352bf592764649b9`,
  `fe5b1230e78ba6d4b7d356e610ef9956f2758b128de491794e88d585033c23d1`.
- 655: `c0aeb1cc9098fc8398299be2d9116691432b6b590f76080edabcff1d021bee55`,
  `86e93ccd74e98331a913ee91a85f554e9d86a28aca6818018e9247f22ee5fbec`.

Audit report SHA-256:
`9ae634b60f7ac4ed3393f5256a6d826411c969fa53a83caf36fe68071f273526`.
Diagnostic executable SHA-256:
`18d2f4736c88c935f6914f57e8f306a5e66d55cda67e6fda78080d0d46895ad2`.
Source/tool hashes are retained in `tool-identities.json` beside the audit.

Skeleton: DataPC.forge entry 110, male ID `00000006a522da4d`, 109 bones,
payload SHA-256
`28cf74f0bc1ae0ad05504f4c91350914a28c30eb69ee7a51b73f4ca1e8da4ba1`.
Stored/decoded entry ranges, lengths and hashes are in finding
[0014](ACU-RE-0014-humanoid-skeleton.md). The skeleton input is the independent
sequential Python/native-LZO diagnostic `local/skeleton-re-20261019/probe.json`;
this audit does not extend Rust skeleton schema coverage.

Provider: current Rust archive/mesh/vertex readers plus Python standard-library
matrix arithmetic. No REA or IDA session, additional third-party decompilation,
original executable launch or install mutation was needed. `acu-mesh --skin`
reports raw mesh matrices, hardware palettes, decoded joint/weight bytes and
part spans. `tools/skin_audit.py` compares these to the independently decoded
skeleton by exact name hash. File IDs remain 16-digit hexadecimal strings.

## Observations

All 29,481 vertices in 19 parts have 4 or 8 joint slots; every weight-byte sum
is 255. Every nonzero-weight joint fits its part's hardware palette, every
palette entry fits the mesh bone table, and the part spans cover the entire
decoded vertex buffer exactly once. Zero-weight slots are not dereferenced.

| Mesh suffix | Active mesh bones | Matched to male rig |
| --- | ---: | ---: |
| ShoulderCover | 23 | 7 |
| Belt | 15 | 4 |
| Flaps | 17 | 3 |
| Legs | 17 | 11 |
| scarf | 8 | 6 |
| ThrowBlade | 23 | 22 |
| Upperbody | 47 | 34 |
| Upperbody_Down | 10 | 6 |
| Hood_Up | 20 | 6 |
| Head | 116 | 5 |
| HairPlane_HoodUp | 1 | 1 |

Counts are per mesh, not distinct across the assembled character. In total,
105 of 397 active mesh-bone entries match the male rig by hash; 292 remain
explicitly unresolved against that rig. Unused bones in the serialized mesh
table are excluded from this active-bone count.

For each match let `S` be the stored skeleton global transform, using the
Hamilton xyzw convention independently checked in finding 0014, and `B` the
stored mesh bone matrix. The audit normalizes nearly unit quaternions, treats
matrix vectors as columns and compares four explicit hypotheses: column/row
storage, each with `S*B` or `B*S`. For the nine outfit pieces and head, the
column-major `S*B` products are essentially constant within each mesh. Most
are close to the following transform `C` (shown by rows):

```text
 0 -1  0  0
 1  0  0  0
 0  0  1 -0.964
 0  0  0  1
```

Maximum component deviation from the per-mesh mean is at most 1.828e-6
across these ten meshes. The other three hypotheses have deviations of at
least 0.0854 on each of these meshes, making them useful negative controls.

The blade's product has an additional Y translation near 0.000369 native
units. The hair's single matched bone gives a small rotation/translation
difference (Y translation near -0.002578); one match cannot distinguish the
matrix conventions by consistency. No universal transform constant is added
to the runtime. Exact means, deviations and alternative hypotheses are retained
in `audit.json` rather than rounding them into a skinning implementation.

## Inference and limits

The consistent relationship supports interpreting `B` as a mesh-space inverse
bind transform: `B ≈ inverse(S) * C`, for the matched base bones. Correspondingly,
`inverse(C) * S_animated * B` is a candidate skinning transform in mesh space.
This is a hypothesis for controlled deformation testing, not recovered engine
code or a native runtime comparison. Identity deformation constructed from
these same matrices would be circular evidence on its own.

Hash matches support a hardware-index → mesh-bone → skeleton-hash chain on
the sampled parts. They do not prove native additive-skeleton selection, bone
hash generation, modifier execution, root-motion ownership, gameplay axes,
world units or animation semantics. The large unresolved set prevents a
complete animated assembly. Some hashes occur in additive/head diagnostic rigs;
those rigs must be selected and composed through actual asset references before
combining them. Do not fill unmatched joints with the root or an identity matrix.

## Reproduction and falsification

From `acu-rs/`, build `cargo build --locked -p acu_assets --bin acu-mesh`.
From the project root, with `ACU_GAME_DIR` set:

```powershell
$archive = Join-Path $env:ACU_GAME_DIR 'DataPC_ACU_LGS_BelleEpoque.forge'
$dir = 'local/skin-bind-20261019'
New-Item -ItemType Directory -Force $dir | Out-Null
& acu-rs/target/debug/acu-mesh.exe $archive 679 11 15 19 23 27 31 35 39 43 --skin "--out=$dir/outfit.json"
& acu-rs/target/debug/acu-mesh.exe $archive 351 1 --skin "--out=$dir/head.json"
& acu-rs/target/debug/acu-mesh.exe $archive 655 1 --skin "--out=$dir/hair.json"
python tools/skin_audit.py --skeleton-probe local/skeleton-re-20261019/probe.json --mesh-reports "$dir/outfit.json" "$dir/head.json" "$dir/hair.json" --output "$dir/audit.json"
python -m unittest discover -s tools -p test_skin_audit.py
```

Regenerate the independent skeleton diagnostic using the exact command in
finding 0014 if its identified input is unavailable. The report hashes all
diagnostic inputs and retains archive/entry/payload provenance. The analytical
synthetic fixture uses noncommuting rotation and translation to reject wrong
storage/order alternatives; malformed spans, palettes, weights, matrices,
ambiguous names and invalid quaternions are rejected. Unknown hashes are listed,
never silently accepted as resolved skinning.

Next: resolve selected additive skeleton references and their root transforms,
then inspect a bind-pose rig against Arno and perform controlled deformation
before animation playback. No visible renderer change is made by this audit.

Validation: preparation check/doctor/inventory pass; fmt check, workspace clippy
with warnings denied, all 40 authored Rust tests and workspace build pass
(14 install tests ignored by default). The focused original-install audit above
passes; it is distinct from those ignored tests. Six Python tests pass, including
three new analytical/boundary audit tests. Logs: `local/skin-{clippy,tests,build}.log`.
No fresh image is claimed because no presentation code changed.
