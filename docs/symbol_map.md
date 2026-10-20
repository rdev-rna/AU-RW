# Symbol map

Map observed original identities to clean implementation names. Native behavior
symbols are added only after executable/export correspondence is established.

| Original identity | Project name | Meaning / evidence |
| --- | --- | --- |
| Skeleton `24aecb7c` / Bone `95741049` | `skeleton::Skeleton` / `skeleton::Bone` | Serialized class IDs and complete simple variant, ACU-RE-0010 |
| Mesh `415d9568` | `mesh::Mesh` | Mesh and compiled/clustered geometry, ACU-RE-0008/0011/0012 |
| Material `85c817c3` / TextureSet `d70e6670` | `material::Material` / `material::TextureSet` | Selected material/reference chain, ACU-RE-0013 |
| `sub_140141F90` | No implementation alias assigned | Parkour string constructor candidate; export/binary binding and behavior unknown |

Names supplied by a third-party format reference are layout hypotheses until
checked against original asset bytes. Raw class hashes, IDs and fields remain
preserved; do not rename or rewrite the reference corpus.
