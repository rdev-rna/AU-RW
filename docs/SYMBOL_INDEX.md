# Symbol index (corpus navigation)

A separate, provenance-tracked SQLite index that puts class names on the anonymous
`sub_<VA>` functions in `ACU - Decompiled/`. It is derived from the game's own runtime
class reflection, read from a captured mapped image. The corpus is listed, never read in
bulk or modified. Mechanism, counts and limits: [ACU-RE-0060](findings/ACU-RE-0060-reflection-symbol-index.md).

## Build (offline, ~10 s)

```powershell
python tools/symbol_index.py `
  --image "E:/ReverseEngineering/Projects/Unity/local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin" `
  --expect-sha256 9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520 `
  --corpus "E:/ReverseEngineering/Projects/Unity/ACU - Decompiled"
```

This writes `local/symbol_index.sqlite`, which git ignores. `--corpus` defaults to
`ACU_DECOMPILED_DIR` or `local/project.json`. Without a corpus, `in_corpus` is NULL
(unavailable), not zero. The build is deterministic: it records no timestamps, and
`meta` holds the image, tool and findings SHA-256s plus the coverage JSON. Curated
finding names come from `data/symbol_names_findings.json` (`--findings`).

## Ask

```powershell
python tools/symbol_query.py info                      # inputs + coverage
python tools/symbol_query.py name 0x141CE1FF0          # -> Entity::vf16 (slot +0x80), corpus header
python tools/symbol_query.py class BipedComponent      # parent chain, size, vtable(s)
python tools/symbol_query.py methods BipedComponent --own
python tools/symbol_query.py fields BipedComponentBase --inherited
python tools/symbol_query.py find Velocity
python tools/symbol_query.py callers 0x14262AF90       # candidate call edges, with names
python tools/symbol_query.py sql "SELECT name, printf('0x%X', object_size) FROM class WHERE name LIKE 'Biped%'"
```

Add `--json` for machine output. VAs are INTEGER columns; use `printf('0x%X', va)`.

## Reading the names

| source | confidence | meaning |
| --- | --- | --- |
| `capture-finding` | `finding-live` / `finding-static` / `finding-inferred` | Role stated by a cited `ACU-RE-00xx` finding (`detail.finding`, `detail.line`). |
| `reflection-vtable` | `high` | `Class::vf<slot>`. Every class using the function inherits it from that class. |
| `reflection-vtable` | `medium` | The same slot in classes that are not on one inheritance line, typically identical code folded by the linker. Named after the shallowest class; `detail.classes` lists every user. |
| `reflection-vtable` | `low` | `folded_vf_<VA>`: the same code sits at several different slots (stubs such as `ret`). |
| `reflection-property` | `medium` / `low` | `Class::<prop>_accessor<k>`: a getter/setter in a reflected property descriptor. The property name is `medium` when its CRC-32 matched an identifier string in the image, `low` when it is shown as `prop_<hash>`. |
| `propagation` | `low` | `Class::helper_<VA>`: every candidate caller is a named function of that one class. |

`vf<slot>` is a slot number, not a method name. Two classes' `vf16` are the same virtual
only when they share an inheritance line. The original `sub_<VA>` name comes from the
corpus filename convention; `name <va>` also prints that file's real first line.

## Tables

`class` (class info: name, getter, parent, CRC type id, object size, isA interval,
property count), `vtable` (start, slot count, why the scan stopped), `method` (function,
class, slot, `own` / `inherited` / `new-slot` / `unknown`), `field` (reflected property:
name hash, resolved name, type code, type CRC, offset, accessor check), `name`, `function`
(corpus/pdata membership), `call` (E8/E9 rel32 candidate edges), and `meta`.

Treat field offsets as observations from this capture. When the `offset_check` column is
`match`, a one-instruction accessor touches the same offset.
