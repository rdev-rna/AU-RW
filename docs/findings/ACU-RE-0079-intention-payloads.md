# ACU-RE-0079: What does each NetHumanIntention message carry?

Date / investigator / related task: 2026-11-15 / Claude (Opus 5.5), static only / #85, co-op plan Phases 3/4 ([COOP_RE_PLAN.md](../COOP_RE_PLAN.md))
State: partially established. The field layout (slot, value type, source offset) of all 94 intention
messages is extracted from the message builder. The field meanings and the exact wire bit layout are not
established.

## Provenance

Binary path / SHA-256 / image base / VA / RVA: same capture as ACU-RE-0072 to 0078: mapped image
`9a1124f1…0520`, image base `0x140000000`.
Corpus path and lines / export identity status: not used.
Archive identity / byte range / range hash / file ID (hex string): not applicable.
Stored and decoded lengths / SHA-256: not applicable.
Tool/provider/version / evidence IDs or saved output:
- Capstone 5.0.3, offline on the mapped image. IDA was not used: `idb_open` timed out twice (30 min each).
- `tools/net_intention_payloads.py` (SHA-256 `fa63a105…e6e0`) reads the image and
  `local/coop/net_protocol_atlas.json`, and writes `local/coop/intention_payloads.json`
  (SHA-256 `cbd8ebb6…bff2`). Two runs produced identical output.
- The synthetic tests are `tools/test_net_intention_payloads.py` (7 tests).

Exact commands and search boundary:

```
python tools/net_intention_payloads.py --image <mapped image> --atlas local/coop/net_protocol_atlas.json --output local/coop/intention_payloads.json
```

Every `NetHumanIntention_*MSG_NetProtocol` type record is included (94). Each block is located by its
broadcast wire ID imm32 (the ACU-RE-0075 rule).

## Result

### Observations

1. **One builder for all intentions.** A single function, split into three `.pdata` chunks
   (`0x14071F090`, `0x1407200CF`, `0x140720497`), has one block per message. Each block does the
   following:
   - It hashes `"<prefix><Msg>_NetProtocol_ID"` and compares the hash with the source intention's id at
     `+0x8` (register `rbx` in every block).
   - It creates the message object, then stores the unicast or broadcast wire ID. The choice is
     `test r12b` → unicast.
   - It copies the intention's fields into the message's slots, then jumps to the common exit
     `0x140726983`.

   The tool finds exactly one block for each of the 94 messages.
2. **Two copy styles, same slot format.** Each slot is laid out as {changed-flag pointer `+0x08`,
   changed byte `+0x10`, value `+0x18`}.
   - **Form A (35 messages):** inline code, after `mov dword [obj+8], wire_id`. It loads the source,
     compares it with the slot value, sets changed, and stores. Several messages jump to a shared tail.
   - **Form B (59 messages):** after `mov edx, wire_id` and `call 0x14018DE90`, it calls typed setters
     with `rcx` = slot and `rdx` = source.

   Setter types:

   | Setter | Type | Change rule |
   |---|---|---|
   | `0x1401D7650` | u32 | value differs |
   | `0x14018C150` | u64 | value differs |
   | `0x140783190` | u8 | value differs |
   | `0x14012AA10` | bool (source byte ≠ 0) | value differs |
   | `0x14012A9E0` | f32, raw | changes by ≥ 0.001 |
   | `0x1401D7670` | three raw f32 (vec3) | changes by ≥ 0.001 |
   | `0x14018C1D0` | vec3, each component quantized over ±4000 (`0x14018C060`) | changes by ≥ 0.001 |
   | `0x140782B00` | f32 quantized over ±7 | changes by ≥ 0.001 |

   `0x14043A1A0` and `0x1401518A0` turn an entity pointer into a u64 handle before it is stored.
   Hand check: the AssassinateTarget block (`0x140721F83`) decodes to the same eight fields as a manual
   disassembly. They are `{u32 +0x8, u8 +0x14, u64 +0x38, u64 +0x50, u64 +0x68, u32 +0x78, bool +0x7C,
   u8 +0x7D}`, in slots `0x38` to `0x118`. The resync slots of ACU-RE-0075 observation 7 (`+0xF0`,
   `+0x110`, `+0x130`) are the value offsets of three of these slots.
3. **Common header.** All 94 messages start with slot `0x38` = u32 message id (source `+0x8`) and slot
   `0x58` = u8 (source `+0x14`). 25 messages carry only this header, for example `ExitCover`,
   `CoverChangeHeading` (checked by hand), `ResumeNavigation`, `CancelLookAt` and `StartSmoked`.
4. **Catalogue totals:**
   - 549 fields: 193 u32, 96 u8, 83 bool, 65 u64, 53 vec3, 37 f32, 11 entity handles, 6 bit flags taken
     from one source byte, 4 vec3 quantized over ±4000, and 1 f32 quantized over ±7.
   - 515 fields read directly from the intention object. 34 are "derived", meaning they come through
     another pointer or a computed value; most are u64 entity handles.
   - Field count per message: 2 to 29. The navigation messages are the largest:
     - `NavigateToPatrolPoint` has 29 fields, including six vec3s and five f32s;
     - `NavigateToNavTarget` has 25;
     - `PlayCustomActionWithParams` has 28.
5. **Examples** (header omitted; `@` gives the source offset):

   | Message | Fields |
   |---|---|
   | `NavigateToEntity` | u64 (derived), u32@0x40, vec3@0x50, u32@0x60, bool@0x64, bool@0x65 |
   | `FollowEntity` | u64 (derived), u32@0x40, u32@0x44, bool@0x48, bool@0x49, vec3@0x50, bool@0x60, bool@0x61 |
   | `Orientate` | vec3@0x30, f32@0x40 |
   | `LookAtRequest` | u32, u32, u64 (derived), vec3_q4000@0x50, vec3_q4000@0x60, u32, u64, u32, f32, f32_q7@0x88, u32 |
   | `EnterCover` | vec3@0x30, vec3@0x40, f32@0x50, vec3@0x60, f32@0x70, f32@0x74 |
   | `AimAtTargetWithWeapon` | u32@0x28, u64 (derived), bool@0x48 |
   | `FireRangedWeapon` | u64 (derived), five bools @0x40 to @0x44 |
   | `ThrowProjectile` | vec3@0x30, three u64, u32, five f32, bool, u64 |
   | `SheathePrimaryWeapon` | bool@0x28 |

### Inferences and alternatives

- **The wire payload is these slots in slot order.** The message objects are snapshots
  (`NetHumanIntention_NetProtocol` derives from `NetObjectMemento`), and the snapshot codec writes slots
  in layout order with a changed flag per slot (ACU-RE-0075). The alternative is a per-message serializer
  with its own order; none has been found. Bit widths are still not known (see ACU-RE-0076).
- **u64 fields are mostly entity references.** Entity handles are built by the two helpers, and the
  "derived" u64s are entity handles read through a pointer. The receiving peer resolves them by entity ID
  (ACU-RE-0076 observation 4).
- **vec3 fields in movement messages are world positions or directions.** The ±4000 quantized vec3 in
  `LookAtRequest` uses the same range as proxy x (ACU-RE-0076), so it is likely a world point. Only some
  vec3 fields are quantized.
- **The header u8 (`+0x14`) may be a per-intention channel or priority.** It is not established.

### Unknowns / incomplete coverage

- Field meanings. The offsets name source fields of game intention classes that have not been read.
- Exact bits on the wire: the slot bit widths, and the writer's per-type encoding of u32 and u64.
- How a replica turns a received message back into an intention (the receive and re-enact path).
- Whether the 34 derived values are always entity handles.

### Independent comparison / falsification test

- **AssassinateTarget:** the tool's output matches the manual disassembly field by field, and the
  resync function's offsets agree with it.
- **CoverChangeHeading:** confirmed by hand to copy only the header.
- **Tests:** the synthetic tests check both copy styles, `setne` booleans, stack temporaries written
  after their `lea`, derived sources, and both anchor encodings.
- **Coverage check:** the tool raises an error if any message has zero or more than one block. A wrong
  wire-ID rule would make that fail for most of the 94.

### Reproduction steps

1. Run the atlas (ACU-RE-0072), then the command above.
2. Disassemble the blocks at the reported `block` addresses with capstone. Read only.

### Implementation implications and finding-linked constants

- `local/coop/intention_payloads.json` is a machine-readable schema for every intention message our
  transport must carry: a 2-field header, then typed slots. It is regenerated from the image; the repo
  holds only the tool.
- The constants `0x14018DE90`, the setter VAs and the helper VAs are specific to this capture.
