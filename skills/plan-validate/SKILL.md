---
name: plan-validate
description: "Validate a multi-stream implementation plan before /stream or /dominion execution. Checks that stream structure, dependencies, file ownership, required skills, verification strategy, and final validation mode are present and coherent. Use when reviewing a plan file, before execution handoff, or when plans may have been hand-edited."
---

# Plan Validation

## Overview

This skill validates the plan itself before implementation starts.

It exists because the summon/stream/dominion workflow increasingly depends on plan metadata being correct. A good-looking plan that is structurally incomplete can cause subtle downstream failures:
- streams that cannot be claimed correctly
- false dependencies that kill parallelism
- overlapping file ownership
- final validation mode missing
- no clear verification story

## When to Use

- Before `/stream` or `/dominion` on any multi-stream plan
- When a plan was edited manually after generation
- When a plan was written outside the normal `/summon` workflow
- When the user says "validate the plan", "check this plan", or "is this plan executable?"

## What to Validate

### 1. Stream Structure

Check that:
- stream headers parse cleanly
- stream IDs are unique
- sub-streams are named consistently
- each stream has a clear title and task scope

### 2. Dependencies

Check that:
- every dependency refers to an existing stream
- no dependency cycles exist
- dependencies are real, not just conceptual ordering
- the critical path is not obviously over-constrained

When a dependency looks fake, ask:
"What file, type, endpoint, schema, or migration does this stream actually need from the other one?"

If you cannot name the artifact, flag it.

### 3. File Ownership

Check that:
- each stream has owned files listed
- ownership is concrete enough to be actionable
- shared files are either additive-only or explicitly sequenced
- no two parallel streams mutate the same file without coordination

### 4. Required Skills

Check that:
- `## Required Skills` exists for multi-stream plans
- baseline skills are present
- per-stream skills look plausible for the work described
- skills are not obviously missing for security, data, API, UI, or test work

### 5. Verification Story

Each stream should have an obvious verification surface:
- tests to add or update
- type/build/lint expectations
- smoke-test expectations for routes/pages/endpoints

If a stream has no clear verification story, flag it.

### 6. Final Validation Mode

Check that the plan records:

```markdown
## Final Validation Mode
Mode: codex
```

or

```markdown
## Final Validation Mode
Mode: review
```

If missing, recommend adding it before execution.

## Output Format

Report in three buckets:

### Executable
- Things that are structurally correct

### Gaps
- Missing sections, invalid dependencies, ambiguous ownership, missing verification

### Recommended Amendments
- Concrete edits to the plan before execution begins

If the plan is execution-ready, say so explicitly.

## Rules

- Be strict about structure, not verbose about theory
- Prefer concrete amendments over abstract criticism
- Flag false dependencies aggressively
- Treat missing verification and missing final validation mode as real issues

