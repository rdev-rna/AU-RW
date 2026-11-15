# ACU-RE-0074: Do other installed Assassin's Creed titles carry a reusable replication stack?

Date / investigator / related task: 2026-11-15 / Claude (Opus 5.5), static only, read-only file scan / #85, co-op plan Phase 6 ([COOP_RE_PLAN.md](../COOP_RE_PLAN.md))
State: observed (string signatures in the installed executables). It is not established whether the code behind those strings can be reached.

## Provenance

Binary path / SHA-256 / image base / VA / RVA (files read only, nothing executed or modified):

| File | SHA-256 |
|---|---|
| `E:/Games/Assassins Creed IV Black Flag/AC4BFMP.exe` | `1aa1e505d953eb0f9657fe3e151854f97d403c05e3e21e6ff4c6ddc9307fc70a` |
| `E:/Games/Assassins Creed IV Black Flag/AC4BFSP.exe` | `bc11ad7924e1eca7363ee86aa3332c05386d0a05896c0b7370131daee9ab5150` |
| `E:/Games/AC Freedom Cry/ACFC.exe` | `63a1dc7a357943c8e7c9c7529213bd8e6e934332c0b9d1819063b74534dae96d` |
| `E:/Games/Assassin's Creed II/AssassinsCreedIIGame.exe` | `7749d96ce04d7383352a36327ea3910a076704a8e0273206d938574658ee717b` |
| `E:/Games/Assassins Creed/AssassinsCreed_Dx9.exe` | `393dd15e5167f27e6bd676e23ad80f62f208bbc1ffaab38b3ea87301b8a7f790` |
| `D:/Games/Assassin's Creed Unity/ACU.exe` (packed control) | `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40` |
| ACU analysis view (unpacked control) | `47a81dc9f29c48a709cbdc1ef60a7e7db9eb8caefaabf30940dd4e1ee755d8c2` |

Corpus path and lines / export identity status: not used.
Archive identity / byte range / range hash / file ID (hex string): not applicable.
Stored and decoded lengths / SHA-256: as in the table.
Tool/provider/version / evidence IDs or saved output: `tools/net_title_survey.py` (stdlib only) writes
`local/coop/title_survey.json`. Two runs produced byte-identical output.
Exact commands and search boundary: the whole file, matching printable ASCII strings of 4 to 200 bytes.

```powershell
python tools/net_title_survey.py "<each exe above>" --output local/coop/title_survey.json
```

## Result

### Observations

| Executable | Entropy | Storm/Echo | NetProtocol | HackNetZ | RDV | ubiservices | `Net*` class names |
|---|---|---|---|---|---|---|---|
| ACU analysis view (control) | 6.51 | yes | yes | no | yes | yes | 135 |
| `ACU.exe` on disk (control) | 7.88 | ? | ? | ? | ? | ? | 0 (packed) |
| `AC4BFMP.exe` (Black Flag multiplayer) | 6.77 | no | no | **yes** | yes | yes | 32 |
| `AC4BFSP.exe` (Black Flag campaign) | 6.73 | no | no | no | yes | yes | 8 |
| `ACFC.exe` (Freedom Cry) | 6.73 | no | no | no | yes | yes | 8 |
| `AssassinsCreedIIGame.exe` | 6.32 | no | no | no | yes | no | 0 |
| `AssassinsCreed_Dx9.exe` (AC1) | 6.27 | no | no | no | no | no | 0 |

1. **The method works on both controls.** The packed ACU executable reports *unknown*, not absent. The
   unpacked ACU view shows both of ACU's stacks. The other five executables have normal code entropy
   (6.3 to 6.8) and thousands of readable strings, so an absent signature there is meaningful.
2. **No other installed title carries ACU's stack.** None has a single `storm::`, `_NetProtocol`,
   `zenitar_` or `NetObjectRemoteFunctionCall` string.
3. **Black Flag's multiplayer executable runs on HackNet/HackNetZ:** 911 `hacknet` strings, `DupSpace`,
   `DOCore`, `MasterStation`, `ChangeMasterStation`, and messages such as "CREATE_DUPLICA message for %s
   %x (master is on Station %x)". The source paths read
   `D:/_Work/AC3/acga_code/.../extern/online/middleware/hacknet/{HackNetCore,HackNetZ}`, with
   `HackNetCore` transports `PRUDP`, `UDP`, `Routing` and `UPnP`. Its game-level classes include
   `NetObject`, `NetEntity`, `NetCharacter`, `NetPlayer`, `NetGenericNPC`, `NetProxyClass`,
   `NetSessionMember`, `NetGameplayCoordinator` and `NetSoundReplicator`.
4. **Black Flag's campaign executable and Freedom Cry have no replication framework.** Their only network
   strings belong to the RDV and ubiservices clients (`PRUDP` × 36 inside the RDV paths). Their 8 `Net*`
   names are Windows NETAPI imports (`NetApiBufferFree`, `NetStatisticsGet`) and online-challenge classes
   (`NetChallenge*`, `NetConnectionStatus`, `NetMapMarkerComponent`). There is one stray string,
   `NetZ has not been initialized`, and no HackNetZ library strings to go with it.
5. **ACU and Black Flag multiplayer share a few class names:** `NetObject` and `NetPlayer`. ACU's type
   records root a family at `NetObject` (ACU-RE-0072).

### Inferences and alternatives

- **Ubisoft moved from HackNetZ to Storm/Echo.** HackNetZ is apparently Ubisoft's fork of Quazal Net-Z,
  used in AC3 and Black Flag multiplayer. Unity moved to Storm/Echo. Both use the same model: master and
  duplica/replica objects with master migration. The shared `NetObject` name hints at a game-side layer
  that carried over. This is inferred from names; no code was compared.
- **Porting into the Black Flag or Freedom Cry campaigns means reimplementation (plan option C).** There is
  no dormant replication code to switch on. The engine-level hooks (entity components, events, animation)
  have to come from those executables, and replication comes from our side, specified by the ACU work.
- **Black Flag multiplayer is an alternative host,** since it already has P2P replication for characters
  and NPCs on the AnvilNext engine. Whether it could load campaign content is unknown and probably a
  large job. It is recorded as an option, not a recommendation.
- AC1 and AC2 have no replication layer at all. AC2's RDV strings belong to its online-services client.

### Unknowns / incomplete coverage

- Strings show a library is linked, not that it runs. The `NetZ has not been initialized` string in the
  campaign executable hasn't been followed.
- Syndicate, Rogue, AC3 and the later titles are not installed. Syndicate, which shares Unity's engine
  generation, remains the most likely title to carry Storm/Echo. The tool is ready for it.
- Packed executables (the installed ACU) need a runtime dump before they can be surveyed.

### Independent comparison / falsification test

The two controls bound the method. A title carrying Storm/Echo would show `storm::echo::object` the way
the ACU view does. A packed title shows up as unknown.

### Reproduction steps

Run the command above on the same files and compare `present` and `counts` with the table.

### Implementation implications and finding-linked constants

- Porting plan: for every installed title, co-op means our own replication layer plus engine glue
  (option C), using ACU's NetProtocol/Echo design (ACU-RE-0072 and 0073) as the specification. Survey
  Syndicate first if it becomes available.
