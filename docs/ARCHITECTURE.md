# Proposed architecture

This is a starting design, not a claim about the native engine. The runtime
workspace now contains `acu_assets` and the `acu_bevy` inspection app; the user
prioritizes complete direct runtime asset reading. `acu_sim` remains a proposed
responsibility, to add when it owns concrete behavior (fixed ticks, movement).

Start `acu-rs/` with three responsibilities; split further only when needed:

| Proposed crate | Owns | Must not require |
| --- | --- | --- |
| `acu_assets` | Install discovery, Forge index/read/decode, typed resources, provenance | Bevy, a rendering window or modification of the install |
| `acu_sim` | Fixed-tick inputs, movement/traversal states, replayable simulation | Bevy ECS/rendering, wall clock or asset extraction side effects |
| `acu_bevy` | Application, input collection, camera, rendering, animation presentation, diagnostics | Native game executable execution |

Asset flow: owned install → archive index → selected entry with provenance →
bounded decoder → typed CPU resource → Bevy asset. Preserve file IDs through
that flow. Keep container parsing, encryption, compression and resource layouts
separate so each can be compared independently. Runtime loading is the intended
path; extracted files are useful debugging fixtures and not a required install
conversion unless evidence justifies that choice.

Current `acu_assets` modules: `forge` (v27 index/stored reads), `frame` (LZO
framing), `package` (resource envelopes, metadata-only reads), `control`
(ID0x10 global metadata, ID0x91 prefetch), `prefetch`, `texture`, `mesh`
(graph, `MeshData`, typed clustered descriptors/draw primitives and cluster
audits per ACU-RE-0012), `vertex` (recovered attribute layouts and
clustered-buffer decode), `material` (bounded Material/TextureSet/dynamic
parameter decoding, ACU-RE-0013), `skeleton`, `catalog` (runtime lookup over selected
archives; no patch-precedence claim) and `reader`. The coverage scanner and
`acu-mesh`/`acu-probe` CLIs stay report-only and never write to the install.

Current `acu_bevy` modules: `options` (CLI and default Arno piece set),
`load` (per-archive Forge cache, draw-part material and diffuse resolution with
provenance) and `main`
(synthetic grey arena, presentation rotation, camera/orbit, headless offscreen
capture). It reads the installed game directly and never writes to it. Native
axes/units and winding remain unverified. Arno's inspected draw parts bind
materials and diffuse maps, with native mip data uploaded in memory; this does
not implement original material template shaders or animation/skinning.

Input flow: device samples → explicit tick commands → simulation state →
presentation snapshot. Decide whether native root motion or simulation owns
displacement per state; never apply the same displacement twice. Animation
selection and playback time must remain inspectable.

Resolve before implementation depends on them:

- Rust/Bevy pins, Windows linker availability and initial rendering backend.
- ACU container variants, patch precedence, codecs and thread-safety constraints.
- Coordinate axes, handedness, units, quaternion conventions and bind transforms.
- Input sampling/tick policy, collision representation and root-motion ownership.
- Identity/revision policy for caches, resource schemas and replay manifests.

Record durable choices in `docs/DECISIONS.md` using the decision template.
Separate original-format facts from convenient implementation choices.
