# AU-RW master prompt

New collaborators should start with the portable
[onboarding prompt](docs/COLLABORATOR_PROMPT.md). This file describes the
original research machine's continuation context.

Paste this into a new coding-agent session opened at
`E:\ReverseEngineering\Projects\Unity`:

```text
We are building AU-RW, a Rust/Bevy reconstruction of Assassin's Creed Unity, working
toward original behavior and appearance through evidence-backed vertical slices.

Read AGENTS.md, docs/STATUS.md, docs/ROADMAP.md and docs/WORKFLOW.md first.
Resume from a supplied external handoff before repeating investigations.
Choose the next ready task from docs/TASKS.md unless I specify another task.
Complete the task's acceptance criteria, validate, and update project records.
Do the work; do not stop after proposing a plan.

Reference inputs:
For collaborators, use `python tools/acu.py configure --game-dir <install>
--decompiled-dir <corpus>`; `ACU_GAME_DIR` and `ACU_DECOMPILED_DIR` override
ignored local/project.json. The machine paths below identify the original
research environment, not required checkout locations. The sibling is optional.
- Decompiled pseudocode: ACU - Decompiled/ (217,041 address-named C files).
- Installed game: D:/Games/Assassin's Creed Unity/.
- Existing runtime Forge parser:
  E:/ReverseEngineering/Projects/ForHonorRustRewrite/fh-rs/crates/fh_assets/src/forge.rs
  Read docs/FORGE_REUSE.md for its dependencies and ACU compatibility questions.

You may write throughout this project and read elsewhere on this machine.
You also have standing permission to execute ACU game code at your discretion:
launch/restart it, attach debuggers, load DLLs, invoke native routines, and build,
deploy or inject DLL hooks. Live DLL-hook comparisons are central to this rewrite.
No per-action permission or advance capture-plan approval is needed. This differs
from the For Honor project; do not import its execution restrictions. Preserve
replaced files/configuration for reversible instrumentation and record deployment.
Keep the pseudocode and sibling project unchanged; implement derived work here.
Do not repeatedly ask for permission for routine authorized work. Use explicit project file paths
for Git because this directory is currently inside a parent repository.

Treat pseudocode as fallible. Separate observations, inferences and choices.
Tie recovered behavior and assets to hashes, addresses/offsets, file IDs and
reproducible evidence. Never transfer For Honor assumptions to Unity silently.
Preserve uncertainty instead of filling gaps with plausible game behavior.

Keep the simulation and asset readers independent of Bevy; use fixed ticks and
explicit inputs. Deliver small observable slices before expanding systems.
Do not start multiplayer, a city-wide renderer, or a wholesale code translation
before the foundations and first traversal slice are demonstrated.

Start with the preparation checks and existing inventory. State the bounded task
and acceptance criteria, then implement and validate it. If a dependency or a
required user choice blocks one part, continue independent useful work.
End with changes, validation, limitations and the next concrete task.
At a meaningful checkpoint save a unique handoff outside the repository and
provide its exact path for a fresh session.
```

For a new contribution append (ACU-001 is already complete):

```text
Assigned task: <task ID and bounded slice>.
Acceptance criteria: <observable result and evidence required>.
Handoff: <path, or none>.
Read the task's current status before investigating. Do not repeat completed
compatibility or setup work. If no task is assigned, identify a ready bounded
slice from docs/TASKS.md and coordinate file ownership with collaborators.
```
