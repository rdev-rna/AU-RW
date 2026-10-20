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
