# Roadmap

The long-term objective is broad game fidelity. Each milestone closes a bounded
capability; none implies full-game parity. Revisit later priorities after each
playable slice rather than attempting a complete engine translation up front.

| Milestone | Deliverable | Exit evidence |
| --- | --- | --- |
| P0 — preparation | Instructions, tasks, tools, inventory and parser reuse map | Workflow checks pass; identities and unknowns recorded |
| M0 — data foundations | ACU format matrix, minimal Rust workspace, bounded archive reader | Base and patch entries decoded reproducibly; malformed inputs rejected; supported variants documented |
| M1 — first visible asset | One identified mesh/material and, where available, skeleton displayed in Bevy | Source IDs/hashes, correct transforms, visual inspection and repeatable launch |
| M2 — first controllable slice | Character and camera in a small explicit test scene; idle/walk/run/turn | Fixed input replay, animation/collision ownership documented, original reference captures where available |
| M3 — traversal slice | One representative obstacle route: climb, vault, ledge and drop as scoped tasks | State transitions and motion compared with identified native/data evidence and captures |
| M4 — small original-world slice | Bounded Paris area, streaming/collision and essential interactions | Stable identities, patch resolution, traversal and performance measured in that area |
| M5 — gameplay expansion | Combat, stealth, AI, missions, audio, UI, saves as separate slices | Per-feature scenarios and persisted-state/versioning checks |
| M6 — breadth and polish | Larger world, crowds, content coverage and optimization | Coverage matrix, representative routes and measured budgets |

Native animation/traversal research may run earlier when needed to unblock a
specific slice. A discovered codec can change M0 sequencing. Online/co-op design
is deferred; do not invent a multiplayer requirement from the sibling project.

Do not hardcode a frame rate, memory budget or visual parity percentage before
the first measured baseline. Set measurable budgets in the relevant task.
