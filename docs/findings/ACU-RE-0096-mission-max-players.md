# ACU-RE-0096 — Where is a co-op mission's player limit, and can it go above 4?

Date / investigator / related task: 2026-11-16 / Claude (Opus 5.5), static, IDA 9 headless on a plugin DLL / #85
State: partially established. The per-mission limit is a data field, and the plugin's range and defaults are read.
Whether ACU accepts more than 4 players is not established.

## Provenance

Binary path / SHA-256 / image base / VA / RVA: `VisualCustomization.dll`, the user's own ACU plugin (March 2026
build, from the old `ACUFixes/plugins/` folder in the game directory; not loaded by the current install). SHA-256
`2629abc5cdabb5c2399055c1ae8e2648741ebbe7f298063fd65431a0d9ff7a4d`, image base `0x180000000`. Analysed from a copy in
the session scratchpad. The older `VisualCustomization.dll.off` (`4ea8bc51…`) has no such option.
Corpus path and lines / export identity status: not used.
Archive identity / byte range / range hash / file ID (hex string): not applicable.
Stored and decoded lengths / SHA-256: not applicable.
Tool/provider/version / evidence IDs or saved output: IDA Pro (idalib, Hex-Rays) through the ida-pro-mcp plugin.
Exact commands and search boundary: the string "Max players for multiplayer missions" (`0x1800B1F20`), its only
reference in `sub_180018650`, the slider helper `sub_18003AF40`, and the reset tables at `0x1800BC5F0`–`0x1800BC6FF`.

## Result

### Observations

1. **One slider per mission, range 2–4.**
   - `sub_180018650` draws the "Max players for multiplayer missions" panel: 20 missions in three groups.
     - Paris co-op: indices 0–10.
     - Paris heists: 11–17.
     - Dead Kings: 18–19.
   - Each slider calls `sub_18003AF40(label, 4, p_value, &min, &max, "%d")`. That matches ImGui's `SliderScalar`
     with data type 4 (signed 32-bit). Min is 2 and max is 4.
   - The slider writes straight into game memory.
2. **The value is the mission record's field at `+0x120` (288).**
   - **Paris missions (0–17):** the address is `<table> + off[i]`. The table is reached from the game global
     `0x145260780` through the chain `+192 → +36 → +8 → +84 → +8 → +112 → +24`. The `u16` offsets `off[i]` are
     `0x120`, `0x500`, `0xB70`, `0xE70`, `0x11E0`, `0x1550`, `0x19A0`, `0x1D10`, `0x2080`, `0x23F0`, `0x2970`,
     `0x2C00`, `0x2E20`, `0x3040`, `0x3260`, `0x3480`, `0x36A0` and `0x38C0`.
   - **Dead Kings missions (18–19):** the address is `*(<table> + off[i]) + 288`. The table is reached from
     `0x145219250` through `+16 → +24 → +40 → *+24 → +40`, with `off[i]` = `0` and `8`.
   - ACU-RE-0092 found the map's player-count check reading a mission's count at `+288` from the mission database
     (lookup `0x1410968F0`). This is the same field.
3. **Reset values, which are probably the game's originals.** "Reset##MaxPlayers" writes:

   | Max | Missions |
   |---|---|
   | 2 | Heads Will Roll, The Infernal Machine, **The Austrian Conspiracy** (our testbed mission `COOP_RB050`), Jacobin Raid, Moving Mirabeau, Tithing Templars, Catacomb Raider, The Party Palace, Last Rites, Holy High Rollers |
   | 4 | The Food Chain, Danton's Sacrifice, Political Persecution, Les Enrages, The Tournament, Women's March, Royals, Guns and Money, Smuggler's Paradise, Ancient History, It Belongs in a Museum |

### Inferences and alternatives

- **The player limit is per-mission data, read before hosting.** Raising it to 4 lets a 2-player mission be
  hosted for 4. The networking layer is not changed.
- **4 is probably the session's hard limit.** The plugin's slider stops there, and nothing found so far raises the
  Storm or party session size. The alternative, that the author simply chose 4, is not excluded. It needs a test:
  write 5 into the field and try to join a fifth game, or find the party/session maximum in code.
- **Danton's Sacrifice is a 4-player mission; The Austrian Conspiracy (our testbed) is a 2-player one.** Testing 3
  or 4 players needs either a 4-player mission or the raised field.

### Unknowns / incomplete coverage

- Whether ACU's party, session or avatar code accepts more than 4 peers, and where that limit lives.
- Whether the reset values are the game's originals (not compared against the live field yet).
- What the rest of the mission record holds.

### Independent comparison / falsification test

- The `+288` field agrees with ACU-RE-0092's static reading of the map's player-count check. That reading was made
  without this plugin.
- Live check: on the testbed, read the field for `COOP_RB050` through the chain above before any write (expect 2),
  and compare the other missions with the reset table.

### Implementation implications and finding-linked constants

- **For a port:** keep the player limit as per-mission data (2 or 4 in ACU), separate from the transport's peer
  limit. The library allows 8 peers (`kMaxPeers`). Missions decide how many may join.
- **Modern improvement:** scale encounter difficulty and spawn counts with the player count, instead of a fixed
  per-mission cap.
- Constants: field `+0x120` of the mission record; roots `0x145260780` (Paris) and `0x145219250` (Dead Kings);
  ACU-RE-0092 lookup `0x1410968F0`.
