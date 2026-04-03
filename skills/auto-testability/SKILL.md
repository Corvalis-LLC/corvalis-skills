---
name: auto-testability
description: "Testability discipline for application code. Identifies when logic should be extracted into pure helpers, when routes/pages/components are carrying too much domain work, and when code needs seams for focused unit tests. Use when reviewing maintainability, refactoring for better tests, or validating whether code is easy to verify without brittle integration-heavy coverage."
---

# Testability

## Overview

This skill is about whether code can be tested cleanly, not whether tests exist.

A codebase can have many tests and still be hard to verify because the logic is trapped inside:
- route handlers
- page components
- giant service functions
- deeply stateful orchestrators
- inline conditional ladders repeated across call sites

The goal is to create code with clean seams:
- pure helpers for domain logic
- thin orchestration layers
- explicit dependencies
- narrow units that can be tested directly

## When to Use

- Reviewing code structure after implementation
- Refactoring large pages, handlers, or services
- Evaluating whether a feature is easy to verify
- The user says "make this more testable", "extract this logic", or "why is this hard to test?"

## Positive Patterns

Reward these:
- Pure helper extraction for pricing, validation, payload shaping, mapping, selection logic
- Rule/factory-driven validation instead of repeated `if` chains
- Thin routes/pages that orchestrate, while helpers/services own domain logic
- Focused unit tests around extracted pure logic
- Dependency boundaries around DB, network, file I/O, time, randomness
- Small workflow modules instead of giant transactional scripts

## Negative Patterns

Flag these:
- Large handlers/components doing business logic inline
- `$effect`/UI blocks containing domain calculations that should be extracted
- Repeated validation logic across files
- Heavy conditional trees that should be data-driven or rule-driven
- Tests that must boot too much application surface just to validate one business rule
- Internal logic that can only be exercised through UI or API integration paths

## What to Recommend

### Extract pure helpers when:
- logic appears in 3+ places
- the logic has clear inputs and outputs
- the logic encodes business rules
- the tests currently need too much setup to reach it

### Keep orchestration local when:
- it is mostly wiring state and calling helpers
- extraction would only move one trivial line
- the logic is tightly coupled to one UI/control flow boundary

### Create seams for:
- time
- randomness
- external APIs
- persistence
- serialization/deserialization boundaries

## Output Format

### Testability Strengths
- What already has good seams

### Testability Risks
- Where logic is trapped in high-friction locations

### Recommended Extractions
- Exact helpers/modules to create or strengthen

### Suggested Tests
- Which focused tests become possible after the extraction

## Rules

- Do not equate “has tests” with “is testable”
- Prefer extraction of domain logic over adding more brittle integration tests
- Favor pure functions and narrow workflow modules
- Treat testability as an architectural quality, not a test-file concern

