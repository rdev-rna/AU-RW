# Local developer reference library

Additional user-supplied web references and their verified game scope are indexed
in [the 2026-10-19 review](REFERENCE_LINKS_20261019.md).

Reviewed 2026-10-19. Original PDFs are user-supplied, read-only inputs in
`docs/references/`; authored source bundles exclude PDFs. Pages below are physical
PDF pages. Text extraction is retained under ignored
`local/rigged-world-20261019/`, with per-file SHA-256 in `reference-index.json`.
Diagrams must be inspected before deriving an implementation from their details.

| Local PDF | Pages | Scope and useful entry points |
| --- | ---: | --- |
| `aaltonenhaar_siggraph2015_combined_final_footer_220dpi.pdf` | 60 | GPU-driven rendering; ACU cluster/instance expansion, culling, index compaction and shadow work, pp12–28. Separate the RedLynx portions. See RENDERING_SOURCES.md for already inspected figures and limitations. |
| `Cournoyer_Francois_Massive_Crowd_On_ACU.pdf` | 55 | Unity crowd representation/LOD, pooling and swaps. pp8,14,17 distinguish reduced-bone bulk animation and full NPCs; p35 discusses torso remapping during swaps. These crowd animation shortcuts are not evidence for player clip decoding or player motion. |
| `Blondeau_Christine_DevelopingSystemicCrowd.pdf` | 73 | Systemic crowd behavior and interaction design. Relevant to later ambient population; not a source of player movement constants or shader packing. |
| `REAC_2025_Anvil.pdf` | 84 | Later Anvil rendering architecture. Packages/ownership pp9–17; generated object/shader interfaces p22; jobification p27; code versus data shaders and permutation/PSO pipelines pp28–34; shader hooks/GBuffer extensions pp35–38; binding groups p39; geometry path tradeoffs p51; hardware abstraction and modern ray tracing later. |

The 2025 talk describes a later shared engine, not the installed 2014 game's
binary or file schema. Use its separation of packages, shader bindings,
permutations and render scheduling as architectural context. Recover original
Unity material math, GBuffer formats, pass order and asset fields from identified
Unity artifacts/runtime before claiming a match. Do not transfer modern
micropolygon, ray-tracing or ShaderGen implementation details backward in time.

Current build implications: keep archive decoding, simulation and presentation
separate; retain stable asset identities; classify original skinning/shader
variants explicitly; keep synthetic floor shaders separate from original Arno
material research; compare equivalent captures before measuring parity.

SHA-256 identities, in table order:

```text
6ec37407f9f702423aff79cb920d4b9228d672d4f86d1161cf4c43554c62ed3e
de4412eba89c2cbfc6c8fc16026b4686db0adcb63af35c069362e070e811e1dc
b96d81bd30f06ae6f40ce23e8b6de0f8a6e3c0aef3d530a3ca4545b2045649f9
c0ec61d5a1e72149f7e91f39c1a4411937e0410adff9bc9bc808484625ab95fc
```
