# ACU-RE-0019 ? Directional navigation requests and original lateral takes

Date: 2026-10-19. Related task: ACU-008. State: partially established.

## Provenance

Asset-only observation; no new executable/corpus address or native execution claim.
Same DataPC.forge entry 928 as finding0018: stored offset 101271720,
length 123303092, SHA-256
`08b0a18bd2fa603ae2b77f2b17587c4f41c7154050552b5874edcd8c2bceda00`.
Decoded resource frame length 206993148; metadata frame length 116160.
Resource-frame SHA-256
`e33c8441ffc8b8189391b4078e212716e90bbcccc196230070f36328c1f927fd`.
Offsets below are decoded package byte ranges, exclusive end. Decoder:
`acu_assets::animation`, via `acu-mat DataPC.forge 928 INDEX --out=PATH`.
Diagnostics: `local/directional-matcher-20261019/clip-INDEX.json`.
Discovery: bounded name queries in the reusable animation SQLite database,
entry 928 only; names are hints, not proof of Arno ownership.

| Resource | File ID | Name | Payload range | Length | Payload SHA-256 | Duration | Root end XY |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 3087 | `000000137c558075` | ACU_NPC_NAV_WalkFwd_StrafeRight | 97738856..97748705 | 9849 | `6704115901735c90c668168bfc0709bd97fcbd4b486457c28c6af04f5d2f3638` | 1.200000 | (1.496247, 0.000000) |
| 3088 | `000000137c55824d` | ACU_NPC_NAV_WalkBck_StrafeBack | 97748863..97760428 | 11565 | `57a2c16c00bc1d2c74488c217324d64a5ca0914f1af0f6a2f1aaead62af8d0ea` | 1.166667 | (-0.000000, -1.305590) |
| 3089 | `000000137c55802d` | ACU_NPC_NAV_WalkFwd_StrafeLeft | 97760586..97770154 | 9568 | `319e54e9422e9eed335b25744d4a35c2b3a252e27a5414b0339fe90a254ed816` | 1.133333 | (-1.423850, 0.000000) |
| 3138 | `0000000f9f512b90` | xx_Nav_Sprint_3-Loop_BankRight_Light | 98906499..98921330 | 14831 | `8b8d591066139e4d2918f5154d6219dd0e82c83fcee439a4838059966a992e02` | 1.400000 | (0.000003, 9.896491) |
| 3141 | `0000000f9f512b7f` | xx_Nav_Sprint_3-Loop_BankLeft_Light | 98955471..98970295 | 14824 | `bc101662fd8e8f9edef02f677e15fc781724f9c92e7e64b36720ae92a8a0834b` | 1.400000 | (0.000003, 9.896010) |
| 4246 | `0000000fed5e3950` | xx_Nav_SprintFwd_SwayRt90 | 125036383..125051309 | 14926 | `ee8b9e63ed541abc54d55546c1a084aaf766bfe91d2c3bd27810827fb273e9e6` | 1.400000 | (9.277452, -0.000003) |
| 4247 | `000000080594a3c6` | xx_Nav_SprintFwd_SwayLt90 | 125051462..125066388 | 14926 | `fbeb4b934a132a3578b968ca41d1a1ba0c4b9ac4eeb98cdaf58979ae01679b52` | 1.400000 | (-9.277286, 0.000003) |
| 5655 | `0000000805948331` | xx_Nav_WalkFwd_StrafeLeft90 | 150934528..150942822 | 8294 | `62cf47e85f74a2cc1491e1f59cde3964290c08fab4b21d3d0a164179a8345ee2` | 0.933333 | (-2.180315, 0.000001) |

## Result

Observed: sprint sway clips have opposite lateral root displacement and identity
navigation-root rotation, despite containing SprintFwd in their names. Walk
strafe and backward takes likewise have lateral/backward root channels. Bank
clips have forward root travel, so names alone cannot select a lateral trajectory.
Pose root-bone rotation is separate from navigation-root rotation; retain the
skeleton root placement correction from finding0018.

Code inspection: the original Unity adapter loaded only three takes and searched
only on profile changes. Its player adapter discarded root direction, while the
simulation instantly faced the input. Thus changing direction within Sprint
could never select a strafe take.

Read-only reference: sibling `fh_bevy/src/install_loco.rs`, test
`continuity_preserves_eight_direction_steering`, builds request velocity relative
to current facing and integrates facing separately. Imported
`amo_prediction::prediction_geometry_cost` provides trajectory position/velocity
costs, separate from primary joint-pose continuity. FH native addresses/settings
remain FH evidence only; see FOR_HONOR_MOTION_REUSE.md.

Implementation choices: cache Unity phase features at 30 Hz; evaluate original
root trajectories at 0.05/0.5/1.1 seconds with loop extension; request direction
in actor-local right/forward coordinates. Search on profile/direction changes and
at 10 Hz, retain incumbent phase, penalize switching, blend local pose for 160 ms.
Facing turns gradually toward travel; residual direction warping keeps controls
responsive. In the 2026-10-19 user visual-review follow-up, the viewer's turn-in
looked too sharp. The initial idle-to-movement heading snap was removed and the
authored facing limits were reduced to 90 degrees/s for walk and 135 degrees/s
for sprint. The 180-tick synthetic `turn` replay now changes heading over fixed
ticks; see `local/captures/acu-gradual-turn-20261019/turn.png` and
`local/captures/acu-arno-rigged-20261019/scene-turn-180-145.json` (machine-local).
This is user-directed presentation tuning, not a native measured turn rate or
evidence of steering parity. These policies and numerical weights are authored,
not recovered Unity graph rules. Do not interpret a synthetic replay as native
steering parity.

Unknowns: native Arno clip ownership, graph eligibility/mirroring, turn scheduling,
cloth bindings and native motion ownership. No high-profile backward sprint
is established by these vectors. Use lateral takes during steering, then resume
forward locomotion as facing catches up; no fake backward sprint is invented.

Falsification: feed opposite lateral requests in the same profile without an
idle transition; assert opposite original root displacement/take IDs. An unchanged
forward-only bank must fail this test. Replays must show strafe selection during
a corner and return to forward after holding direction; inspect fresh captures.

## Cloth reference boundary

Sibling `fh_bevy/src/cloth.rs`, `cloth/kernels.rs`, `cloth_runtime.rs`, and
`docs/re/CLOTH_SOLVER.md` supply a concrete solver/integration reference: authored
op programs, fixed-step particles, skinned targets, bone-linked deflectors, and
render deformation. The solver note identifies FH executable SHA-256
`7ca57147a51c8451c7a3d5b3043a0abf8ea53c41d436d7eba655a91482316916`,
base `0x7ff770000000`; these are not ACU offsets. Unity cloth record/op/render-map
compatibility has not been established. No cloth solver is enabled by this fix.

Loop audit: `local/directional-matcher-20261019/loop-audit.json` compares decoded
transform endpoints, excluding sentinel control hashes. Sidewalk maximum local
position mismatch is 0.001913 units; sprint sway is below 0.000014. Backward
clip has a 1.305588 mismatch on hash `a9611103`, an unweighted leaf in the
selected outfit/head/hair palettes (the base skeleton lists children_count=0).
That helper's semantics are unknown; no helper/native IK parity is claimed.
This endpoint check shares the decoder and is not independent codec validation.

Validation completion: 106 workspace tests pass, 18 install-dependent tests are
explicitly ignored by default; the original-install directional regression
passes when requested. Fmt, Clippy with warnings denied and build pass. Eight
fresh matcher captures were inspected, plus two fresh captures after adding
cooperative I/O decoding. Those two PNGs, motion traces and asset identities are
byte-identical to their pre-I/O counterparts. Manifests:
`local/captures/acu-directional-final-20261019/manifest.json` and
`local/captures/acu-directional-io-20261019/manifest.json`.
