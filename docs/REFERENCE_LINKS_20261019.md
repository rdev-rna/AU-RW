# User-supplied Anvil references reviewed 2026-10-19

Research index and capture-design leads, not recovered ACU behavior. Reviewed
with the web provider's HTML/text extraction. No video was watched, transcript
obtained, third-party code imported, dependency installed or game file changed.
Read alongside `REFERENCE_LIBRARY.md` and finding
`findings/ACU-RE-0020-idle-leg-helper-audit.md`.

## Supplied links and actual scope

| Link | Access, identity and scope |
| --- | --- |
| [GDC Vault: Micropolygon Rendering in 'Anvil'](https://gdcvault.com/play/1035671/Micropolygon-Rendering-in-Anvil) | Public session metadata accessible; speakers Lionel Berenguier and Jack Minnetian, Ubisoft, GDC 2026. Abstract explicitly says the system first shipped in Assassin's Creed Shadows and mentions rasterization, deferred materials and GPU streaming. Recording/transcript contents were not retrieved. |
| [GDC session agenda](https://schedule.gdconf.com/session/micropolygon-rendering-in-anvil/917310) | Supplied URL returned HTTP 404. Its contents and association with the Vault session remain unverified. |
| [Ubisoft: Inside Anvil: The Technology Powering Assassin's Creed Shadows](https://www.ubisoft.com/en-ca/game/assassins-creed/news/3aw71nNlR7kZJzoCATuNtm/inside-anvil-the-technology-powering-assassins-creed-shadows) | Accessible official article, dated April 14, 2026, institutional Ubisoft authorship; no individual byline visible. Covers Atmos weather, micropolygon geometry and raytraced global illumination in Shadows. It is a developer overview rather than a Unity implementation specification. |
| [BuildTable & EntityBuilder System](https://github.com/Kamzik123/AnvilToolkit-Resources/wiki/BuildTable-%26-EntityBuilder-System) | Accessible author-maintained toolkit wiki, Kamzik123, edited July 16, 2026. Examples explicitly come from AC4 exports. Broad cross-game compatibility assertions are the author's claims, unverified against this ACU installation. |
| [AnvilToolkit wiki home](https://github.com/Kamzik123/AnvilToolkit-Resources/wiki) | Accessible; Kamzik123, edited October 12, 2026. Index of export/modding documentation, including skeletons, animation swapping and BuildTables; some tutorials marked WIP. Not a runtime SDK. |
| [Korean Mirage article, supplied URL](http://techartnomad.tistory.com/709?category=1210236) | Initial query URL timed out. [Same article without category query](https://techartnomad.tistory.com/709) accessible: jplee, February 26, 2026, translation title GPU-Driven Rendering in Assassin's Creed Mirage. Introduction says it combines three parts and adds explanation; credits 肖鲁伟. Secondary translation, not primary engine evidence. |
| [CSDN Mirage article](https://blog.csdn.net/qq_41940565/article/details/150465588) | Accessible excerpt titled GPU-Driven Rendering inAssassin's Creed Mirage, account __浅墨__; page shows a latest recommendation/publication timestamp March 28, 2026, whose meaning is ambiguous. Excerpt ends during section 1.2.3 with a subscription prompt. Secondary material; no paywall bypass attempted. Original chapter authorship was not established from this page. |
| [AnvilFix](https://github.com/sherainman/AnvilFix) | Accessible repository by sherainman. Repository description prioritizes Brotherhood. [README](https://github.com/sherainman/AnvilFix/blob/master/README.md) marks AC2/Brotherhood confirmed, lists AC1/Revelations, and describes D3D9 proxy installation. No Unity support evidenced. |

Modern rendering talks provide architectural context. None of these reviewed
rendering links establishes Unity's traversal queries, player animation selection,
bone solver, runtime offsets or raycasting API. Graphics ray tracing and gameplay
collision raycasts are separate questions.

## Concrete research leads

The [BuildTable guide](https://github.com/Kamzik123/AnvilToolkit-Resources/wiki/BuildTable-%26-EntityBuilder-System)
describes numeric asset identities, column/row joins, selectors, tags, weights,
property paths and build passes. Its Standard-versus-Schema exporter section
also warns that equivalent concepts have different XML field names. Treat these
as discovery vocabulary. Proposed ACU capture: connect the player's builder/table
IDs and selected rows to loaded skeleton/mesh/material IDs, with thread, time and
call provenance. This would help distinguish an incorrect outfit/rig selection
from a pose-evaluation defect. Selection precedence, layouts and numeric enum
values still require ACU evidence.

[AnvilFix's API header](https://github.com/sherainman/AnvilFix/blob/master/AnvilAPI.h)
declares pattern scanning, player-entity access, vtable/mid-function hooks and
frame callbacks. Its game enum only lists AC1, AC2, Brotherhood and Revelations;
its context uses D3D9. These are interface-design examples, not compatible ACU
addresses or ABIs. For the local probe, preserve executable hash, module base,
signature occurrence count, validated bytes and lifecycle ownership for each
hook. No code/license review or reuse decision has been made.

## Additional author-maintained skeleton leads

The [Skeleton Files guide](https://github.com/Kamzik123/AnvilToolkit-Resources/wiki/Skeleton-Files)
uses AC4 examples with Rogue schema documentation. Its MaxLookAt section lists
aim axes X/Y/Z/minus-X/minus-Y/minus-Z as 0..5 and up-node control modes
WORLD_ALIGNMENT=0, AXIS_ALIGNMENT=1, LOOK_AT_ALIGNMENT=2. It documents UpNode,
UpUsedAxis, SourceAxis and target offsets. Its orientation-constraint section
describes an offset applied after weighted rotation blending. These are useful
field-identification leads for ACU's unevaluated leg helpers; they do not recover
Unity's exact equations or quaternion averaging.

Proposed comparison: capture original clip pose, modifier parameters and
pre/post-modifier local/global transforms for the actual weighted thigh helper
hashes, their aim targets and up nodes. Correlate the final palette with mesh
vertices. This directly tests the adapter described in ACU-RE-0020.

The linked [Reflex3 constraints guide](https://github.com/Kamzik123/AnvilToolkit-Resources/wiki/Skeleton-Files-%E2%80%90-Reflex3-Constraints)
explicitly scopes its system to Origins onward and says the toolkit handles
Mirage v2 with partial older support. It is separate from BoneModifier data.
Do not substitute its layouts for Unity's early Reflex/BoneModifier data.

## Limits and next evidence

These sources add search terms and capture relationships, not verified hook
sites. Record installed ACU binary/asset provenance in a separate finding before
promoting any lead into recovered behavior. A broad generic register/memory
probe does not by itself decode bones, clip graphs, traversal states or collision
results; each requires identified structures and independently checked samples.
