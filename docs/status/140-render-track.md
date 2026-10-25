# 139 - Render track: sky, ambiance, climate, GI, water (recon)

Issue: #139 (`type:re`, `area:render`). Branch: `acu-139-render-track`.

## What is measured

| resource | type | where | size | note |
| --- | --- | --- | --- | --- |
| `PAR_WorldAmbiance_Dawn` | `21c2d472` | Paris entry 22301 | 4,037 B | property tree; no readable field names |
| `PAR_LayeredSky_Dawn` | `24d32cf8` | Paris entry 22301 | 2,360,060 B | float/LUT payload; **no** embedded texture, material or texture-set header |
| `Climate` (x4) | `f549cd6b` | Paris entries 22301-22304 | 231 B | one per time of day |
| `PAR_LayeredSky_{Dawn,Night,Noon,Sunset}` | `24d32cf8` | entry 22301-22304 | ~2.36 MB each | four sky sets |
| `GILoader` (x4) | `9ed472d4` | Paris entry 21267 | 6.1-6.6 MB each | the baked GI sets |
| `LocalCubeMap_Cell_N_<hash>` | `eebb2443` | e.g. `DataPC_ACU_LGS_BelleEpoque_assets.forge` | 380 B each | **decodable sky/reflection path**, 23,019 of them |
| `FRV_Water_Seine_LOD0_Chunk_*` | `415d9568` | Paris entry 21267 | - | material `FRV_Water_Seine_Rivers` |

### The usable sky path

A `LocalCubeMap` record is a small struct that links to:

1. a real texture, `a2b7e917`, e.g. `00000027c6a5f511` `LocalCubeMap_Cell_466_1C116722EED5188A`,
   131,412 B - the existing texture parser can decode it (cubemap/layered case to be handled);
2. a time-of-day set, `41813744` named `Sunset`/`Night`/... in `DataPC.forge` entry 110.

So the sky can be reproduced from the game's own cubemaps rather than a guessed gradient. The
`WorldAmbiance` and `LayeredSky` payloads stay undecoded: they are hashed property trees with
no field-name table in the data and no embedded texture headers (`a2b7e917`/`85c817c3`/
`d70e6670` byte patterns do not occur in the sky payload).

## Next steps

1. Decode a `LocalCubeMap` texture (dxgi format, faces, mips) and upload it as a Bevy cubemap
   skybox; pick the record nearest the player/cell.
2. Read the `WorldAmbiance` tree with the same reflection metadata the material parser uses, if
   that metadata exists for this class; otherwise derive sun direction and fog only from measured
   fields once they are identified.
3. Water: apply the `FRV_Water_Seine_Rivers` material (already resolved) and check whether its
   diffuse is present; the Seine currently renders with the ground tone.
4. GI: the four `GILoader` blobs are the baked lighting volumes; format unknown.

No guessed colours or light values are wired into the renderer.
