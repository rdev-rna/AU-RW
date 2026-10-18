# Shading comparison deltas

No matched original/rewrite camera/pose/lighting comparison exists yet. Current
static Arno rendering validates diffuse binding, not shading parity.

Default target: **Day, Paris**. See [lighting research](LIGHTING_RESEARCH.md)
for the user's video reference, access status and baked-lighting/GI/cubemap
questions. Current scene lighting remains synthetic.

| Area | Current behavior | Outstanding evidence |
| --- | --- | --- |
| Diffuse / alpha | Native mip chains, sRGB presentation, serialized alpha-test flags | Original sampler/color-space and template shader paths |
| Surface response | Bevy PBR roughness0.85; no native normal/specular/mask layers | Texture packing, original BRDF and material math |
| Lighting / shadows | Synthetic directional and ambient lighting | Native lighting buffers, shadow passes and scene conditions |
| Skinning / procedural layers | Static decoded pose | Bone palette/bind transforms, original skinning shader and animation |
| Post / fog / grading | Bevy presentation defaults | Native pass order, precision and transfer functions |

Per-pixel comparisons must identify original game/mod configuration, both source
and executable hashes, fixed pose, camera, lights, color space and image outputs.
Do not report a numerical parity score from unmatched synthetic captures.
