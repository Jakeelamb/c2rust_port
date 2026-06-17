# Rewrite Manifesto Gate

Use this as a compact operational reading of `rewrites.bio` for faithful bioinformatics rewrites. Source: https://rewrites.bio/manifesto.md

## Required Commitments

- Credit original authors visibly in README/docs/papers/reports, with version, DOI/citation, and source lineage when available.
- Emulate exactly: deterministic tools need byte-identical outputs; floating-point tools need an explicit scientist-defined tolerance; headers, columns, filenames, summaries, parse behavior, and exit behavior all count.
- Be transparent about AI: document AI tools used, their role, validation commands, datasets, and known gaps.
- Work small: choose one testable function, recursion group, CLI surface, or format edge at a time; validate before expanding.
- Use real data before claims: include organism/platform/library-prep diversity, edge cases, exact commands, hardware, and output comparison.
- Build only what is needed: audit target-pipeline usage, implement that surface faithfully, and fail loudly with a clear original-tool fallback for unsupported features.
- Pin versions and document: every equivalence claim names original version, Rust version/commit, data, commands, methodology, date, and update policy.
- Preserve compatibility: one-line pipeline substitution should work when a feature is in scope.
- Maintain and govern: revalidate on upstream releases, expand benchmarks, triage edge cases, and keep release notes honest.
- Contribute upstream responsibly: manually verify with original tool and clean real data before filing upstream bugs; do not automate AI-generated issue reports.

## Fast Decision Gate

Before translating a unit, confirm:

- Original version/source/citation known.
- In-scope behavior and unsupported behavior known.
- Exact output or numeric tolerance known.
- Smallest real or golden fixture known.
- Compact tool path known under `.port-work/`.

If any item is unknown, make it the next blocker or write the uncertainty into `PORT_CONTEXT.md`; do not pretend the rewrite is validated.
