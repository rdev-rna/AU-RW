# Work areas

Each area has an `area:*` label and a parent epic. Evidence below is leads only:
string hits in the pseudocode are not proof of mechanics. The export's correspondence
to the installed executable is bound by PE headers and file-backed resources
(ACU-RE-0031); the code bytes themselves are not comparable because the installed file
is packed. Status: done / partial / started / none.

| Area | Status | Scope | Anchors and reuse |
|---|---|---|---|
| binding | partial | ACU-003 done (ACU-RE-0031): installed exe `748abdc6..`, VA = RVA + 0x140000000; `40ded8de..` unidentified (issue #34). Remaining: name registry | NetProtocol string layer 0x140278900-0x141F35C50 |
| assets-containers | partial | Forge, patch precedence, map/DLC archives, mod-layer exclusion | `acu_assets` forge/frame/package/catalog; ASSET_COVERAGE.md |
| assets-types | partial | Entity, EntityGroup, LODSelector, Material/TextureSet/TextureMap variants, skeleton remainder, codec 7/8 | STATUS unsupported-type counts |
| world-data | none | map/world/layer/cell format; units, axes, tick rate decisions | DECISIONS.md pending items |
| collision-nav | none | Havok collision readers, nav data, traversal surfaces | Havok 0x142Axxxx-0x142Dxxxx; FH `world_collision`, `static_walls` (review first) |
| streaming | none | cell streaming, LOD, entity pools, budgets | `Engine::ProcessEntityPoolCreationRequests`, `ExecuteLinkRequests` |
| environment | none | static geometry, props, interiors, lighting/GI, weather (unconfirmed) | `*_gi` archives |
| traversal | started | scanner, root warp, Parkour states, vault/climb/ledge/drop | ACU-RE-0030; Parkour_* ~0x1406xxxx; MOVEMENT_PLAN |
| statemachine | partial | native graph, Atom sync groups, controller, input, actor identity | `AtomWorldComponent`; FH `graph.rs`, `anim_state.rs` |
| motion | partial | native locomotion/animation selection (recover, do not assume), reversals, root-motion ownership, codec-7 | PLAN_MOTION_PARITY; ACU-RE-0029 |
| warping | partial | motion warping | `acu_motion/src/target_warp.rs` |
| trajectory | started | landing/trajectory prediction | `traversal::landing_score` (0x140177c40) |
| ik | none | foot/look-at/hand IK | `Anim::SkeletonIKUpdate` |
| modifiers | partial | bone modifiers, coat, spring, ragdoll, cloth | ACU-RE-0028; FH cloth (read-only) |
| crowd | none | herding, composition, LOD, instancing, 10k scale | CrowdHerder/FakeCrowd ~0x1402B-C; docs/references crowd PDFs |
| ai-awareness | none | AwarenessDirector, perception, alert, notoriety, factions | ~0x1404Axxx-0x1404Exxx; FH `soldier_ai.rs` |
| ai-navigation | none | pathing, steering, avoidance | `Ai::NavigationPathingUpdate/SteeringUpdate` ~0x14031x |
| combat | none | melee/firearm/bomb RFCs, damage, takedowns, hit reactions | ~0x14034xxxx; FH `fh_combat` |
| render | partial | deferred renderer, shaders, post FX, particles | `acu_bevy/shaders`; RENDERING_SOURCES.md |
| audio | none | sound engine, `.pck` parsing | `Sound::Engine::SoundManager`; no Wwise strings |
| video | none | Bink2 `.bk2` inventory/playback | `bink2w64.dll` |
| ui | none | HUD, menus, localization | format unknown |
| mission | none | missions, quests, cinematics, scripting data | `Ai::UpdateCinematicManager` |
| save | none | profile and savegame data | `Assassin*ProfileData`, `Engine::SaveGame*` ~0x1408D-0x14106 |
| progression | none | skills, store/currency (offline), equipment, outfits | Helix/store strings; BuildTable lead |
| coop | none | co-op, heists, club, matchmaking (deferred, M6+) | matchmaking ~0x1404DCxxx |
| physics | none | Havok runtime: character body, ragdoll constraints | hknp/hkp strings |
| tooling | partial | probe/tap, oracles, captures, gates | `tools/`, `acu_tap_bridge` |
| infra | partial | workspace, crates, docs, board hygiene | VALIDATION.md |

Note (maintainer, 2026-10-22): ACU does not use For Honor-style motion matching. The `amo*` and
`loco_match` modules in `acu_motion` are For Honor-derived scaffolding for the viewer, not recovered ACU
behaviour. Do not extend them; recover the native selection mechanism instead (see the motion epic).
