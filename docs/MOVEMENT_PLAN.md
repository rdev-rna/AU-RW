# Unity movement reconstruction plan

## Objective

Build a hands-off, replayable Rust/Bevy reconstruction of Arno's movement from
ACU's own assets and evidence. The runtime we are building should run scripted
scenarios, expose its own collision and traversal decisions, and save enough
state to reproduce a failure without requiring a person to steer Arno.

The AC1 Movement Rewritten project is a workflow reference: its README describes
a clean-room Rust/Bevy reconstruction that loads AC1 models, animations, map
data, collision and climbable edges; it also documents autopilot cases and
replayable bug bundles. It is early and explicitly has partial/approximate
features, so this is a pattern for our tooling, not evidence about ACU mechanics
or data formats: [AC1 project](https://github.com/Banned445/AC1-Movement-Rewritten).

Do not assume ACU encodes climbable edges as AC1 does. Discover ACU's actual
world, collision and traversal representations first. When asset data does not
settle a mechanic, use a narrowly scoped native comparison. Keep observations,
inferences, implementation choices and unknowns separate.

## Current position

- Original idle and directional navigation clips can play through the composed
  rig. The current selection, transition, facing and root-motion adapter is
  partly informed by For Honor and remains an ACU implementation choice; actor
  ownership and native graph selection are open. See findings ACU-RE-0018/0019.
- The current Bevy player uses synthetic collision and authored input/state
  rules. Its vault, climb, ledge and drop behavior is not ACU parity; see
  `mechanics/test-world-movement.md`.
- The asset workspace has no typed ACU world/collision/traversal pipeline yet.
  World/map and other large resource families remain open under ACU-005.
- `tools/native_probe/` supports guarded hooks, watches, events and scripted
  controller input. No ACU movement, raycast or traversal site is semantically
  verified. A full process dump is not a movement trace and is not a prerequisite
  for this plan.

## Automated work loop

Every movement scenario should run from a known simulation start state with
fixed-tick input. One command should produce a portable report bundle containing:

- scenario definition, start-state and input timeline;
- source revision, executable/build identity, fixed-tick configuration and asset
  identities;
- per-tick position, velocity, facing, contact/grounding, movement state,
  candidate query results, selected action, animation/clip time/blend and root
  displacement, with unavailable fields explicit;
- deterministic replay output and first differing tick when compared with an
  earlier run;
- labeled screenshots or video and a debug view of collision surfaces,
  traversal candidates and chosen targets.

Run the Rust reconstruction offline for routine iteration. Keep a separate
original-game comparison mode for questions that need native evidence: replay a
prepared controller sequence, collect timestamped screenshots and byte-guarded
probe observations, and pair them with the Rust scenario. Probe events remain
raw until site identity, ABI, arguments and result semantics have been checked.
Do not require manual gameplay or visual steering as the normal workflow.

## Work order and exit gates

| Stage | Work | Exit evidence |
| --- | --- | --- |
| 0. Scenario and replay foundation | Define a fixed-tick scenario and trace format; add named scripted cases, reproducible start states, automatic trace/screenshot bundles and replay diffing to the current viewer/simulation. | The same input produces the same declared state trace; a deliberately changed expected value identifies the first differing tick. No ACU world reader or native game run is required for this gate. |
| 1. ACU movement-data inventory | Search real archives/resource metadata for world geometry, collision, navigation, traversal markers, interaction types and related references. Identify a small route and preserve archive/file/range hashes. | One candidate representation is decoded with independent structural checks, or a bounded negative finding records what was searched and the next evidence route. No AC1 schema assumptions. |
| 2. Data-backed route and debug view | Load one bounded ACU route into the simulation; render its collision and any authored movement affordances with stable source identities. Use explicit unknown/unsupported markers for missing layers. | Repeatable spawn and route, checked transforms/units where evidenced, debug overlays, malformed-data rejection and an inspectable capture. Synthetic gaps remain labeled. |
| 3. Core locomotion | Recover input mapping, grounded/air state, acceleration, stopping, turning, jump/fall/landing and collision response for the selected route. Reuse original clips only where ownership and root-motion behavior are supported. | Scenario matrix with fixed inputs and per-tick state/pose/root trace; each recovered rule cites evidence, and authored approximations remain identified. |
| 4. First traversal decision: vault | Determine how ACU senses an obstacle, builds eligible actions, selects a vault/mantle/continue/stop outcome, aligns Arno, blends the action and resolves root motion/contact. | At least one data-backed obstacle and one rejected/failed approach replay automatically; traces expose candidate/decision/result, and a targeted native comparison resolves the key unknowns. No screen-only classification is called recovered behavior. |
| 5. Climb, ledge and drop | Extend the same query/action interfaces and tests to climbing, grab/hang, shimmy/corner, pull-up, pass-over and safe drop. | Each mechanic has positive and negative scenarios, stable target identity, replayable state/animation traces and evidence for selection/alignment rules. |
| 6. Blend and feel refinement | Validate original animation ownership, graph transitions, blend timing, pose modifiers, IK, cloth, and camera interaction against matched input/state traces. | Matched views and numeric traces explain remaining deltas; matcher experiments remain optional and separately switchable from native-reference policy. |
| 7. Route breadth | Add world streaming, more environment types and interaction contexts only after the bounded route works. | Coverage matrix names tested geometry, actions, failure cases and remaining unsupported types; no single route implies full-city parity. |

## Native comparison policy

Use the game and DLL to answer specific unknowns, not as the main automation
surface. Start with existing mapped-image/decompile candidates, then verify the
live executable hash, module base, RVA and bytes before probing. Capture related
entry and post-call sites when a decision result is needed. Record thread/tick
context, controller inputs, event loss and instrumentation limits. If the
four-site probe capacity prevents simultaneous observation, replay the exact
scenario with rotated verified profiles and retain the same start/input trace.

If no suitable semantic site can be identified, use controlled before/after
observations and say which fact remains unknown. Do not enable guessed offsets or
claim that category checkboxes, strings, a screenshot, a memory watch or a
partial process dump established traversal semantics.

## Scope boundaries and dependencies

- This movement plan is a focused playable slice, not a prerequisite to finish
  every ACU-005 asset family. It does depend on typed world/collision/traversal
  data for each selected route; expand those decoders only as a movement scenario
  requires.
- Keep decoding and simulation independent of Bevy. Keep presentation,
  instrumentation and scenario orchestration outside the core movement rules.
- Preserve the current user-selected Tailored equipment and Day/Paris lighting
  goals for eventual visual review; they do not block the first data-backed
  movement tests.
- The sibling For Honor project may supply reviewed architecture and matcher
  comparisons only. It is not evidence for ACU's axes, thresholds, animation
  graph, traversal rules or timing.
- Keep readable records in `docs/`, reusable tooling in `tools/`, and runtime
  code in `acu-rs/`. Put machine captures and proprietary data under ignored
  `local/` paths.

## Next bounded task

Start at Stage 0/1: specify the scenario/trace contract, then inventory ACU world
and traversal candidates from the existing typed-resource scan and source maps.
Choose a route only after a resource identity and decode path are evidenced.
The first acceptance is a one-command offline replay bundle plus a finding that
identifies the best ACU movement-data lead (or a bounded negative result). Do not
implement a generic vault controller before those inputs are known.
