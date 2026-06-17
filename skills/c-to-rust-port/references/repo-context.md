# Repo Context

Keep repo-local context small. Create files only when they reduce repeated rediscovery.

## Preferred files

- `PORT_CONTEXT.md`: glossary and port policy only. No implementation plan.
- `docs/adr/`: decisions that are hard to reverse, surprising without context, and trade-off driven.
- `.port-work/`: ignored/generated tool artifacts and compact summaries.

## PORT_CONTEXT.md shape

```markdown
# Port Context

## Original Lineage

- Original project/version/commit/license/citation.
- Original command/API surface validated.
- Credit locations to update before release.

## Consumer Contract

- CLI/API/header/file-format surfaces in scope.
- Unsupported features and exact fail-loud behavior.
- Intentional breaking changes, if any, with revalidation requirement.

## Validation Policy

- Exact output requirement or numeric tolerance.
- Real/golden fixtures and edge cases.
- Original/Rust commands, hardware, dates, artifact paths.
- Upstream update/revalidation policy.

## Vocabulary

- Source term: canonical Rust/port term.

## Fidelity Rules

- Numeric, threading, allocation, ordering, and output-format constraints.

## Tool Artifacts

- CCC summary path.
- Tracehash summary/deep run path.
- GDB verifier summary/config path.
- Output/benchmark gate path.

## Known Gaps

- AI-generated or unverified areas.
- Thin validation coverage.
- Upstream bug hypotheses requiring manual proof.
```

Read `PORT_CONTEXT.md` for vocabulary before drafting or reviewing. Update it only when a term or rule will be reused across units.
