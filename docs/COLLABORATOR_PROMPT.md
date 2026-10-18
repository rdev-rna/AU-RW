# AU-RW collaborator onboarding prompt

Open a coding-agent session at your checkout root. Fill in the fields below,
then paste the entire block. Coordinate the task and files with the maintainer
before overlapping another contributor's work. Setup details are in
[CONTRIBUTING.md](../CONTRIBUTING.md).

```text
You are contributing to AU-RW, a Rust/Bevy reconstruction of Assassin's Creed
Unity. Work toward original behavior and appearance through small, evidenced
vertical slices. Complete the assigned slice, validation and project records.

Checkout root: <absolute path to your checkout>
Original game install: <your local install path, or unavailable>
Decompiled corpus: <your local corpus path, or unavailable>
Assigned task and slice: <task ID plus one bounded deliverable>
Acceptance criteria: <observable result and required evidence>
Owned files / coordination notes: <agreed files and concurrent work>
Continuation handoff: <absolute path, or none>
Personal handoff directory: <outside all repositories>

1. Establish context before editing
Read AGENTS.md, MASTER_PROMPT.md, CONTRIBUTING.md, docs/STATUS.md,
docs/TASKS.md, docs/ROADMAP.md and docs/WORKFLOW.md. Resume the supplied handoff
before repeating investigations. Read relevant findings, docs/VALIDATION.md
and docs/MOVEMENT_PLAN.md for movement work. Inspect existing changes and the
actual Git root. Preserve other contributors' edits; use explicit project paths.
The original research checkout is inside a parent repository: do not initialize
nested Git or stage/publish its history. Do not assume its absolute paths exist
on this machine. If no task is assigned, coordinate a ready bounded slice first.

2. Verify local setup
Use Python 3.12+ and the pinned acu-rs/rust-toolchain.toml and Cargo.lock.
With your own inputs available, run from the checkout root:
  python tools/acu.py configure --game-dir '<install>' --decompiled-dir '<corpus>'
  python tools/acu.py paths
  python tools/acu.py check
  python tools/acu.py doctor
Configure once: it refuses to overwrite local/project.json. Edit that ignored
file for later changes. ACU_GAME_DIR / ACU_DECOMPILED_DIR override configuration.
The For Honor reference checkout is optional; do not add a live path dependency.
Reuse a current local inventory; when inputs change or none exists, run:
  python tools/acu.py inventory --output local/inventory.json
The static scene requires the install, not the corpus. Doctor/inventory need
both; workflow checks and authored tests need neither. If inputs are absent,
record unavailable checks and continue independent work within the assigned scope.
Do not report missing-data checks as passes.

3. Bound the contribution
State the question, acceptance criteria, owned files and expected validation.
Current capability is static textured Arno in a synthetic inspection arena;
movement, animation, full asset coverage and native lighting remain incomplete.
ACU-005 is broad: choose one explicit sub-slice rather than attempting it all.
Consult current status for priorities and counts; do not treat this prompt as
a frozen task assignment. No new platform, multiplayer or engine rewrite scope.

4. Follow the evidence-to-code loop
Gather the smallest useful evidence, write/update a finding, implement one
slice, validate independently, then update the task, status and relevant indexes.
Use docs/templates/TASK.md and docs/templates/FINDING.md. Separate observations,
inferences, implementation choices and unknowns. Pseudocode is fallible and its
correspondence to the installed executable is not established merely by a VA.
Record executable hash, image base, VA/RVA and corpus lines/disassembly; for
assets record archive identity, offset, file ID, stored/decoded lengths and hashes.
Keep uint64 IDs as 16-digit hexadecimal JSON strings. Use bounded rg searches
and exact address files. Bind the explicit ACU target for REA/IDA; never use
an attached For Honor session as ACU evidence. Read docs/FORGE_REUSE.md before
importing reviewed parser modules. Fail clearly on unsupported variants.

Keep decoding and simulation independent of Bevy. Check bounds/overflow,
allocation limits and archive-table cycles. Preserve stable asset identities.
Use explicit inputs and fixed simulation ticks; do not guess native axes, units,
tick rates, patch precedence, animation semantics or root-motion ownership.
Read original assets at runtime; do not introduce an extracted-data dependency.
Use the runtime authorization in AGENTS.md within this project's scope. Keep
instrumentation reversible, preserve replaced files/configuration and document
deployment, restore steps, target identity and actual observed traces.

5. Validate the claim
Run from the checkout root:
  python tools/acu.py check
  python -m unittest discover -s tools -p 'test_*.py'
For Rust changes, run from acu-rs/:
  cargo fmt --all -- --check
  cargo clippy --locked --workspace --all-targets -- -D warnings
  cargo test --locked --workspace
  cargo build --locked --workspace
For direct Cargo install checks, set ACU_GAME_DIR to your resolved install:
  $env:ACU_GAME_DIR = python tools/acu.py paths --key game_dir
Run that command from the checkout root. Cargo tools do not read local/project.json.
Run relevant ignored install tests explicitly; record exact commands and results.
Test malformed inputs and independent expectations for recovered formats.
For visible changes, build, launch, save a fresh capture manifest and inspect
the result. The launcher from the checkout root is ./tools/run_arno.ps1.
Passing compilation or rendering a synthetic arena does not prove game parity.

6. Finish a reviewable contribution
Update docs/STATUS.md, the task and relevant findings/indexes; update PROGRESS.md
when the movement mission changes. Report changed behavior, commands/results,
coverage limits, unresolved questions and the next concrete step. Review the
diff and explicit file list. Keep proprietary data, decompiled code, binaries,
archives, extracted assets, captures, machine reports, credentials and local
settings out of commits/uploads. No publication is implied by this task.

Save a unique personal handoff outside repositories with Goal; Plan / Roadmap;
Tried & Failed; Current State & Modified Files; Next Step. Include exact paths
and validation results. Provide its resume path and suggest /clear at a useful
checkpoint. Keep portable knowledge in shared docs and personal notes outside.
```
