# ACU code and asset map

Initial mission index, 2026-10-19. Installed executable identity:
SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`,
preferred base `0x140000000`. Export filenames suggest VAs; their correspondence
to the installed binary remains unverified (ACU-003). Never treat a string hit
as proof of a movement implementation or live address.

| Subsystem / seed | Location | Established meaning / limitation |
| --- | --- | --- |
| Parkour string | `ACU - Decompiled/140141F90.c:5`, candidate VA `0x140141F90`, RVA `0x141F90` | Calls string construction with `Parkour`; only a discovery seed, no mechanic recovered |
| Skeleton / Bone records | DataPC.forge entry110, Skeleton `24aecb7c`, Bone `95741049` | Typed simple variant; native-LZO proof and exact resource ranges in finding0010; humanoid modifiers/pose groups still open |
| Draw parts / palettes | `acu-rs/crates/acu_assets/src/mesh.rs`, Mesh `415d9568` | Compiled instancing contains hardware-to-mesh bone indices; semantic validation pending |
| Vertex attributes | `acu-rs/crates/acu_assets/src/vertex.rs` | Packed positions, normals, UV0 and 4/8 joint slots, finding0011; bone mapping not established |
| Materials / textures | Material `85c817c3`, TextureSet `d70e6670`, TextureMap `a2b7e917` | Selected Arno diffuse chain and scope-byte provenance in finding0013; native shader math unknown |
| Animation / selection | Native function/data locations unresolved | No codec or original blend/state/selection behavior claimed |
| Player / input / collision / camera | Native function/data locations unresolved | Current orbit camera is presentation code only |
| Climbing / ledges / vaults | Native function/data locations unresolved | Parkour name is not implementation evidence |

Read-only reference provider: AnvilToolkit.dll 1.3.7.0 SHA-256
`940374775553172b06d55537c4db78048bc3206b9b2f72bcc61bacfdcdb57636`, ilspycmd
8.2.0.7535. Layout hypotheses live under ignored local/reference-atk*. Native
byte consumption and independent numeric checks are required before support.
The For Honor sibling is a separate game/build; use only as an explicitly
identified reference, not as a native Unity symbol source.
