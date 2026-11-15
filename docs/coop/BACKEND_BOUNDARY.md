# Backend boundary register (co-op plan, Phase 5)

Each row is one place where ACU code crosses into Ubisoft online services: Quazal RendezVous (RDV), Hermes,
ubiservices, Uplay, or the Storm `punch` servers. Rule B1 in [../COOP_RE_PLAN.md](../COOP_RE_PLAN.md)
applies: **we record these, we never decode, call or emulate them.** We will use our own networking.

Columns:
- **VA:** the analysis view's VA (image base `0x140000000`, mapped image `9a1124f1…`).
- **Evidence:** observed (`obs`) or inferred (`inf`), plus the finding.
- **Replacement:** what our networking supplies instead. "None" means co-op doesn't need it.

| # | VA | Service | What crosses (apparent purpose) | Evidence | Replacement |
|---|---|---|---|---|---|
| 1 | `sub_1424E17B0` (via thunk `sub_1424E1D40`) | ubiservices | Reads the NAT-detect and traversal server URLs (`detectUrl1`, `detectUrl2`, `traversalUrl`) from the ubiservices object at `qword_145199610+0x138`. Returns false without it. | obs, ACU-RE-0073 | Our own peer addressing: LAN discovery, direct IP or our own lobby/relay. Leave Storm `punch` unconfigured. |
| 2 | `sub_142886730` | ubiservices | Parameter lookup used by row 1. Logs "Empty punch url is returned". | obs, ACU-RE-0073 | None, if row 1 is bypassed. |
| 3 | Storm `punch` (`storm::punch::inEvent::LoginServer`, `traversalLogin`) | Storm punch server | Logs in to a traversal server with a challenge exchange, detects the NAT type and connects endpoints. | names obs; behaviour inf | Not used. Direct, LAN or relay transport. |
| 4 | `sub_1423242A0` | RDV | Handles the RDV notification logged as "onHostMigrated … new host". | obs (string xref), purpose inf | None expected: Storm runs its own host migration (ACU-RE-0073 obs 3). To verify. |
| 5 | `sub_142582A90` | RDV | `JobConnectEndPoint::TryDirectConnect`, RDV's endpoint connection (PRUDP). | obs (string xref), purpose inf | Our transport. |
| 6 | `sub_1424E5130`, `sub_142514470` | RDV | Reference `RDV_OFFLINE`. Probably connection-state reporting. | obs (string xref), purpose unknown | To be decided. |
| 7 | `sub_14255B740` | probably RDV | References `OnHostMigrationCompleted`. | obs (string xref), side unknown | To be decided. |
| 8 | `0x142321A8C` (no function bounds) | Hermes | Hermes service initialisation ("Hermes Services already initialized"). | obs (string xref) | To be decided. Identity and session discovery go through our own lobby. |

Still to classify (Phase 2/3): `SessionTicket*` issuance (backend-signed or peer-local?), the Storm `relay`
module, matchmaking (`MissionMatchmakingManager`, `AmbMatchmaker*`, `OnlineRequestMatchmakingEvent`), and
party and invites (`NetPartyManager`, `PartyHost*Event`, `OnlineMatchmakingEventReceivedInvite`).
