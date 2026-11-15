# ACU-RE-0088 — How ACU builds the online error code `[0xb0040001]`

Date / investigator / related task: 2026-11-15 / claude-session, static plus one live hook run / #85, co-op plan
Phase 2 ([COOP_RE_PLAN.md](../COOP_RE_PLAN.md)); follows [ACU-RE-0087](ACU-RE-0087-lan-path-static.md) dynamic addendum
State: partially established.
- Established (static): how the bracketed code is built, the error-category registry, and the table that maps
  error keys to message IDs.
- Established (live): the error record is `{category 3 (OSDK), code 0x80040001}`, delivered through the async-job
  completion path below. It maps to message `544321`.
- **Corrected:** the static pass read the code as category 11. That was wrong: the code word's own top bit
  is set, so `(3 << 28) | 0x80040001 = 0xB0040001` (see the live section).
- Not traced: where the job's OSDK result is produced.

## Provenance

Binary path / SHA-256 / image base: installed `ACU.exe` SHA-256 `748abdc6…27e40`. Addresses come from the
hash-bound runtime view: mapped image SHA-256 `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`,
base `0x140000000`. Function names (where any) come from `local/symbol_index.sqlite` (tool SHA-256 `9d6ef9d9…`).
Corpus path and lines: `ACU - Decompiled/<VA>.c`, whole file, for each function named below. The export
identity follows ACU-RE-0031 (header and resource binding only).
Tool/provider: Python scans of the mapped image:
- RIP-relative `disp32` references, assuming no trailing immediate;
- `E8`/`E9` rel32 calls;
- absolute 8-byte pointers;
- string searches in ASCII and UTF-16.

Also the symbol-index tables `call`, `name` and `function`. No IDA or REA session. The RIP-relative scan can
miss references whose instruction has an immediate after the displacement.
Archive identity: none (the message text is localized data that is not decoded here).

## Result

Observations:
1. **The code is not a constant.** Neither `0xB0040001` nor `0x00040001` appears as an immediate in game code.
   The four byte matches for `0xB0040000` are not code constants. The two `0x40001` stores are geometry index
   tables (`0x142402BA0`, `0x1424503E0`). The game DLLs in the install folder contain neither value.
2. **The bracket is appended by one function.** The UTF-16 format `L" [0x%08x]"` at `0x14300F340` has one
   reference, `0x141C551C0`. It does three things:
   - looks up the localized message by ID (`0x141DFDCE0`, on the string table `qword_145219CF0`);
   - computes `(record[0] << 28) | record[1]` from an error record `{u32 category; u32 code}` (`0x140128290`
     returns `*rec`; `0x141F88530` returns `rec[1]`);
   - appends the result.

   So `0xb0040001` means a category whose low four bits are `0xB`, with code `0x0040001`.
3. **The message table is a singleton.** `0x141C551C0` is slot 1 of vtable `0x14300F320`, which belongs to the
   object at `qword_1451C5FE0`. Its constructor is `0x141C3BD30`, called from `0x141CDD550` (object-model
   start-up). The other slots:
   - slot 2 (`0x141C55240`) runs slot 3 and then slot 1;
   - slot 3 (`0x141C4FB00`) gets a key from the record (`0x14275B200`), then looks it up in the hash map at
     `+8` to get a message ID.
4. **Error categories are a registry.** `qword_145218F38` (built by `0x1427473E0`) holds category objects, with
   the ID at `+8` and the name at `+16`. `0x14275B200` finds the category whose ID equals `record[0]`. If none
   matches, it uses ID `-1`. It then returns `category->vf9(record[1])` as the key. The registered categories:

   | ID | Name | Constructor | Code→key mapper (vf9) |
   |---|---|---|---|
   | -1 | Invalid | `0x142747750` | `0x140076B00` returns 1 |
   | 0 | System | `0x142748AB0` | `0x14209B110` returns the code |
   | 1 | Win32 | `0x142747DD0` | `0x140076B00` returns 1 |
   | 2 | Online | `0x1424A6180` | `0x14209B110` returns the code |
   | 3 | OSDK | `0x1424AC880` | `0x1424F17A0`, maps `0x8001xxxx`… ranges |
   | 6 | SystemInternal | `0x142748BC0` | `0x142764860` |
   | 7 | UbiServices | `0x1424B2240` | `0x1424F1C20`, maps small codes, for example 1–7 → `0x100106` |
   | 8 | OnlineInternal | `0x1424A63B0` | `0x1424F1630` |
   | 9 | Hermes | `0x1424A4EC0` | `0x1424F14E0` returns `code != 0` |

   Sources: the four built-in categories are constructed inline. The other five are registered through
   `0x1427473A0`. A full rel32 scan finds exactly those five callers (above). Of the 8 RIP-relative references
   to the base category vtable `0x14363F300`, 5 are constructors; the other 3 (`0x14274A0F0`, `0x14274B1D0`,
   `0x14274C3D0`) set no ID. **No category with ID 11 (or low nibble `0xB`) was found.**
5. **The key → message table is filled in one place.** `OnlineUniverseComponents::vf16` (`0x140F33EC0`) calls
   `0x141C68870(table, key, messageId)` 32 times:
   - key `0` → `-1` (no message, success);
   - keys `1`, `-1` and `-2` → message `544619` (`0x84F6B`);
   - keys `0x100100`–`0x100106` → `544317`, `603885`, `603884`, `544320`, `544319`, `544321`, `544321`;
   - keys `0x100200`–`0x100206` → `544320`;
   - keys `0x100300`–`0x100303` → `544321`;
   - keys `0x100400`–`0x100409` → `544624`–`544633` and `603883`.

   Unmapped codes also fall back to key `1` (most mappers' default) or `-2` (UbiServices, for example).

Static inference, **falsified by the live run below**: the static pass assumed the code word has no high bits,
read `0xb0040001` as category 11, and inferred a fallback to message `544619`. The formatter ORs the category
into the top nibble without masking the code, so a code with bit 31 set changes that nibble.

## Live run — hook on the formatter during Play in Public (2026-11-15)

Provenance: installed `ACU.exe` SHA-256 `748abdc6…27e40` (checked at attach), loaded at preferred base
`0x140000000`. Original Ubisoft-signed loader active. Ubisoft Connect ran non-elevated, with its overlay off.
Launched with `capture_run.py local/native-probe/runs/coop-errhook-20261115 --until world --fps-cap 60
--allow-user-input` on the user's own save (PID 124224, `capture.json` `ac437c4c…`). The focus shim was installed
with input blocking off, so the user drove the game. Native probe, two execute hooks with expected entry bytes:
- slot 0 `0x141C551C0` (`48895c240848896c2410488974241857`), 256 bytes at RSP;
- slot 1 `0x141C55240` (`48895c24104889742418574883ec2048`), 8 bytes at RDX.

Log: `errhook.jsonl` `669d773b…` (machine-local, not distributed); hooks removed afterwards. The user pressed
Play in Public on a co-op mission marker twice (21:41:18 and 21:41:35 local time). Each press showed the
`[0xb0040001]` popup and produced one hit per hook (4 hits, 0 dropped, 0 thread conflicts).

Observations:
6. **The record is `{3, 0x80040001}`.** Both presses read 8 bytes `03 00 00 00 01 00 04 80` at RDX. Category
   3 is `OSDK` (observation 4). `(3 << 28) | 0x80040001 = 0xB0040001` matches the popup.
7. **It maps to message `544321`.** OSDK's mapper `0x1424F17A0` returns key `0x100105` for `0x80040001`
   (`a2 == -2147221503` → `1048837`). Key `0x100105` maps to message `544321` (observation 5), the same
   message as keys `0x100106` and `0x100300`–`0x100303`. Inference: `544321` is the "required Ubisoft services
   are currently unreachable" text. The popup showed that text, but the string table was not decoded.
8. **The call chain is the same on both presses** (two different threads, 9812 and 124872). Return addresses
   recovered from the 256 sampled stack bytes, which are not an unwound stack:

   | Depth | Return address | In function | Role (from the corpus) |
   |---|---|---|---|
   | 0 | `0x141C55293` | `0x141C55240` | message-table slot 2: look up, then format |
   | 1 | `0x14275AA54` | `0x14275AA20` | calls the message table `qword_14525DA18` with default message `544619` |
   | 2 | `0x14250465E` | `0x1425045A0` | job-completion callback (below) |
   | 3 | `0x142737E98` | `0x142737E50` | marks the job done (`state = 2`), stores the result at `+8`, runs the callback at `+48` |
   | 4 | `0x1425713C1` | `0x1425712A0` | finishes a job with a result record: removes it from the manager's list at `+216`, then calls `0x142737E50` |

   `0x1425045A0` reads the job's result record at `+8`. It looks up the job's facility selector, kept at
   `+88` in the pending list `qword_1452525F0` (`0x14250C8A0`), and resolves it through `0x1424E01A0`,
   which picks one of ten singletons. Success requires all three:
   - the result is OK (key `0`);
   - that object's vf8 returns true;
   - its field `[38]` equals `3`.

   Otherwise it builds the message (default `544619`) and posts it into a 40-byte UI event ring at
   `unk_145221A30`.
9. **Four functions can finish a job** (direct calls to `0x1425712A0`): `0x1424EFD20`, `0x142514D60`,
   `0x142561540` and `0x142571210`. The deepest captured frame stops inside `0x1425712A0`, so which one ran is
   not observed.

Inferences:
- The co-op Public flow fails in an OSDK request. The result comes back as an OSDK error, and the async-job
  framework delivers it to the UI. The name `OSDK` and its `0x80xx0001`-style codes suggest Ubisoft's PC
  client SDK (the uplay loader's API), but that is not established. Under the backend rule (B1), this is
  recorded as a backend boundary and not decoded further.
- Every flow the user tried stops at this OSDK error before any Storm session exists (ACU-RE-0087).

Unknowns:
- Which of the four callers finished the job, which OSDK request it was, and the facility selector at `+88`.
- What the backend would hand Storm on success (peer addresses, session ID). That is still the target, so
  our own lobby can supply it.

Next live step (read-only): hook `0x1425712A0`'s entry. Read the result record at `r8` and the job at `rdx`
(selector at `+88`), and take the return address to see which caller ran. Then trace that caller's request.

Reproduction steps:
```powershell
# UTF-16 " [0x%08x]" at 0x14300F340; its single RIP-relative reference is in 0x141C551C0
# vtable 0x14300F320 slot 1 = 0x141C551C0; singleton qword_1451C5FE0
# category registry users: references to 0x145218F38; constructors: rel32 calls to 0x1427473A0
# key->message table: ACU - Decompiled/140F33EC0.c lines 95-126 (32 sub_141C68870 calls)
```

Implementation implications and finding-linked constants:
- Our own lobby only has to reach the point before this check, or suppress it. The bracketed code is a
  diagnostic, not part of the transport.
- Constants: `0x141C551C0`, `0x14300F320`, `qword_1451C5FE0`, `qword_145218F38`, `0x14275B200`,
  `0x140F33EC0`, OSDK mapper `0x1424F17A0`, job path `0x1425712A0` → `0x142737E50` → `0x1425045A0`, and
  message IDs `544321` (this popup), `544619` (default) and the others above.
