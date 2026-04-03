---
name: codex-validation
description: "Findings-first validation workflow modeled on strong Codex manual review. Verifies the project, reviews changed code plus surrounding context for correctness and maintainability, fixes approved issues, re-verifies, and only then commits. Use for final validation streams or when the user wants a more rigorous manual audit than the classic /review flow."
---

# Codex Validation

<command-name>codex-validation</command-name>

## Overview

This is a stricter, more manual validation pass than `/review`.

It is designed for the end of a multi-stream implementation, where the goal is not just to comment on the diff, but to validate that the streams fit together cleanly, the project verifies, the abstractions are holding up, and obvious cleanup/testability gaps are closed before commit.

```
1. Verify → 2. Read surrounding context → 3. Produce findings-first review
   → 4. Fix approved findings → 5. Re-verify → 6. Commit
```

## When to Use

- Final validation stream for a `/summon` / `/stream` / `/dominion` plan
- The user explicitly asks for "Codex-style validation", "manual review", or a stronger audit than `/review`
- Multi-stream work where integration risk is higher than usual

## When NOT to Use

- Tiny one-file edits where `/review` is enough
- Early implementation, before the project can be verified meaningfully

## Step 1: Full Verification First

Before reviewing the code, run the full project verification suite.

Minimum:
- Type check / lint
- Tests
- Build
- Smoke checks for changed endpoints/pages when applicable

If verification fails:
- Enter helper mode immediately
- Fix the project until the verification suite is clean enough to review meaningfully

The review is not a substitute for running the project checks.

## Step 2: Read the Actual Change Surface

Inspect:
- Git diff
- Changed file list
- The most important surrounding files for each changed area

Do not review in isolation. Always check adjacent call sites, shared types, tests, and any files that should have changed if the diff is correct.

## Step 3: Review With Findings First

Prioritize findings over summary. The review should answer:

1. Could this break in production?
2. What was changed without its dependent co-change?
3. What logic is duplicated and should be extracted now?
4. What pure logic lacks direct tests?
5. What large files or handlers should be decomposed before they calcify?

### Review categories

- Correctness and business logic
- Cross-file impact and missing co-changes
- Edge cases and error handling
- Test gaps and testability
- Duplication and extraction opportunities
- Architectural drift: routes/pages doing domain work that should live in helpers/services

## Step 4: Report Format

Use this order:

1. **Findings**
2. **Open questions / assumptions**
3. **Short change summary**

Findings should be concrete and actionable:

```markdown
## Findings

1. [Correctness] `src/routes/api/foo/+server.ts:42`
   Refund metadata is written before the Stripe call succeeds, so failed refunds can still mark the record as refunded.

2. [Cross-file impact] `src/lib/server/services/bar.ts:15`
   The new return type adds `statusLabel`, but the consuming page still formats labels locally, so the two can drift.

3. [Test gap] `src/lib/features/baz/pricing.ts:1`
   Discount ordering is now encoded in helper logic but there is no direct unit test covering fixed-then-percent application order.
```

If there are no findings, say that explicitly and mention any residual risk.

## Step 5: Fix the Approved Findings

For approved issues:
- Fix them directly
- Prefer extraction to reusable factories/helpers when the same pattern appears 3+ times
- Add focused tests for pure logic you extracted or materially changed
- Re-run verification after each meaningful batch

## Step 6: Re-Verify

After fixes:
- Re-run type check / lint
- Re-run relevant tests
- Re-run build if touched areas justify it

Do not close the validation pass on assumptions.

## Step 7: Commit

When the user approves:
- Stage specific files only
- Commit with a conventional message
- Push if requested by the active workflow

## Principles

- Findings first, summary second
- Verify before opinion
- Review surrounding context, not only the diff
- Extract repeated patterns instead of leaving them as review notes
- Add tests where refactors create clean pure seams
- Prefer "thin route/page, rich helper/service" architecture

