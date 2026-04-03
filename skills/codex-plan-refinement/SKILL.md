---
name: codex-plan-refinement
description: "Codex-side plan refinement before execution begins. Refines plans for clarity, stream quality, realistic dependencies, abstraction sanity, reuse of existing components/patterns, verification coverage, and better compression without losing important intent. Use when reviewing a plan that exists without a status file or when the user wants Codex to improve a plan before /stream or /dominion."
---

# Codex Plan Refinement

## Overview

This skill refines a plan *before* execution starts.

It is not the same as structural validation:
- `plan-validate` asks whether a plan is executable
- `codex-plan-refinement` asks whether the plan is sharp, sane, reuse-aware, and likely to produce senior-maintainable code

The goal is to make the plan better while often making it smaller:
- compress redundant or over-explained plan sections
- sharpen stream intent
- improve reuse of existing code patterns/components
- make abstractions and verification expectations explicit

## Refinement Angles

### 1. Plan Clarity

Check whether each stream is easy to understand quickly:
- does the title say what actually changes?
- does the task list communicate intent rather than implementation noise?
- is the plan bloated with wording that can be compressed?
- is any stream carrying too many unrelated ideas?

Prefer concise, high-signal stream descriptions.

### 2. Dependency Sanity

Refine dependencies so they reflect real constraints:
- remove conceptual dependencies
- split interface-first work when it unlocks parallelism
- merge tiny streams that only exist because the plan was written too literally

Ask:
"What artifact does this stream actually need from the other one?"

### 3. Reuse Over Reinvention

Look for places where the plan should explicitly reuse existing code instead of making new patterns:
- shared helpers
- existing components
- current services
- established validation/payload/workflow patterns
- current UI/editor structures

If the plan implies new abstractions where existing ones should be extended, flag it.

### 4. Abstraction and Sanity

Refine the plan toward senior-maintainable outcomes:
- avoid creating new monoliths
- push domain logic out of routes/pages/components when appropriate
- identify where factories/helpers/workflow modules should be used
- identify where extraction should happen during implementation, not as an afterthought

### 5. Verification Story

Every stream should have a concrete verification path:
- what tests change?
- what type/build checks matter?
- what smoke behavior should be verified?

If verification is vague, refine the stream to make it more testable and more obviously checkable.

### 6. Plan Compression

Make the plan smaller when possible without making it weaker:
- compress repeated guidance
- remove duplicate stream wording
- collapse obviously overlapping tasks
- keep the plan high-signal and execution-oriented

The objective is:

> better plan, less plan

## Output Format

### Refinement Findings
- Where the plan is unclear, overgrown, under-specified, or likely to generate weak code shape

### Recommended Plan Edits
- Exact amendments to streams, dependencies, reuse expectations, or verification notes

### Compression Opportunities
- What wording or structure can be simplified without losing intent

### Handoff Readiness
- Is this ready for `/stream` or `/dominion` after the amendments?

## Rules

- Favor clarity, reuse, and execution-readiness
- Improve the plan without inflating it
- Prefer reusing established patterns over inventing new ones
- Treat abstraction drift and monolith risk as planning concerns, not only implementation concerns

