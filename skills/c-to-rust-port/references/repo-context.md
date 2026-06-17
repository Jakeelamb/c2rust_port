# Repo Context

Keep repo-local context small. Create files only when they reduce repeated rediscovery.

## Preferred files

- `PORT_CONTEXT.md`: glossary and port policy only. No implementation plan.
- `docs/adr/`: decisions that are hard to reverse, surprising without context, and trade-off driven.
- `.port-work/`: ignored/generated tool artifacts and compact summaries.

## PORT_CONTEXT.md shape

```markdown
# Port Context

## Consumer Contract

- CLI/API/header/file-format surfaces in scope.
- Intentional breaking changes, if any.

## Vocabulary

- Source term: canonical Rust/port term.

## Fidelity Rules

- Numeric, threading, allocation, ordering, and output-format constraints.

## Tool Artifacts

- CCC summary path.
- Tracehash run path.
- GDB verifier config path.
```

Read `PORT_CONTEXT.md` for vocabulary before drafting or reviewing. Update it only when a term or rule will be reused across units.
