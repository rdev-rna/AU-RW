# ACU-RE-0021 — Virtual controller enters world and XInput polling is observed

Date / investigator / related task: 2026-10-19 / Codex / ACU-008 controller-first follow-up
State: partially established

## Provenance

Binary path / SHA-256 / image base / VA / RVA:
- `D:/Games/Assassin's Creed Unity/ACU.exe`, SHA-256
  `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, runtime
  and preferred base `0x140000000`.
- XInput API sites in the live process (system modules, not ACU functions):
  `XINPUT1_3.dll` SHA-256
  `60cd2bcbb97f3a51892ff55ed6e93b2ee0955eb913031dacde885b50e7095dac`, base
  `0x00007fff742d0000`, `XInputGetState` VA `0x00007fff742d4ee0`, RVA `0x4ee0`,
  expected entry bytes `e9eec0feff574883ec20488bda8bf9e8`;
  `xinput1_4.dll` SHA-256
  `cc119f987bf33731fcd22c3a9bbcc675f31d80593a4c26428b363d6eedef974b`, base
  `0x00007fffafd00000`, VA `0x00007fffafd01750`, RVA `0x1750`, expected entry
  bytes `e97ef75bc4565741564883ec30488bfa`.
- Captured mapped ACU image SHA-256:
  `933e90e745db8a2b0fcf6e91bd32badf0f4763f21aa3eb6053220da48f6b492d`.

Corpus path and lines / export identity status: the live image inventory at
`local/native-probe/controller-sweep-20261019/image-inventory/summary.json`
reported 263,663 unwind ranges. `decompile_xref.py` joined 161,063 exact starts,
319 addresses inside ranges, and 55,659 filename candidates without an unwind
match; 161,087 native ranges received at least one candidate name. These are
address joins only. Export-to-installed-image identity and function semantics
remain unverified. XInput sites belong to Windows DLLs and are not matched to
the ACU corpus.

Archive identity / byte range / range hash / file ID: not applicable.
Stored and decoded lengths / SHA-256: not applicable.
Tool/provider/version / evidence IDs or saved output: local `acu_probe.dll`
SHA-256 `4b0bbf8bd4fc88110d65485b0cd680a3e6c3f4267254b5e624e7b0f6db441373`;
ViGEmClient tag `v1.16.18.0`, commit
`9e91a124d179bf26a878a952153042ac871da243`, DLL SHA-256
`cbb99ae33c8e5d6cc3a025c3fd6458a23003f9f46b03e02473befb8d231f85f3`;
Python 3.12, local native probe and `tools/native_probe/xinput_profile.py`.
Evidence is preserved under ignored `local/native-probe/`:
- Menu input timeline SHA-256
  `654ab602bdd28cdc3beecdeb4346c1cfe6b7d23f4ffc723145078ad99ef68741`.
- Full sweep timeline SHA-256
  `abfd7fa3aa54d150a2d6e3573ef13d1611855bcd215e6c59a3f490b08cbdf2ef`.
- XInput session trace SHA-256
  `0551bc2f2f0e26d46fc26f5643530c855aa2eab3fbb379fce3b397a3668a1ef2`.
- Pre-sweep world screenshot SHA-256
  `0e8afd971f7b49b07abde3578db4259300e35d8e2c3d74ca60fb7bf27b5813b4`;
  after-sweep Skills-screen screenshot SHA-256
  `7bc9e8f4ecd0d6e8f5e9aa5fe7d4fecc7f32790c7d5249caef72de4c98ee81bb`.

Exact commands and search boundary: the repaired game was launched through
Ubisoft Connect, PID 25068, then attached with
`acu_probe_launcher.exe --attach 25068 --dll local/native-probe/acu_probe.dll`.
`tools/native_probe/menu_then_world.example.json` sent A twice (180 ms each),
with 4 s and 5 s menu waits. A world screenshot about 30 s after the second A
showed Paris, Arno, crowd and gameplay HUD. The first route replay later stopped
when a screenshot command changed foreground; the controller was released by
the runner. `controller_sweep.example.json` completed all 29 states. A second
full sweep ran with a generated `HOOKVA` profile for both loaded XInput modules.
The live `MODULES` response, local export resolution and a 16-byte remote `READ`
were used to derive and verify each hook RVA and byte guard.

## Result

Observations: ViGEmClient created and released a virtual X360 at assigned slot 0.
The menu route entered gameplay on this build. During the full sweep, the probe
recorded 4,243 `XInputGetState` entry events: 1,806 at XINPUT1_3 and 2,437 at
xinput1_4. RCX user indices were 0: 2,521 calls; 1: 1,714; 2: 4; 3: 4. The
probe reported 84 dropped/overwritten events. The game stayed open and the
after-sweep screenshot showed its Skills UI.

Inferences and alternatives: ACU polls XInput slots 0 and 1 during gameplay and
the virtual device is visible to the game at its assigned slot. The two-A
sequence is a practical route from the tested title/menu state. The last UI
screen is consistent with sweep inputs reaching menu handling, but no frame-by-
frame screen capture ties it to a specific button.

Unknowns / incomplete coverage: Entry hooks run before XInput writes the output
state, so the trace does not prove the values returned for each virtual report.
It does not establish ACU action bindings, camera movement, traversal/raycast
behavior, bone updates, or which calls belong to Arno. Only `input` currently
has a verified category profile. The first concurrent full-memory attempt
stopped at 1.9 GB after pipe contention. A standalone retry produced a 2.57 GB /
5,736-file partial artifact, then stopped during a user-directed turn
interruption; its manifest is marked partial. All captures are sequential and
non-coherent.

Independent comparison / falsification test: run the same sweep with the
virtual controller assigned a different XInput user index and compare the RCX
histogram; disconnecting the virtual target should leave polling events but
remove the report source. A return-site capture is required to compare actual
`XINPUT_STATE` values with the input timeline.

Reproduction steps: keep the matching game build open; build the native probe;
generate `local/native-probe/live-xinput-profile.json` with
`python tools/native_probe/xinput_profile.py <PID> local/native-probe/live-xinput-profile.json`;
run `controller_autoplay.py <PID> tools/native_probe/controller_sweep.example.json
local/native-probe/replay --expected-sha256 <ACU.exe SHA256> --probe-profile
local/native-probe/live-xinput-profile.json`; analyze the mapped image with
`native_image.py`, then join names using `decompile_xref.py` and the local corpus.

Implementation implications and finding-linked constants: the menu route uses
45 s as a conservative loading wait (world was visible by about 30 s here).
`xinput_profile.py` resolves exports against each live module and byte-guards
the live RVA; no executable bytes are changed. Other category checkboxes remain
unmapped until equally reproducible native sites are found.
