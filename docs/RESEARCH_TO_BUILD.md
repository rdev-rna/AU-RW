# Research -> build flow

Research finds out what the original does. Build implements it. Keeping them separate
stops agents from coding on guesses and from duplicating each other's investigations.

```
type:re (stage:research)  ->  finding + handoff  ->  type:slice (stage:build)  ->  parity gate
   question, evidence          ACU-RE-NNNN            cites the finding            validated vs native
```

## Research issue (`type:re`, label `stage:research`)
- One bounded question, evidence pointers, what would falsify the expected answer.
- Done when `docs/findings/ACU-RE-NNNN-slug.md` merges with observation / inference /
  unknown separated, and the issue has a **Development handoff** comment (template below).
- A negative result is a valid result: record it, then close.
- Research may include throwaway probes and captures (take the locks), but not product code.

## Development handoff comment (required to close a research issue)
```
HANDOFF finding=ACU-RE-NNNN confidence=observed|partial|unresolved
Established: <facts a builder may rely on, with VA/archive pointers>
Not established: <what a builder must not assume>
Interface: <inputs/outputs/units/axes for the thing to build, or "unknown">
Oracle: <how a build slice can be checked against native: capture, tap, gate>
Spawned: #N, #M   (build slices, each with Depends on / finding link)
Follow-up research: #K  (open questions that remain)
```

## Build issue (`type:slice`, label `stage:build`)
- Must name the finding(s) it implements. No finding, no `status:ready`: label it
  `needs-finding` and `status:blocked` and link the research issue that will produce one.
- Acceptance must include an independent gate against native behaviour (oracle from the
  handoff) or an explicit statement that it is a labelled synthetic/approximate choice.
- If the build reveals the finding is wrong or incomplete, stop, comment on the research
  issue, reopen it (`stage:research`), and mark the build `status:blocked`.

## Labels
`stage:research`, `stage:build`, `needs-finding`. Existing `type:re` and `type:slice`
issues carry the matching stage label. `type:bug`/`type:infra` are not staged.

## Who does what
The same agent may do both stages, but they are separate PRs: findings first, code after,
so another agent can pick up the build from the finding alone.
