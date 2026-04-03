---
name: auto-sanity
description: "Senior-engineer sanity discipline for readability, maintainability, and abstraction quality. Identifies monolithic files, repeated inline logic, weak helper boundaries, and code that technically works but is calcifying. Use during implementation, cleanup, refactoring, or review when code should feel senior-maintainable rather than merely functional."
---

# Sanity Checks

## Overview

This skill asks a simple question:

> Does this code still feel like something a strong senior engineer would leave behind?

It is not a final review workflow. It is an ongoing discipline that catches the drift toward:
- giant handlers and components
- repeated inline logic
- helper extraction that never happens
- awkward abstractions
- code that passes checks but is getting harder to live with

## When to Use

- During implementation and cleanup
- After review comments point at readability or structure
- When files are growing quickly
- When code technically works but no longer feels crisp
- When the user asks for "sanity checks", "senior-level cleanup", or "make this feel more maintainable"

## Positive Patterns

Reward these:
- Thin routes/pages/components with richer helpers and services
- Reusable factories for validation, payload shaping, workflow decisions
- Pure helper extraction for business rules
- Clear naming tied to domain language
- Local orchestration with extracted domain calculations
- Focused tests for extracted pure logic

## Negative Patterns

Flag these:
- Files becoming monoliths
- Repeated `if` ladders that should be rule-driven
- Business logic trapped in handlers, pages, or effects
- Overly clever abstractions that hurt readability
- Under-abstracted repetition that should already be shared
- Code that needs comments because the structure is unclear

## What to Recommend

### Extract when:
- a pattern appears 3+ times
- a handler/page is mixing rendering, validation, pricing, and submission logic
- helper-worthy logic has stable inputs/outputs
- the same review comment would be repeated more than once

### Leave local when:
- moving it would only relocate trivial wiring
- the code is already clear and cohesive
- extraction would create indirection without reuse

### Add tests when:
- extraction creates a pure seam
- business rules move out of orchestration code
- a helper now carries meaningful behavior

## Output Format

### What Feels Solid
- What already matches senior-level maintainability

### What Feels Fragile
- Structural drift, monolith risk, repeated logic, naming/readability concerns

### Best Next Refactors
- The smallest high-leverage extractions or cleanups to do now

## Rules

- Favor readability over cleverness
- Favor useful extraction over review-only commentary
- Treat maintainability drift as a real quality issue
- Distinguish between "working" and "senior-maintainable"

