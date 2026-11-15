# ACU-RE-0088 — How ACU builds the online error code `[0xb0040001]`

Date / investigator / related task: 2026-11-15 / claude-session, static only / #85, co-op plan Phase 2
([COOP_RE_PLAN.md](../COOP_RE_PLAN.md)); follows [ACU-RE-0087](ACU-RE-0087-lan-path-static.md) dynamic addendum
State: partially established.
- Established: how the bracketed code is built, the error-category registry, and the table that maps error
  keys to message IDs.
- Inferred: that category 11 is unregistered and falls back to the generic message.
- Not traced: which code produces the category-11 error.

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

Inferences:
- Category 11 has no registered handler, so the lookup falls back to `Invalid`, whose vf9 returns `1`. Key `1`
  maps to message `544619`, the generic fallback. That fits the generic text the game shows ("required Ubisoft
  services are currently unreachable"). The text of `544619` is not decoded here, so this link is an
  inference, not an observation.
- The error record is created at runtime, by code or data this pass did not find. That could be an
  unregistered facility, or a value passed through from the Ubisoft loader or SDK.

Unknowns:
- Which function writes the record `{11, 0x40001}`, and what step of the connect flow it reports.
- What the backend would hand Storm on success (peer addresses, session ID). That is still the target, so
  our own lobby can supply it.

Independent comparison / falsification test (live, no connection needed beyond the failing attempt): put an
execute hook on `0x141C551C0` (expected entry bytes to be read from the image), then press Play in Public.
- At the hit, read the record at `rdx` (expect `{0x0000000B, 0x00040001}`) and walk the return-address chain.
- The callers name the code that raised the error, which is the connect step.
- A different record, or no hit, would falsify observations 2–4 for this popup.

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
  `0x140F33EC0`, and message IDs `544619` and the others above.
