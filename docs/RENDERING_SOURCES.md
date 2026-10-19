# Primary rendering references

The user-supplied local PDFs are indexed in
[the reference library](REFERENCE_LIBRARY.md), including the Unity crowd talks
and the 2025 Anvil architecture deck. The latter is later-engine context, not
proof of Unity's installed shader pipeline or modern ray-tracing support.

User-designated architecture ground truth (2026-10-19):
[GPU-Driven Rendering Pipelines, Haar / Aaltonen, SIGGRAPH 2015](https://advances.realtimerendering.com/s2015/aaltonenhaar_siggraph2015_combined_final_footer_220dpi.pdf).
Local reference: `local/rendering-references/aaltonenhaar-siggraph2015.pdf` (60
pages), SHA-256 `6ec37407f9f702423aff79cb920d4b9228d672d4f86d1161cf4c43554c62ed3e`.
References use physical PDF pages: page1 is blank and the title is page2. Browser
text extraction omits that blank page, so its labels are one lower. PyMuPDF1.28.2
in an ignored local venv rendered and verified the relevant figures.

Use the explicitly ACU portion as the architecture baseline, reconcile exact
fields/constants against installed assets/shaders, and document differences.
This combined deck also contains RedLynx material. Do not assign that portion's
claims to ACU; page5's absence of baked lighting refers to RedLynx.

| Pages | Evidence / relevance |
| --- | --- |
| 12 | ACU clusters contain 64 triangles, with indirect indexed draws and generated index data; differs from preceding fixed-strip example |
| 13-21 | CPU/GPU pipeline, instance/chunk/cluster expansion and index compaction; informs findings0011/0012 without proving serialized offsets |
| 22-24 | Six-direction per-cluster visibility masks are baked backface-culling data, not lighting cubemaps |
| 26-28 | Occlusion hierarchy and depth reprojection, including shadow cascades |
| 39 onward | RedLynx section; treat as a separate implementation/reference |

The local variable-size Arno records exceed64 triangles per descriptor. Their
relationship to this fixed-cluster path still needs validation (finding0012).

This establishes neither ACU's full shading math nor baked GI, reflection-probe
placement, time-of-day maps or Paris lighting selection. Continue those questions
in [lighting research](LIGHTING_RESEARCH.md). The video supplies appearance
observations; this developer deck supplies attributed architecture claims;
installed runtime/assets supply build-specific decoding and behavior evidence.

## Primary retrospective confirming baked GI

[Ubisoft Montreal: Ray tracing the world of Assassin's Creed Shadows, SIGGRAPH2025](https://advances.realtimerendering.com/s2025/content/Advances%202025%20-%20Raytracing%20the%20world%20of%20Assassin%27s%20Creed%20Shadows.pdf)
explicitly compares earlier games on physical pages5-6: Unity uses open-world
irradiance volumes, with uniform0.5m probe spacing and four baked time-of-day
keyframes listed for its4km2 world. This is a developer-published historical
baseline, not a verified map of this installed build's data.

Local file `local/rendering-references/ubisoft-raytracing-ac-shadows-2025.pdf`,
141pages, SHA-256
`abca40d37b9e224a261eba9b927372c2e48593254f4fbf56bdef6ae49f8c1c93`.
Pages5-6 were extracted and visually inspected as `shadows-page-05.png` and
`shadows-page-06.png`. A later bulk text print hit Windows cp1252 on a symbol;
it did not invalidate those saved/inspected pages. Only the historical Unity
claims from those pages are used here; do not transfer Shadows' modern GI path.
