# Inferences and presentation choices

| Item | Status / confidence | Reason / validation needed |
| --- | --- | --- |
| Native decoded character space displayed Z-up | Presentation choice; high confidence for this mesh set | Feet/head bounds and coherent static silhouette; native gameplay axes/units not yet bound |
| Draw order paired to instancing order | Bounded inference; high for inspected Arno | Draw counts and each vertex count match; validate palettes/deformation before generalizing |
| UV0 unchanged for diffuse | Bounded observation; high for inspected Arno | Coherent cloth/skin details across three angles; not universal convention |
| PBR roughness, two-sided materials, grey arena lighting | Presentation choices, no native confidence claim | Useful asset inspection; original shader/pass/lighting math not reconstructed |
| File-reference scope 4 | Meaning unknown | Native blade bytes prove a nonboolean byte; original resolver semantics still unknown |
| Fixed-size 48-byte cluster mask is six-direction triangle visibility | INFERRED; high for fixed-size purpose, unverified bit order | 384 bits match 64 triangles x 6 directions in SIGGRAPH2015 pp22-24; Arno variable-size records exceed64 triangles, so their mapping remains open |
| Baked irradiance volumes / Day Paris selection | GI supported by primary retrospective; installed encoding/selection unknown | Ubisoft SIGGRAPH2025 pp5-6 names Unity irradiance volumes and four baked keyframes; recover Paris resources rather than infer from video |
| Hardware palette → mesh bone → skeleton name hash | Bounded numerical support; sampled Arno only | Finding0015: 29,481 vertices have valid palette lookups and 255 weight sums; 105/397 active entries match male rig; additive selection unresolved |
| Mesh bone matrix is inverse bind with mesh/skeleton transform | INFERRED; strong numerical support for matched base bones | Finding0015 compares four storage/order hypotheses; column-major S*B nearly constant per mesh, mostly Z quarter turn / -0.964 Z translation; blade/hair differ; native execution and controlled deformation still required |

No player speed, gravity, jump, probe, collision or state-transition constants
have been introduced as inferred substitutes. Add explicit confidence and source
when an approximation becomes necessary; keep it distinguishable in the UI/docs.
