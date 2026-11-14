# Synthetic test-world movement

Scope: a controllable rig-validation build, not recovered native locomotion.
Original animation codecs, root-motion ownership, traversal and material shaders
remain separate research gates. Use original archive mesh weights and bind data;
do not replace missing influences or silently truncate eight-slot weights.

Controller priority (user-directed, 2026-10-19): left stick is analog,
camera-relative movement. Hold RT/R2 for high profile / freerunning intent;
hold it with PlayStation X (South, Xbox A) to prioritize up, or Circle (East,
Xbox B) to prioritize down. Releasing the trigger immediately clears both
priorities. X means the physical South button, not Xbox's West/X button.
Right stick controls camera yaw and pitch, non-inverted: right looks right,
up looks up. There is no camera recentering that overrides the right stick.

The test approximation runs in high profile, launches toward an obstructing
block on upward intent, and suppresses that launch on downward intent so a
lower route or running off an edge can descend under gravity. Native vault,
climb, ledge selection, safe-drop rules and traversal animations are unresolved.
Both face buttons together choose Down as an explicit test safety choice;
this conflict rule is not established for Unity. Keyboard equivalents are
Shift, Shift+Space and Shift+C. Space alone retains the test jump.

Chosen tuning: radial stick deadzone 0.16 rescaled to full range; RT threshold
0.25; camera yaw/pitch speeds 2.4/1.8 radians per second with clamped pitch.
Use one stable connected pad (lowest entity identity on initial selection),
clear input on disconnect or focus loss, and resample held state before fixed
ticks. Headless replays ignore physical devices. These constants are authored.

Implementation choices: 60 fixed ticks/second; world Y up; mesh face toward +Z
at zero yaw (verified from the front capture); WASD camera-relative
movement. Original navigation clips supply walking/sprinting root displacement
(roughly 2.46 / 7.07 source units per second in the forward walk/sprint
loops; lateral walk/sprint averages roughly 1.25 / 6.63).
Analog magnitude scales clip time and displacement together. The standalone
synthetic simulation API retains its earlier speed/acceleration constants for
tests; the viewer uses the clip-root API. Gravity is 18 and jump speed 5.5. Character contact
uses a radius 0.28 horizontal circle against explicit synthetic AABBs, bounded
arena and a ground plane. Downward crossings land on block tops. Sliding resolves each horizontal axis in turn; no
claim of native capsule, stairs, slopes or traversal. R resets position/velocity.
The simulation is independent of Bevy; presentation interpolates fixed states.
Cross-platform bitwise float determinism is unverified. No randomness is used.

The pose driver decodes original idle/walk/sprint navigation tracks directly
from DataPC.forge and applies matching joint hashes in local rig space. The
imported FH pose and trajectory objectives select phases on profile or direction
changes and at 10 Hz; a 160 ms local-transform blend preserves hierarchy lengths.
Eight original takes include lateral/backward walk and lateral sprint. Facing
turns independently at authored 360/540 degrees/s for walk/sprint. The initial
facing is retained when movement starts, then approaches requested heading over
multiple fixed ticks. The 2026-10-20 visible review found the earlier 90/135
degrees/s caps too slow; the new caps give a 90-degree turn in 0.25/0.167 seconds.
These are responsiveness settings, not recovered Unity turn rates. Residual direction warping retains
the selected root distance while following analog travel. Continuing a take
preserves phase. See finding0019. These are adapter choices, not recovered Unity
graph behavior; see finding0018. Actor ownership remains unknown. Native modifier
inputs are decoded but their solvers are not executed. Bind pose and
controlled single-joint deformation are required validation cases, followed by
walk, run, turn, stop, jump, obstacle contact and reset input replays.

Lighting is a synthetic daylight rig with shadows and an explicit PBR response.
It does not claim recovered Day, Paris lighting, baked GI or shader parity.
Every visible result requires fresh captures from the built executable with
source/binary hashes, arguments, ticks and original asset identities.

## Paris collision block (2026-11-15)

`--block FILE` replaces the grey floor and cubes with a block of decoded Paris
collision, and the rigged Arno traverses it. The block is exported by
`python tools/world_collision.py --archive DataPC_ACU_Paris.forge block X Y --half H --write local/blocks/NAME.bin`
(`ACUBLK01`: f64 origin, then f32[9] origin-relative native triangles and an
instance index; a JSON sidecar lists every instance's body, shape, name and source
archive entry, and a `.samples.txt` grid of Python ground heights). It holds the
MeshShape triangles of ShadowRigidBody and Entity RigidBody placements overlapping
the box (ACU-RE-0050), with identical (shape, matrix) placements kept once.
Primitive shapes and the navmesh are not decoded and are absent. The renderer draws
the collision triangles themselves (warm = walkable top, grey = steep), not the
render meshes, and the ACU-RE-0055 ledge edges near the player in yellow with
their outward normal in orange.

Reference block: `local/blocks/paris-arrival-40m.bin`, centre (768.55, -46.653)
(the Paris arrival point of ACU-RE-0049), half extent 20 m, origin z -0.45267
(ACU-RE-0050 support under probe -0.456), 84,288 triangles from 1,284
placements, SHA-256 `802bdf4549562f8f3e3084236384af03541df8483e3cf46cd36b506e3f27d8f6`,
index 2026-10-22.1.

What is decoded and what is authored (`acu_sim::block`, `acu_sim::traverse`):

- Decoded: triangles and placement (ACU-RE-0050); ground = highest surface at or
  below the probe plus a step (ACU-RE-0050 rule); ledge edges = convex triangle
  edges with a walkable top (|n.y| >= 0.7) and a steep side (|n.y| <= 0.3)
  (ACU-RE-0055 recipe); fall acceleration 9.55 m/s² (ACU-RE-0059).
- Authored, not native: the body (three 0.28 m spheres at 0.73/1.12/1.52 m),
  0.45 m step, axis sliding, jump speed 4 m/s, RT + up into a wall mantles a ledge
  0.5-2.4 m above the feet within 0.7 m or else starts a free wall climb
  (1.4 m/s up, 0.9 m/s sideways while RT is held), a climb tops out when a ledge
  is within 1.9 m, Circle/B lets go, and a mantle is a 0.35 s + 0.2 s/m
  interpolation onto the ledge top. No native candidate selection, reach,
  timing, climb clip or warp is used; the locomotion matcher idles while
  climbing or mantling.
- Coordinates: native (x, y, z) -> arena (x, z, -y), the renderer's presentation
  rotation. Falling 5 m below the block respawns at the spawn point.

Checks: `cargo test -p acu_sim` (synthetic boxes: wall stop, mantle vs Down,
6 m climb and top-out, let go and land, walk off a roof, deterministic replay,
parser rejects bad magic/length/count/NaN). Install-dependent, with
`ACU_BLOCK=<absolute path to the .bin>`:
`cargo test -p acu_sim -- --ignored ground_matches paris_arrival` compares the
Rust ground query with the exporter's Python query on 81 samples (76 surfaces,
5 empty; worst 0.05 mm) and replays the scripted climb (three mantles, a facade
climb, top-out at 4.25 m, identical on rerun).

Run (window): `acu_bevy --game-dir <install> --block ../local/blocks/paris-arrival-40m.bin`.
Scripted: add `--spawn 12,12 --scenario block-climb --ticks 300` (RT + up held
toward -Z), with `--camera-distance M --camera-pitch DEG` for framing.
