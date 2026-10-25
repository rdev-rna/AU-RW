# Agent instructions

## Objective and scope

Build a Rust/Bevy reconstruction of Assassin's Creed Unity, working toward the
original game's behavior and appearance through small, evidenced playable slices.
The Rust workspace is in `acu-rs/`; the current scene is static asset inspection.
Read `MASTER_PROMPT.md`, `docs/STATUS.md`, and the current task before work.
Use `docs/ROADMAP.md` for sequencing and `docs/WORKFLOW.md` for the work loop.

## Asset index (use first)

A local SQLite index lists every resource in all 63 Forge archives (ID, name, type, archive,
entry, hashes) plus candidate links between resources. Query it instead of scanning archives.

- At the start of a session run `python tools/index_status.py`. If it is not READY, run
  `python tools/index_status.py --fetch` (downloads from a GitHub Release with a progress bar).
- Ask with `python tools/asset_query.py where <id> | find <name> | types | archive <name> |
  walk <name> | sql "SELECT ..."`. Details: `docs/ASSET_INDEX_QUICKSTART.md`.
- Evidence: a link is a **candidate** (equal bytes), never a decoded reference; a type label has a
  `confidence` (`observed-decode`, `crc-name`, `unknown`). Cite which you used in findings.
- Which duplicate ID the game loads is not established (#35). Do not assume latest archive wins.

The user authorizes writes throughout this project and reads elsewhere on this
machine. The user also explicitly authorizes executing original ACU game code
at the agent's discretion, including launching/restarting the game, attaching
debuggers, loading game DLLs, calling native routines, and building, deploying
and injecting DLL hooks for live inspection and comparison. No separate user
confirmation or advance capture-plan approval is required for these actions.
Live comparisons through a DLL hook are a core part of this rewrite. This
authorization is specific to this project; do not inherit the For Honor project's
runtime restrictions. Continue routine authorized work without repeated confirmation.
Keep derived code here and treat the corpus and sibling project as read-only
references. Hook/loader deployment and configuration changes needed for ACU
instrumentation are authorized; preserve replaced files/configuration and record
how to restore them. Prefer reversible instrumentation over permanent binary edits.
Do not publish, upload game data, message others, or change unrelated projects
as an incidental part of implementation.

## Paths and repository boundaries

- Project: `E:/ReverseEngineering/Projects/Unity`.
- Pseudocode: `ACU - Decompiled/` (217,041 C files at initial inspection).
  Collaborators configure their own corpus using `ACU_DECOMPILED_DIR` or
  ignored `local/project.json`; this folder is never distributed.
- Original install: `D:/Games/Assassin's Creed Unity/`; use `ACU_GAME_DIR` or
  ignored `local/project.json` instead of embedding this path in Rust code.
- Reuse reference: `E:/ReverseEngineering/Projects/ForHonorRustRewrite/`;
  see `docs/FORGE_REUSE.md` before importing its runtime reader.
- Git currently resolves to `E:/ReverseEngineering/Projects`, a parent repo.
  Never initialize a nested repo, stage the parent tree, or change its settings
  incidentally. Use explicit project-relative file paths for Git operations.
- There is no Unity-specific remote, license, release workflow or CI configured.
  Do not invent one or copy the sibling project's publishing policies.
  `CONTRIBUTING.md` documents portable setup and a local source-only bundle;
  that bundle does not authorize publication or changing the parent repository.

Preserve reference bytes, apart from documented reversible runtime deployment.
`local/`, `outputs/`, `assets/` and `target/`
hold ignored machine data, generated files and caches. Put portable project
knowledge in `docs/`, reusable tooling in `tools/`, and future Rust code in
`acu-rs/`. Review `.gitignore` before adding large or derived files.

## Evidence rules

Pseudocode is an analysis artifact, not compilable source or proof of execution.
Preserve original names and bytes. Record recovered names separately.
Distinguish observations, inferences, implementation choices and unknowns.
Never invent offsets, file IDs, animation semantics or parser compatibility.

Every reverse-engineered behavior needs a finding with reproducible provenance:
binary SHA-256, image base, VA/RVA, corpus path/lines or disassembly reference;
for assets, archive identity, offset, file ID, stored/decoded length and hashes.
Use `docs/templates/FINDING.md`. Address filenames suggest export VAs; the
export's correspondence to this installed executable remains unverified.
Preserve uint64 file IDs as 16-digit hexadecimal strings in JSON.

Use REA/IDA only when the question needs binary evidence, and bind the explicit
ACU target before analysis. Never reuse an attached For Honor session or its
catalogue as Unity evidence. Record tool/provider identity and limitations.
Use `rg` with bounded results; avoid printing the entire 217k-file directory.
For an exact address, open the corresponding file directly.

## Implementation rules

Implement one vertical slice at a time; each task needs a bounded result and
acceptance criteria. A synthetic test arena is a validation tool; label it as
synthetic and do not claim original-world or animation parity from it.
Keep simulation and data decoding independent of Bevy. Start with a small
workspace; add crates only as responsibilities become concrete.
Use fixed simulation ticks, explicit input/state, seeded randomness when needed,
and explicit coordinate/unit conversions. Record unresolved float determinism.
Do not guess native axes, world units, tick rates, patch precedence or root-motion
ownership. Preserve stable asset identities through rendering.

For parser reuse, import only reviewed modules with provenance; avoid a live
path dependency on the sibling workspace. Fail clearly on unsupported variants.
Bound allocation/decompression, check offsets/overflow, and detect archive-table
cycles. Keep source-game-specific decoding behind explicit format boundaries.
Do not change sibling code to make ACU work.

Select and pin Rust/Bevy versions at workspace creation using then-current
official compatibility documentation; record the decision and commit Cargo.lock.
Do not prescribe an unverified Bevy version from memory. No dependency download
or installation is needed for the preparation tools.

## Validation and completion

Preparation checks, run from this directory:

```powershell
python tools/acu.py check
python tools/acu.py doctor
python tools/acu.py inventory --output local/inventory.json
```

After the workspace exists, run from `acu-rs/`:

```powershell
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets
cargo test --locked --workspace
cargo build --locked --workspace
```

These Rust commands are future gates, not checks already available or passed.
Run meaningful tests for format boundaries and recovered behavior, including
invalid inputs and independent expected results. Install-dependent tests must
be explicit and report missing data as unavailable, never as parity success.
Visible changes require a runnable build and fresh visual inspection; compilation
alone is not visual validation. Follow `docs/VALIDATION.md`.

Update `docs/STATUS.md`, the task and relevant findings after each completed
slice. Report what changed, commands/results, coverage and remaining uncertainty.
Save personal continuation handoffs outside repositories under
`C:/Users/PC/.codex/handoffs/Unity/`, with a unique filename. Include Goal;
Plan / Roadmap; Tried & Failed; Current State & Modified Files; Next Step.
Link the exact resume path and suggest `/clear` at meaningful checkpoints.
Do not overwrite another agent's handoff or use project instructions as a diary.

## Coordination

Claim work through GitHub issues before starting and follow `docs/PROJECT_BOARD.md`
(claim, locks, per-slice status notes, handoffs). Areas: `docs/AREAS.md`.
