# ACU-RE-0078: Where does ACU's shared network clock come from?

Date / investigator / related task: 2026-11-15 / Claude (Opus 5.5), static only / #85, co-op plan Phases 2–4 ([COOP_RE_PLAN.md](../COOP_RE_PLAN.md))
State: partially established. The Storm session-time protocol (host origin, ping exchange, averaging,
slewing, monotonic reads) and the game-side fallback are observed. The link from the game's clock
getter to Storm's `GetSessionTimeInMs` is inferred. The system clock source is not identified.

## Provenance

Binary path / SHA-256 / image base / VA / RVA: same capture and analysis view as ACU-RE-0072 to 0077:
mapped image `9a1124f1…0520`, analysis view `47a81dc9…d8c2`, image base `0x140000000`.
Corpus path and lines / export identity status: not used.
Archive identity / byte range / range hash / file ID (hex string): not applicable.
Stored and decoded lengths / SHA-256: not applicable.
Tool/provider/version / evidence IDs or saved output: IDA Pro 9.3, ida-pro-mcp idalib, headless session
`acu85`, on the private IDB copy. Read only, closed without saving, lock #31 held.
Exact commands and search boundary:
- Xrefs to the clock globals `qword_145212F48` and `qword_14521E148`.
- The string search `GetSessionTime|time::inFunc|echo::time`, then the type getters of those records and
  their callers. All callers of `sub_141BF58E0` and `sub_141C2ADC0` were read.

## Result

### Observations

1. **Game-side clock family.** `sub_141F89950` (the proxy clock, ACU-RE-0077), `sub_141F88550` (seconds),
   `sub_141F884C0`, `sub_141F890D0` and `sub_141F8C7D0` (timer helpers) share one pattern:
   - If `qword_145212F48` exists and virtual `+0x00` of its sub-object at `+0x20` returns true, they use
     that sub-object's virtual `+0x118` (milliseconds, u32).
   - Otherwise they use `*(u64*)(sub_141CD0D50() + 8) + qword_14521E148`, clamped at 0.
     `sub_141CD0D50` returns the game clock block: `qword_145217368->+0x98 + 0x7D8` if present, else
     `qword_14525D9D0`.
2. **Continuity when leaving the network.** `sub_141F8DEA0` sets
   `qword_14521E148 = net_time − local_clock`. Two places call it:
   - the network-session update and teardown path `sub_141C450C0`, before it resets session state;
   - `sub_141C63390`.

   Both are guarded by `dword_144E52380`. Game time therefore continues from the network value instead of
   jumping back to the local clock.
3. **The Storm time module** has these records:
   - `storm::echo::time::inEvent::{SetSessionTimeInMs, PauseSessionTime, StepSessionTime}` (getters
     `sub_141B7AA50`, `sub_141B77950`, `sub_141B7AC80`);
   - `inFunc::{GetSessionTimeInMs, GetSystemTimeInMs}` (getters `sub_141B764F0`, `sub_141B76590`).

   The inFunc dispatcher `sub_141BFE0E0` answers them as follows:
   - `GetSystemTimeInMs` reads the controller's clock object (`+0xB8`, virtual `+0x08`);
   - `GetSessionTimeInMs` goes through `sub_141C05D20` to `sub_141BF58E0` on the per-session time record.

   The inEvent dispatcher `sub_141BFE200` routes `SetSessionTimeInMs` to `sub_141C17DA0`, Pause to
   `sub_141C0C3F0` and Step to `sub_141C197C0`. Step adds its delta to record `+0x18` and `+0x28`.
4. **Per-session time record** (one per session index, at `controller+0x38` → `+0x18 + 8·index`):

   | Offset | Field |
   |---|---|
   | `+0x08` | applied offset |
   | `+0x0C` | target offset |
   | `+0x10` | last time slewed |
   | `+0x14` | time of the last set |
   | `+0x18` | last returned session time |
   | `+0x2C` | fractional slew carry |
   | `+0x30` | lock |
   | `+0x54` | initialised |

   Setting the time (`sub_141C2ADC0` from protocol code, or `sub_141C17DA0` for the event) with value
   `T` at system time `s` sets the target offset to `T − s`. The first set also applies it immediately
   and marks the record initialised.
5. **Reading session time** (`sub_141BF58E0`). Before initialisation it returns `+0x18`. Otherwise, at
   system time `s`:
   - it first slews the applied offset toward the target (`sub_141BE50C0`), then returns
     `max(previous, s + offset)`, so session time never goes backwards;
   - the step is `(s − last)·rate` ms with `rate = max(1/30, err²·4e-8)`, where `err` = target − applied,
     in ms;
   - positive steps are capped by a per-session setting (`config + 0x68`), and no step overshoots the
     target;
   - the fraction is carried in `+0x2C`.
6. **Host origin.** In the periodic time task `sub_141C22AD0`, a peer that is the session host
   (`sub_141B84B80`: the peer id equals host id `+0x478` or `+0x44`) initialises its record with session
   time 0 if it is not already set. `sub_141C04C10`, a session create path, also seeds the record with a
   caller-supplied value.
7. **Ping exchange** on a peer channel, message kind 7. All u32 fields are big-endian.

   | Message | Fields |
   |---|---|
   | Request (non-host → host) | `u32 id`, `bool 1`, `u32 client_system_ms` |
   | Reply (host → requester) | `u32 id`, `bool 0`, `u32 host_session_ms` (`sub_141BF5360`), `u32 echoed_client_system_ms` |

   - Requests are sent every **100 ms** until 10 replies have arrived, then every **2000 ms**
     (`sub_141BBD270`). The host builds its reply in `sub_141BF6B70`.
8. **Client estimate** (`sub_141BF6B70`, reply branch). At receipt, with `now` = client system ms:
   - it requires `now ≥ echoed`;
   - per sample, `estimate = host_session_ms + (now − echoed)/2`, which adds half the round trip;
   - it sums the estimates and counts them;
   - once `now` passes the next apply time, it calls `sub_141C2ADC0` with the integer mean. That sets the
     target offset against the system time at the moment of applying. It then clears the sums and sets the
     next apply time to `now + 500`.
   - It does not correct for the time between taking a sample and applying it.
9. **No backend crossing.** The time protocol uses only peer-channel messages between session members.
   No ubiservices, RDV or punch call appears on this path.

### Inferences and alternatives

- **The game's virtual `+0x118` probably returns Storm `GetSessionTimeInMs`** for the active session, and
  `sub_141F8DEA0` latches it at teardown. This is not proven: the sub-object at `qword_145212F48 + 0x20`
  lives on the heap (ACU-RE-0073), so its vtable cannot be read from this capture. A live probe can
  resolve its `+0x118` target.
- **The proxy timestamps of ACU-RE-0077 are host-relative session milliseconds.** So a replica's
  extrapolation age is `now − ts` measured on the host's timeline, and its error is about the residual
  offset after slewing.
- The averaging window in observation 8 averages samples taken across 0.5 s or more but applies them at
  one instant. A port that copies this keeps ACU's behaviour; one that corrects the timing will produce
  slightly different offsets.

### Unknowns / incomplete coverage

- The class and virtual `+0x118` of the game's net object.
- What the controller's system clock (`+0xB8`, virtual `+0x08`) reads, for example a QPC- or
  timeGetTime-derived value.
- The value of the per-session positive slew cap (`config + 0x68`).
- When `PauseSessionTime` and `StepSessionTime` are issued, and by whom.
- How a host migration re-seeds session time.

### Independent comparison / falsification test

The request writer (`sub_141C22AD0`) and the reply parser (`sub_141BF6B70`) agree field by field: id,
bool, then one or two big-endian u32s. The host reply echoes the requester's timestamp, which the
requester then subtracts from its own clock. This is the standard symmetric round-trip estimate; a
one-way design would not echo. A live check would log `sub_141C2ADC0` arguments on a client and expect
values that settle within a few ms of the host's `sub_141BF5360` result.

### Reproduction steps

1. Decompile `0x141F89950`, `0x141F8DEA0`, `0x141C450C0`, `0x141BFE0E0`, `0x141BFE200`, `0x141C05D20`,
   `0x141BF58E0`, `0x141BE50C0`, `0x141C17DA0`, `0x141C197C0`, `0x141C2ADC0`, `0x141C22AD0`, `0x141BF6B70`
   and `0x141B84B80`.
2. Find the strings at `0x143002700` to `0x1430028E0`. Read only.

### Implementation implications and finding-linked constants

- Our transport needs a host-origin session clock: the host starts at 0, and each client pings it with
  `{id, request flag, client_ms}`. The reply is `{id, reply flag, host_session_ms, echoed client_ms}`.
  - Each sample estimates host time as `host_session_ms + rtt/2`.
  - Samples are averaged and applied at most every 500 ms.
  - Pings go every 100 ms for the first 10 replies, then every 2 s.
- Apply corrections by slewing: rate `max(1/30, err²·4e-8)` ms per ms, never overshooting. Make reads
  monotonic with `max(previous, s + offset)`.
- When a session ends, latch `offset = net_time − local_time` so game time stays continuous.
