# Animation sources: what is and is not established

Web research 2026-10-22. Confidence labels: **binary** (our own evidence), **public** (published
source), **claim** (maintainer statement, no finding yet), **none** (searched, nothing found).
Public material cannot establish ACU behaviour; the executable can (AGENTS.md evidence rules).

## Maintainer claim
ACU does not use For Honor-style motion matching; its movement relies on complex motion warping
(root-motion/target warping). **claim**. A finding must back this before docs state it as fact.
Related binary leads: `traversal::landing_score` 0x140177c40, scanner 0x14017f8e0 (ACU-RE-0030),
`acu_motion::target_warp`. These are **binary** observations of target scanning; the warp
consumer is not yet identified.

## Public sources reviewed
| Source | What it says | Relevance |
|---|---|---|
| Game Anim / Wolfire GDC13 summary of Jonathan Cooper, "Animating the 3rd Assassin" (AC3, 2013): https://www.wolfire.com/blog/2013/04/GDC13-Summary-Animation-Bootcamp-Part-4-6/ , https://www.gameanim.com/2014/02/11/animating-3rd-assassin/ | Parametric blending of jump clips by height/distance; brute-force transition matrices for assassinations; IK for hand/foot placement when climbing; procedural lean/foot-placement for run turns; whole-body procedural layer for jump/landing reactions; spring/cloth secondary motion | **public**, AC3 only (pre-Unity, older engine revision). Suggests the series' lineage; not proof for ACU |
| Wikipedia, Ubisoft Anvil: https://en.wikipedia.org/wiki/Ubisoft_Anvil | AnvilNext was Unity's engine; animation used HumanIK middleware (hands/feet) | **public**, secondary; HumanIK usage for ACU not verified |
| Holden, data-driven character control (Ubisoft La Forge, 2018): https://www.gamedeveloper.com/programming/video-character-control-with-neural-networks-in-i-assassin-s-creed-i- | Later research on learned locomotion; not ACU shipping tech | context only |
| Clavet, Motion Matching (For Honor, GDC 2016) | Motion matching origin | explains why the For Honor reuse does not transfer |

## Searched, nothing found (**none**)
- Any Ubisoft talk, paper or postmortem describing ACU (2014) locomotion, parkour or "motion warping".
- Any public document naming an "Atom" animation system. The name comes from the binary
  (`AtomWorldComponent::AnimationSyncGroupSetup`, string leads only, not proof).
- Next places to look: GDC Vault "Assassin's Creed" animation sessions 2013-2015, Ubisoft
  La Forge publications, conference slides by Ubisoft Montreal animation programmers.

## Consequences for the project
1. Do not extend the `amo*`/`loco_match` modules (issue #99 retires them).
2. Re-scope motion work around recovering the native selection and warp mechanism (#10, #54, #11, #12).
3. Treat the AC3 techniques (parametric blend, IK, target-driven placement) as hypotheses to
   test against the binary, not as design to copy.
