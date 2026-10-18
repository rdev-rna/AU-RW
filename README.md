# AU-RW

An Assassin's Creed Unity reconstruction in Rust and Bevy.

Repository: [rdev-rna/AU-RW](https://github.com/rdev-rna/AU-RW), maintained by Rdev.
Clone with `git clone https://github.com/rdev-rna/AU-RW.git`, then follow
[contributor setup](CONTRIBUTING.md) and the
[onboarding prompt](docs/COLLABORATOR_PROMPT.md).

Early reconstruction grounded in local game bytes, decompiled pseudocode and
reproducible comparisons. A Rust runtime Forge/LZO reader, typed mesh/material
readers and a textured Arno inspection scene exist; full asset support and
playable movement are still incomplete.
Read the [runtime reader guide](acu-rs/README.md) and
[all-assets acceptance criteria](docs/ASSET_COVERAGE.md).

Start with [agent instructions](AGENTS.md) and the [master prompt](MASTER_PROMPT.md).
New collaborators: use the [onboarding prompt](docs/COLLABORATOR_PROMPT.md)
to guide your first session and contribution.
The [status](docs/STATUS.md) records verified setup facts and open questions.

## Quick start

For the grey test scene, run `./tools/run_arno.ps1 -GameDir <your-ACU-install>`.
Arrows orbit, Q/E zoom, Escape exits. Assets load directly from the installed
archives; proprietary data is excluded from source control. See
[runtime setup](acu-rs/README.md) for builds, capture options and parser tools.

| Feature | Current verification |
| --- | --- |
| Static textured Arno / grey arena | Fresh diffuse/material captures; synthetic lighting |
| Skeletons | Full 109-bone male base decoding; bind/skin remaps unresolved |
| Arno's Tailored four-piece set | Required default; modular asset selection in progress |
| Day, Paris lighting | Required target; native environment/shader selection in progress |
| Animations / traversal / movement | Not playable; decoding and evidence recovery in progress |

Mission sequencing is in [MOVEMENT_PLAN.md](docs/MOVEMENT_PLAN.md), with the
running [progress log](PROGRESS.md). Rendering references and attribution are in
[RENDERING_SOURCES.md](docs/RENDERING_SOURCES.md).

Run from the checkout root in PowerShell:

```powershell
python tools/acu.py check
python tools/acu.py doctor
python tools/acu.py inventory --output local/inventory.json
```

Python 3.12+ and its standard library are sufficient. These tools do not launch
the game, load DLLs, download dependencies or edit reference inputs. Inventory
hashes `ACU.exe`, reads archive prefixes, and summarizes the corpus; it does not
decode archives or establish correspondence between the export and executable.

Configure your own original game install and decompiled corpus locations:

```powershell
python tools/acu.py configure --game-dir 'D:/MyGames/ACUnity' --decompiled-dir 'E:/MyResearch/ACU-pseudocode'
python tools/acu.py paths
```

Precedence: `ACU_GAME_DIR` / `ACU_DECOMPILED_DIR` > `local/project.json` > example configuration.
Paths inside configuration resolve relative to this project. Existing local
configuration is never overwritten by the tools.
The sibling For Honor checkout is optional. See [contributing](CONTRIBUTING.md)
for portable setup, validation and the source-only publication candidate.

## Project map

| Path | Purpose |
| --- | --- |
| `ACU - Decompiled/` | Existing reference pseudocode; preserve unchanged |
| `docs/WORKFLOW.md` | Evidence-to-implementation work loop and search recipes |
| `docs/ROADMAP.md` | Milestones and exit criteria |
| `docs/ARCHITECTURE.md` | Proposed module boundaries and decisions to settle |
| `docs/FORGE_REUSE.md` | Existing runtime reader and Unity adaptation plan |
| `docs/VALIDATION.md` | Parser, simulation and visual verification |
| `docs/TASKS.md` | First actionable tasks |
| `docs/FINDINGS.md`, `docs/DECISIONS.md` | Durable evidence and decision indexes |
| `docs/templates/` | Task, finding, decision and capture templates |
| `tools/acu.py` | Read-only inspection and workflow checks |
| `acu-rs/` | Rust runtime asset library, probe and coverage validation |
| `local/`, `outputs/`, `assets/` | Ignored machine configuration/data/captures |

The active task is [ACU-005](docs/TASKS.md): complete runtime parsing of the
original assets, including models, maps, animations and textures. Extraction
is not required. Rust/Bevy choices are recorded; renderer settings and gameplay
constants remain undecided until verified.

The original research directory remains inside `E:\ReverseEngineering\Projects`.
The GitHub repository uses a separate source-only checkout and fresh history;
the parent repository and its configuration are unchanged. No project license
has been assigned.
