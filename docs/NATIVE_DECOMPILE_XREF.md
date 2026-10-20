# Live image to decompile cross-reference

`tools/decompile_xref.py` joins address-named ACU decompile files to function
ranges found in a mapped main-image capture. It converts the filename VA to an
RVA using the PE **preferred** image base, then reports exact unwind-range starts
and weaker addresses that fall inside an unwind range. This makes candidate
function files quick to open while inspecting native probe captures.

```powershell
python tools/native_image.py local/native-probe/live-image.mapped.bin local/native-targets/live --mapped --live-base 0x140000000
python tools/decompile_xref.py local/native-targets/live/functions.jsonl local/native-targets/live/summary.json "ACU - Decompiled" local/native-targets/live/decompile-xref
python tools/decompile_xref.py local/native-targets/live/functions.jsonl local/native-targets/live/summary.json "ACU - Decompiled" local/native-targets/live/decompile-xref --session local/native-probe/session.jsonl
```

Outputs are generated machine data and belong under ignored `local/`:

- `native-functions.jsonl`: every unwind range, with zero or more decompile
  address/name candidates and an explicit status.
- `decompile-xref.jsonl`: matched corpus files with their first-line declared
  name/size, VA/RVA, range evidence, image hash, preferred/live bases and match
  relation.
- `decompile-xref-summary.json`: counts and limitations.
- With `--session`, `live-events.jsonl` maps each recorded RIP to its module,
  unwind range and any decompile file candidates.

The installed ACU preferred base is `0x140000000`. The explicit live base is
used to resolve relocatable PE structures in the mapped-image inventory; the
preferred base remains the basis for converting corpus VAs. `native_image.py`
records both separately.

## Evidence boundary

This is an address join, not binary identity proof. Decompiled filenames encode
export VAs, but their correspondence to the installed executable remains
unverified. Exact unwind-start matches are stronger address evidence than
containment, yet unwind entries can be fragments and some functions lack entries.
Names are copied verbatim from the decompiler's first line. A matching name does
not prove function semantics, call convention, Arno ownership, or that the
function executed. Preserve the mapped image SHA, disk executable SHA from its
capture manifest, image base, corpus path, and decompiler line when promoting a
candidate into a hook profile or reverse-engineering finding.

The tool scans address-shaped `.c` filenames only, skips symlinked directories,
and bounds corpus traversal with `--max-files` (default 300,000). It reads only
the first line of files that join to a mapped function range. Results contain no
copied pseudocode bodies.
