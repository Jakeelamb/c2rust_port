# Review Schema

Findings use TSV:

```tsv
severity	file	source_line	rust_location	evidence	finding	failure_mode	minimal_evidence	fix_direction
```

Severity:

- `BLOCKER`: must fix before continuing.
- `RISK`: plausible but unproven; needs fixture, source row, trace, debugger check, or benchmark.
- `NIT`: optional cleanup.
- `APPROVED_WITH_GAPS`: no known blocker; list remaining unproven areas.

Common blockers:

- Owned pointer translated as borrowed data.
- Borrowed pointer translated as owned data.
- Nullable pointer lost.
- `const T * + len` not translated as slice or preserved raw pair.
- `&mut` introduced where C allowed aliasing.
- `free`, `delete`, or cleanup path omitted.
- `goto cleanup` changed semantically.
- Allocator family crossed.
- Struct layout, enum value, integer width, signedness, or padding drift.
- Union, bitfield, flexible array, tagged pointer, intrusive list, or `container_of` behavior lost.
- Function pointer, callback, virtual dispatch, or vtable behavior changed.
- Macro behavior omitted.
- Error code changed to panic or lossy `Result`.
- Bytes changed to UTF-8 string.
- Floating-point operation order changed without evidence.
- Extra allocation, copy, hash, ordering, or synchronization on a hot path.
- Unsafe lacks a `SAFETY:` comment tied to source facts.
- No project-specific verification proves behavior.

Judge rules:

- Return only confirmed `BLOCKER` findings for fixes.
- Convert speculative claims to `RISK`.
- Request the smallest deciding evidence.
- Do not approve a unit if a blocker remains or behavior lacks verification.
