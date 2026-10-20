# AU-RW

Experimental runtime reconstruction in Rust and Bevy.

An evidence-led project exploring archive formats, modular characters, skeletal
animation and movement systems. The current build is a static asset inspection
scene; playable movement and comprehensive format support are in progress.

Only authored code, tools and research notes are included. Reference data,
local configuration, captures and build outputs stay outside source control.
No project license has been assigned.

## Getting started

Clone [rdev-rna/AU-RW](https://github.com/rdev-rna/AU-RW), then follow the
[contributor setup](CONTRIBUTING.md) for local inputs and build requirements.
The [onboarding prompt](docs/COLLABORATOR_PROMPT.md) guides coding-agent sessions.

The [runtime guide](acu-rs/README.md) covers the inspection scene, camera
controls, capture options and decoder tools. Assets load from a locally
configured installation; no bundled reference assets are provided.

## Current work

| Area | State |
| --- | --- |
| Archive and resource reading | Bounded readers and reproducible coverage reports |
| Character inspection | Modular textured meshes in a synthetic arena |
| Skeletons and skinning | Hierarchy and palette analysis; rig composition in progress |
| Animation and movement | Research and decoding; not playable yet |
| Rendering | Diagnostic presentation; native shading and lighting remain open |

See the [progress log](PROGRESS.md), [status](docs/STATUS.md),
[task queue](docs/TASKS.md) and [roadmap](docs/ROADMAP.md) for verified results
and the next bounded slices. Findings retain precise provenance and limitations.

## Project guide

| Location | Purpose |
| --- | --- |
| [Contributor guide](CONTRIBUTING.md) | Configuration, builds and validation |
| [Agent instructions](AGENTS.md) | Scope and evidence rules |
| [Working loop](docs/WORKFLOW.md) | Research-to-implementation workflow |
| [Architecture](docs/ARCHITECTURE.md) | Runtime boundaries and design decisions |
| [Validation](docs/VALIDATION.md) | Parser, simulation and visual gates |
| [Findings](docs/FINDINGS.md) | Reproducible research records |
| [Movement plan](docs/MOVEMENT_PLAN.md) | Character and traversal sequencing |
| [Asset coverage](docs/ASSET_COVERAGE.md) | Full decoder acceptance criteria |

Keep contributions scoped to one reviewable slice. Work on feature branches,
record evidence and validation, and merge significant validated milestones into
`main`.
