---
name: verify
description: "Plan-aware Codex verification and refinement. If an active plan has a status file, perform findings-first implementation validation. If a plan exists without a status file, treat /verify as a Codex plan-refinement pass before execution. User-invocable via /verify command."
---

# /verify — Plan-Aware Codex Validation

<command-name>verify</command-name>

## Overview

`/verify` is the Codex-side validation pass the user can invoke directly.

It has two modes:

- **Plan refinement mode**: when a plan file exists but no status file exists yet
- **Implementation validation mode**: when an active plan has a status file, or when no plan exists and review falls back to the current working tree

Use this when the user wants the stronger manual validation style rather than a lightweight diff review.

## Behavior

Default behavior: **begin review immediately**. Do not ask setup questions unless multiple active plans make auto-resolution ambiguous.

## Step 1: Resolve Context

### Prefer the active plan when one exists

Resolve in this order:

1. `docs/plans/*.status.json` with any non-completed streams
2. Most recent `docs/plans/*.md`
3. If no plan exists, review the current git working tree

If exactly one active plan exists:
- read the plan
- read the status file
- identify completed, in-progress, and final-validation state
- enter **implementation validation mode**

If no status file exists but exactly one recent plan exists:
- read the plan
- assume the user wants **Codex plan refinement before execution**
- do not default to code review yet

If multiple active plans exist:
- show the candidates and ask the user which one to verify

If no active plan exists:
- fall back to git-based review of current changes

## Step 2A: Plan Refinement Mode

If a plan exists without a status file, treat `/verify` as a pre-execution Codex refinement pass.

Load and follow `codex-plan-refinement`.

Review the plan using the refinement angles from `codex-plan-refinement`.

Output format in plan refinement mode:

1. **Refinement Findings**
2. **Recommended Plan Amendments**
3. **Compression Opportunities**
3. **Short Handoff Readiness Summary**

If the plan is already strong, say so explicitly.

## Step 2B: Implementation Validation Mode

Run the project verification suite before producing review findings.

Minimum:
- type check / lint
- relevant tests
- build when applicable

If checks fail:
- note the failures
- continue the review, but treat verification failures as top-priority findings

## Step 3: Read the Real Change Surface

Skip this step in plan refinement mode unless the plan references existing code that must be inspected for feasibility.

In implementation validation mode, inspect:
- `git diff HEAD`
- changed file list
- the plan's owned files for the current or final stream when available
- adjacent consumers, shared types, tests, and related services

Do not review the diff in isolation.

Always look for:
- missing co-changes
- duplicated logic that should already be extracted
- pure logic that should have direct tests
- route/page files doing domain work that belongs in helpers or services

## Step 4: Review Using Senior Standards

Prioritize these categories:

1. Correctness and business logic
2. Cross-file impact
3. Edge cases and error handling
4. Test gaps and testability seams
5. Duplication and extraction opportunities
6. Maintainability and architectural drift

Specific patterns to reward:
- thin routes/pages, richer helpers/services
- reusable factories for repeated validation/payload/workflow logic
- pure pricing/validation/domain helpers with focused tests
- findings-first review instead of “looks good” summaries
- explicit verification after changes

Specific patterns to flag:
- repeated inline `if` ladders that should be rule-driven
- large handlers/pages accumulating domain logic
- cross-file contract changes without dependent updates
- extracted helpers without direct tests
- “works but is calcifying” structure in growing files

## Step 5: Report Format

Report in this order:

1. **Findings** — ordered by severity with file references
2. **Open questions / assumptions**
3. **Short summary**

If no findings exist, say so explicitly and mention residual risk if any.

## Step 6: Fix on Request

If the user asks to fix findings:
- fix the approved issues directly
- prefer extracting reusable factories/helpers/workflows instead of patching around duplication
- add focused tests for pure logic that was extracted or materially changed
- re-run verification

## Step 7: Close Cleanly

Before closing:
- re-run checks after fixes
- confirm whether the tree is verification-clean
- summarize what was fixed and what remains, if anything

## Rules

- Start reviewing immediately when invoked
- Prefer active-plan awareness over blind diff review
- If a plan exists without a status file, default to plan refinement mode
- Verify before opinion whenever possible
- Findings first, summary second
- Review surrounding context, not only changed lines
- Treat testability as part of code quality, not an optional add-on
- Prefer extraction over repeated review comments when the same pattern appears in 3+ places
