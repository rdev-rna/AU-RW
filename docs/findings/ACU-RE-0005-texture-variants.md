# ACU-RE-0005 — Inline texture dimensions, cube faces and volume mip spans

2026-10-18 / Codex / ACU-005. State: partially established.

## Provenance

Read-only original archive: `D:/Games/Assassin's Creed Unity/DataPC.forge`.
Entry indices are the existing runtime Forge reader's indices. No executable
VA/RVA, native call or corpus correspondence is claimed: this is archive evidence.
Installed executable identity remains the one recorded in ACU-RE-0002.

| Entry | File ID | Archive offset | Stored length | Decoded resource stream length | Payload length |
| --- | --- | ---: | ---: | ---: | ---: |
| 8 | `0000000000000025` | 945464 | 484 | 1188 | 1153 |
| 9 | `0000000000000026` | 945948 | 187 | 544 | 513 |
| 44 | `000000000000008f` | 2407765 | 159 | 418 | 385 |
| 316 | `000000085c61a8e2` | 84763361 | 2305 | 4270 | 4225 |
| 317 | `000000088167d3d9` | 84765666 | 961 | 2231 | 2177 |
| 327 | `00000015cc8b184a` | 84782300 | 112285 | 149956 | 149925 |

SHA-256 identities, ordered as stored entry / decoded resource stream / resource
payload:

```text
8:
9c9e0f8a0990e83a5b7dbddbdc0164e14c7ba8181f0bd39bef4c06f5b2f657d6
64493f9768a7cae96c8ebd3d75d8694b51f47e7bcb75aa4c9aaa0bbb6fc5536f
5a60616386181658c545039357bf92fad463bcb856baa4556ee1485272587018
9:
5e7303edfc02759339d15b715d05aca04a2048693c12064a82a11c6e5a2c6f71
9fa31fef19dc59af58d73d4a08618d2d61ffba83a049be58c02fa4faa822f98e
47225d4e3430bc3a15cfc77f26a76c5e28cba559898fe76c833c1789f9c81a19
44:
75184a6f32b9ed826a28a1d916c1ad8ac476db80a8b4d8f264272383d8f97ec5
f80eda4fb41f2dd2701dbd25c7ae6a4087ef09533f79ee02b337a7f13d6ccb75
7fff1112b1a686587a03921b857376c717f73f7b0bc88aa3befec41cd287d8d9
316:
7e0c265120fbdca6f03e2c5ce1874334352fec3d9ea4a7c7c103bcbf177ac3e9
1cd45e611f62595779351d3c0908e9435a170678200cd17cfe8bac84d680e035
4f24a53c0cf32c3f229a314740889804f64e776e5b00aaf9ce8ea27a3b8e121f
317:
f50275dfb32eac4c4aa5fbc41a02a04bf3a6168af5acb0bd1a3aa09fea7eafb5
f4649dd57c6cedc1dc96c0744e6f1368dd421a827d3b7886b7d0b642edc012ad
531a50335808bd9bca1effc1395e0260a5383088deea6ac1db2fc8d2ef4ec7de
327:
b7cd590b04c3fc226ead7f431d470109c6162fee1c8d1721c17f958b51351dae
3cf8f9fd022947d0ef8506f875da754fd8c97ddddd3c4619922e74af6cf396c7
a143444cee59cae9102537d3fa7f9689f99b1eba134cbd81010488df836ceb03
```

Third-party reference: ATK 1.3.7.0 `AnvilToolkit.dll`, SHA-256
`940374775553172b06d55537c4db78048bc3206b9b2f72bcc61bacfdcdb57636`;
existing REA managed identity evidence
`ev_01f1308aac08023bb993cc29cf977fa03e4a7076b1985e3d9d02ee25a0c7db1a`.
Read retained `local/reference-atk/TextureMap.cs` and
`CompiledTextureMap.cs`, notably `ToDDS` dimension selection and face-major mip
construction (lines 473–575 in this retained decompilation). Independently ran
`ilspycmd -l e "C:/Users/PC/Desktop/ATK/AnvilToolkit.dll"`, then
`ilspycmd -t AnvilToolkit.Utils.TextureFormat` against the same explicit DLL.
Provider: local ilspycmd 8.2.0.7535; enum is `Tex1D=0`, `Tex2D=1`,
`TexCubeMap=2`, `Tex3D=3`. An initial lookup under the Materials.Texture namespace
failed; the enumerated Utils namespace resolved it. No toolkit source copied.

Independent decompression: native `lzo1x_decompress_safe` from
`C:/Users/PC/Desktop/ATK/Libs/lzo.dll`, SHA-256
`6407c266e7a954e58a3ffff0ea4303020dde3bd58997789d662a131bf1c9a7b9`.
An inline Python/ctypes reader read the six archive ranges above, followed the
frame version's u16/u32 block sizes, decompressed every block, checked each output
length and complete entry consumption, and hashed decoded streams and payloads.
The Rust ignored test embeds these independent decoded-stream hashes; the native
DLL is only an oracle, never a runtime dependency. No extracted asset files or
game/sibling modifications were made.

## Result

Observed entry 8: Noise Gradient Texture, root and compiled 256x1x1, kind 0,
engine format 1, zero raw mip count interpreted as one, 1024 pixel bytes.
Entry 9: DefaultCubeTexture, 4x4x1, kind 2, BGRA8, six 64-byte face blocks
(384 bytes total). Entry 44: DefaultVolumeTexture, 4x4x4, kind 3, BGRA8,
256 bytes, all `ff`. Entry 327: Default_3D_LUT_Map, 32x32x32, kind 3,
six BGRA8 mip levels, 149796 bytes. Independently calculated cumulative volume
mip ends are 131072, 147456, 149504, 149760, 149792 and 149796.

Entries 316/317 are 1x1024 root 2D textures whose compiled width is 4. Their
engine formats 8 (BC7) and 2 (BC1), respectively, use 4x4 blocks. Each contains
one mip of 4096/2048 bytes, exactly one block column times 256 block rows. This
establishes a concrete minimum-block padding case; arbitrary root/compiled
dimension disagreements remain errors. The implementation accepts only exact
dimensions or a root dimension below 4 padded to 4 for a known BC format.

Implementation in `acu-rs/crates/acu_assets/src/texture.rs` retains the existing
`Texture2D` public type name for compatibility, adds native `kind`, `depth`,
`layers` and compiled width/height, and exposes every pixel range. Cubes have
six layers with face-major mip ranges; volumes have one range per mip containing
all its depth slices, with depth halved at each level. Cube face orientation and
native XYZ/channel interpretation of volume data remain unverified. Face-major
ordering is supported by the reviewed third-party reader and byte-size checks,
not observed native rendering.

All dimensions remain bounded; zero depth, unsupported kinds, 1D height !=1,
nonsquare cubes, nonvolume depth !=1, wrong compiled dimensions, missing cube
faces, wrong volume mip lengths, truncated prefixes and trailing bytes fail.
Unresolved external top mips and referenced compiled textures still fail
explicitly. Gamma/platform/alignment/size fields are preserved without claiming
their native interpretation or visual parity. No GPU upload or game execution.

## Validation and remaining coverage

From `acu-rs/`: `cargo test --locked -p acu_assets --test texture` passes four
authored tests; one install-dependent test is explicitly ignored in this command.
Set `ACU_GAME_DIR` and run the same command with `-- --ignored --nocapture`:
the six archive vectors pass stored, native-decoded-stream and payload hashes,
dimensions/kinds/mip byte spans and the all-white default volume pixel check.
`cargo clippy --locked -p acu_assets --all-targets` passes.

Selection used the first 4001 entry-report lines from ignored
`local/coverage-complete-audit/entries.jsonl`; this was a bounded diagnostic
sample, not a new complete scan. No updated install-wide typed total is claimed.
The old 39302/65466 TextureMap total predates these parser changes. External mips,
referenced compiled objects and other unsupported cases remain incomplete; this
finding is not full texture-family or all-asset completion.
