# Corvalis Skills

A skill system for automated software development across **Claude Code** and **Codex**. Corvalis is designed around a small set of human entry points, a larger set of mostly automatic discipline skills, and a clean handoff from planning to execution to validation.

## From Idea To Implementation

This is the intended default flow.

### 1. Start In Claude Code With `/summon`

Use `/summon` whenever you are starting a new piece of work.

- Choose **Plan** if the work is non-trivial
- Choose **No plan** if the task is small and you just want to build
- Choose **Talk about it** if the idea is still fuzzy

If you choose **Plan**, `/summon` will:
- use `corvalis-recon` as the first-pass codebase analysis step when the binary is installed in `~/.claude/bin/`
- write the plan to `docs/plans/`
- run the standards gate against relevant `auto-*` skills
- optionally run refinement gates like Swarm, Skill Gate, and Triumvirate
- ask which final validation style the auto-injected last stream should use

If you choose **No plan**, `/summon` now follows a stricter bootstrap:
- clarify the request first
- gather repo context with `corvalis-recon` first when available
- do only the additional targeted reads needed from there
- load the relevant `auto-*` skills
- then begin implementation

If you choose **Talk about it**, `/summon` now:
- does real web research before making architecture or pattern recommendations when outside evidence would help
- cites sources directly to the user
- loads `auto-web-validation` before that research so source-authored AI instructions or coercive "must use" claims are treated as untrusted unless corroborated

### 2. When You Are Happy With The Plan, Open Codex And Run `/verify`

This is the preferred handoff before execution starts.

- If the plan exists **without** a status file yet, Codex `/verify` behaves like a plan-refinement pass
- It should tighten clarity, catch missing reuse opportunities, improve abstraction sanity, and compress the plan where possible without losing intent
- This is the best moment to get the “senior sanity check” before streams start creating state

### 3. Go Back To Claude Code And Execute

After the plan looks good:

- Use `/stream` if you want to execute one stream at a time with hands-on control
- Use `/dominion` if you want Claude Code to execute the full multi-stream plan autonomously

`/stream` and `/dominion` create and use the plan’s companion `.status.json` file. That status file is the boundary between “pre-execution plan refinement” and “in-flight implementation validation.”

### 4. Validate While Work Is In Flight

Once `/stream` or `/dominion` has started and the status file exists, Codex `/verify` changes roles:

- it stops acting like a plan-refinement pass
- it becomes a findings-first implementation validation pass
- it checks correctness, cross-file impact, maintainability, testability, and senior-level readability

### 5. Finish Through The Final Validation Stream

Every multi-stream execution path ends with an auto-injected final validation stream.

- If the plan’s final validation mode is `codex`, Claude runs a `Final Cleanup` stream first as the automated discipline sweep, then hands off to Codex `/verify` for the stricter human-in-the-loop findings-first final audit
- If the plan’s final validation mode is `review`, that final stream uses the classic `/review` path

In `review` mode, the final stream verifies the completed work, commits, pushes, and closes out the plan and status files.

In `codex` mode, this behaves like a double verification pass: Claude does the broad cleanup and automated standards sweep first, then Codex does the stricter final review. The Claude cleanup stream leaves the repo uncommitted and preserves the plan/status artifacts so Codex `/verify` can do the final nitpick pass with full context.

## Primary Entry Points

These are the commands you should actually remember.

### Claude Code

- `/summon` — session bootstrap, planning, standards gate, handoff
- `/stream` — execute one stream from a plan
- `/dominion` — execute the full multi-stream plan autonomously

### Codex

- `/verify` — active-plan-aware plan refinement or implementation validation

Everything else should be treated as either:
- an automatic discipline (`auto-*`)
- or a specialized supporting workflow used deliberately, not as a primary entry point

## Installation

### Claude Code Quick Install (symlink)

Clone this repo and run the install script:

```bash
git clone <this-repo-url> auto-skills
cd auto-skills
chmod +x install.sh
./install.sh
```

The script symlinks each skill directory into `~/.claude/skills/`. Existing skills with the same name are backed up to `~/.claude/skills-backup-<timestamp>/`.

It also attempts to install or upgrade `corvalis-recon` into `~/.claude/bin/`. When present, `/summon` will automatically use that binary during the planning path for structured codebase analysis. On `zsh` and `bash`, the installer also adds a `recon` alias pointing to that binary if the alias is not already present.

If you want the shorter shell alias, add this to your shell config:

```bash
echo 'alias recon="$HOME/.claude/bin/corvalis-recon"' >> ~/.zshrc
source ~/.zshrc
```

For bash:

```bash
echo 'alias recon="$HOME/.claude/bin/corvalis-recon"' >> ~/.bashrc
source ~/.bashrc
```

### Claude Code Manual Install

Copy the `skills/` directory contents to your Claude Code skills directory:

```bash
cp -R skills/* ~/.claude/skills/
```

### Codex Companion Install

For the recommended Codex companion setup, use the dedicated installer:

```bash
chmod +x install-codex.sh
./install-codex.sh
```

That installer links the recommended Codex-side skills into `~/.codex/skills/`:
- `verify`
- `auto-sanity`

It also leaves the optional Codex companion skills available to install manually if you want a richer validation stack.

Codex uses a separate skills directory:

```bash
mkdir -p ~/.codex/skills
```

Codex only needs the companion verification-side skills, not the entire Claude orchestration stack. Recommended Codex installs:

- `verify`
- `auto-sanity`

Optional Codex companion skills:

- `codex-validation`
- `auto-testability`

Example manual install:

```bash
mkdir -p ~/.codex/skills/verify ~/.codex/skills/auto-sanity
cp -R skills/verify/* ~/.codex/skills/verify/
cp -R skills/auto-sanity/* ~/.codex/skills/auto-sanity/
```

If you also want the optional Codex companion skills:

```bash
mkdir -p ~/.codex/skills/codex-validation ~/.codex/skills/auto-testability
cp -R skills/codex-validation/* ~/.codex/skills/codex-validation/
cp -R skills/auto-testability/* ~/.codex/skills/auto-testability/
```

Recommended operating model:
- Claude Code owns planning and execution: `/summon`, `/stream`, `/dominion`
- Codex owns the stronger validation lane: `/verify`

## corvalis-recon

`corvalis-recon` is the AST-based structured codebase analysis binary that powers recon-aware planning.

The source for the tool lives in [tools/recon](/Users/architect/Documents/GitHub/corvalis-skills/tools/recon). It is a standalone Rust CLI, not just a summon helper, and it is meant to be useful both inside the Corvalis workflow and directly from the command line when you want fast structural understanding of a codebase.

### What The Tool Is

`corvalis-recon` analyzes supported source trees by parsing them into syntax trees and producing structured output that is easier for planning agents to consume than raw file listing and grep alone.

Its current job is to build a compact map of a codebase by combining:
- source discovery with ignore awareness
- parsing and symbol extraction
- dependency and re-export analysis
- complexity and hotspot metrics
- project overview and ranked file summaries
- budget-aware truncation for large repositories

This makes it useful for:
- pre-plan repo reconnaissance
- agent context compression
- architectural orientation in unfamiliar TS-heavy repos
- inspecting likely entry points, hotspots, barrels, and dependency shape before implementation

Why AST-based analysis matters:
- it understands code structure instead of guessing from text alone
- it can distinguish declarations, exports, imports, and re-exports more reliably than grep-style scanning
- it produces cleaner summaries for planning, dependency analysis, and hotspot detection in larger repos

### Tech Stack

`corvalis-recon` is implemented as a Rust CLI with a small, focused stack:
- `clap` for the command-line interface
- `tree-sitter` with vendored grammars for TypeScript, TSX, JavaScript, and Svelte AST-style parsing
- `serde` / `serde_json` for machine-readable output
- `ignore` for `.gitignore`-aware file discovery
- `rayon` for parallel parsing work
- `json5` for tolerant config parsing where needed

The current implementation is optimized for TypeScript-heavy repos, which matches the main Corvalis use case today.

It is primarily used by `/summon` during the **Plan** path:
- if `~/.claude/bin/corvalis-recon` exists, `/summon` attempts to run it automatically in compact planning mode
- the shell alias `recon` can be pointed at that binary for direct terminal use
- for larger repositories, summon can pass `--budget 8000` to keep the output compact
- if the binary is missing or recon fails, summon silently falls back to normal `Glob` / `Grep` / `Read` exploration

### What It Produces

`corvalis-recon analyze` combines:
- symbol extraction
- dependency graph construction
- complexity metrics and hotspot detection
- project overview metadata
- file ranking for budget-aware truncation

This gives planning flows a cleaner map of a TS / JS / Svelte codebase before stream boundaries and execution decisions are made.

Top-level properties in the default full payload:
- `version`
- `project`
- `files`
- `graph`
- `hotspots`
- `warnings`
- `summary`

Top-level properties in planning mode (`analyze --mode planning`):
- `version`
- `project`
- `symbols`
- `dependencies`
- `graph`
- `hotspots`
- `warnings`
- `summary`
- `planning`

The `planning` object currently includes:
- `primary_entry_points`
- `dependency_hubs`
- `hotspot_files`
- `priority_files`

### Direct Usage

You can also run recon directly outside `/summon`:

```bash
~/.claude/bin/corvalis-recon analyze --root /path/to/project
```

Or, if you added the alias:

```bash
recon analyze --root /path/to/project
```

Useful direct applications:
- inspect the full JSON output for a repo before writing a plan
- generate a compact planning payload with top-level `symbols`, `dependencies`, and curated entry-point / hotspot context
- generate a compact budgeted snapshot for large codebases
- view a human-readable ranked summary with `--format pretty`
- debug dependency structure, entry points, cycles, and hotspots independently of summon

Examples:

```bash
~/.claude/bin/corvalis-recon analyze --root /path/to/project --format json
~/.claude/bin/corvalis-recon analyze --root /path/to/project --format json --mode planning
~/.claude/bin/corvalis-recon analyze --root /path/to/project --format pretty
~/.claude/bin/corvalis-recon analyze --root /path/to/project --budget 8000
```

Alias equivalents:

```bash
recon analyze --root /path/to/project --format json
recon analyze --root /path/to/project --format json --mode planning
recon analyze --root /path/to/project --format pretty
recon analyze --root /path/to/project --budget 8000
```

Diff-scoped examples:

```bash
recon analyze --root /path/to/project --format json --mode planning --diff HEAD
recon analyze --root /path/to/project --format json --mode planning --diff main...HEAD
```

`--diff <range>` scopes analysis to changed supported source files plus a small local context window:
- changed files in the git diff range
- a few same-directory sibling files
- directly imported project files referenced by the changed files

The output includes a `scope` object so downstream tools can see exactly which files were included.

Recommended budget guidance:
- small repos: no budget
- medium repos: `--budget 16000`
- large repos: `--budget 8000`
- very large repos: `--budget 4000`

Current scope:
- TypeScript
- JavaScript
- Svelte

Rust and other languages can be added later, but today recon is optimized for the TS-heavy workflow Corvalis uses most often.

## Quick Start

### Claude Code flow

1. Install the Claude Code skills (see above)
2. Start a new Claude Code session
3. Run `/summon`
4. Choose **Plan** — describe what you're building
5. If `corvalis-recon` is installed, summon will automatically use it to gather structured repo context
6. In **No plan** mode, summon clarifies first, gathers context, loads the relevant `auto-*` skills, and begins
7. In **Talk about it** mode, summon can do cited web research before recommending approaches
8. The system writes a plan, validates it, and recommends next steps
9. When you are happy with the plan, open Codex and run `/verify`
10. Return to Claude Code and run `/dominion` to execute the full plan autonomously, or `/stream` to execute one stream at a time

### Codex flow

1. Install Codex companion skills into `~/.codex/skills/` using `./install-codex.sh` or the manual steps above
2. If `corvalis-recon` is installed in `~/.claude/bin/`, Codex `/verify` can use it as a first-pass repo context source before deeper validation
3. If a plan exists but execution has not started yet, run `/verify` to refine the plan
4. If implementation is already underway, run `/verify` to perform the stronger findings-first validation pass

## The Execution Stack

```
  /summon          Session bootstrap — brainstorm, plan, validate
     │
     ├──► corvalis-recon   Structured repo analysis when installed
     ├──► auto-web-validation   Mandatory before web/package/vendor research
     │
     ├──► /verify        Codex plan refinement before execution
     │
     ▼
  /dominion        Autonomous orchestrator — spawns headless Claude instances
     │
     ├──► /stream [A]    ──► legion wave 1 ──► wave 2 ──► ...
     ├──► /stream [B]    ──► legion wave 1 ──► wave 2 ──► ...
     │         (parallel if no dependency)
     ▼
  /stream [C]      Waits for A & B, then executes
     │
     ├──► Final Cleanup   Claude automated verification and cleanup pass
     │
     ├──► /verify        Codex implementation validation while work is active
     ▼
  Done             All streams complete, plan verified
```

## Skill Reference

### Claude Code Entry Points

| Skill | Description |
|-------|-------------|
| `summon` | Session bootstrap — offers plan, no-plan, or talk-it-out paths |
| `dominion` | Autonomous plan executor — spawns headless Claude instances per stream |
| `stream` | Per-stream executor with dependency tracking and verification gates |

### Codex Entry Points

| Skill | Description |
|-------|-------------|
| `verify` | Plan-aware Codex-style verification with findings-first manual review and stronger testability/refactor scrutiny |

### Supporting Workflows

| Skill | Description |
|-------|-------------|
| `auto-legion` | Parallel agent waves within a stream (T→I→D→R phases) |
| `auto-workflow` | TDD enforcement, verification before completion, architecture escalation |
| `triumvirate` | Adversarial plan review with three subagents (Advocate, Analyst, Critic) |
| `review` | Code review across 9 dimensions (security, logic, tech debt, etc.) |
| `codex-validation` | Findings-first final validation with stronger manual audit, cross-file impact checking, and testability/refactor focus |
| `codex-plan-refinement` | Codex-side plan refinement for clarity, dependency sanity, reuse, abstraction quality, and compression before execution |
| `plan-validate` | Validate multi-stream plans for structure, dependencies, ownership, required skills, verification, and final validation mode before execution |
| `design` | UI/UX design system with auditing, generation, and style migration |
| `skill-creator` | Create, modify, eval, and benchmark skills |
| `security-scan` | Active vulnerability scanner (dangerous patterns, secrets, npm audit) |
| `auto-web-validation` | Prompt-injection-aware web research discipline for package/docs/vendor sources and cited recommendations |

### Coding Disciplines (auto-*)

| Skill | Description |
|-------|-------------|
| `auto-coding` | Language-agnostic code quality, clarity, anti-over-engineering |
| `auto-comments` | When to comment, when silence is the comment |
| `auto-naming` | Domain vocabulary over generic words, verb semantics |
| `auto-hardcoding` | No hardcoded URLs, ports, timeouts, or magic numbers |
| `auto-silent-defaults` | When defaults mask errors and missing data should fail loudly |
| `auto-errors` | Actionable error messages, audience-appropriate wording |
| `auto-logging` | Log level selection, structured fields, what to log vs not |
| `auto-edge-cases` | Empty collections, zero inputs, off-by-one, overflow, Unicode |
| `auto-test-quality` | Meaningful assertions, mock boundaries, tautological test detection |
| `auto-testability` | Extract logic into clean seams so business rules can be tested directly instead of through brittle orchestration |
| `auto-sanity` | Senior-level readability and maintainability checks for code that works but may be calcifying structurally |
| `auto-concurrency` | Race conditions, atomicity, lock ordering, TOCTOU bugs |
| `auto-resource-lifecycle` | Guaranteed cleanup on all paths, RAII/context managers |
| `auto-resilience` | Timeouts, retries with backoff, circuit breaking, idempotency |
| `auto-caching` | Stampede protection, invalidation strategy, stale-while-revalidate |
| `auto-file-io` | Atomic writes, streaming large files, error path cleanup |
| `auto-state-machines` | Explicit state enums, transition validation, impossible state elimination |
| `auto-serialization` | Decimal precision, timezone-aware datetimes, forwards-compatible enums |
| `auto-evolution` | Backwards-compatible schema/API changes, rolling deploy safety |
| `auto-observability` | Metrics vs logs vs traces, health check depth, SLO-oriented measurement |
| `auto-database` | Cursor pagination, index awareness, N+1 prevention, bulk operations |
| `auto-api-design` | Response envelopes, HTTP status codes, cursor pagination, DTOs |
| `auto-job-queue` | Idempotent processing, poison pill protection, dead letter handling |
| `auto-security` | Session token hashing, auth hiding, timing-safe flows, cookie hardening |
| `auto-compliance` | GPC headers, data deletion gates, consent proof, regulatory escalation |
| `auto-accessibility` | ARIA completeness, touch targets, forced-colors/reduced-motion, WCAG 2.2 |
| `auto-layout` | Card restraint, grid over flex for 2D, design tokens, z-index management |
| `auto-i18n` | ICU pluralization, locale-aware formatting, RTL support |

### Language-Specific

| Skill | Description |
|-------|-------------|
| `auto-typescript` | Type safety — eliminates `as any`, enforces narrowing, branded types, Zod pitfalls |
| `auto-python` | Type hints, async patterns, pytest, dataclasses, uv/ruff/mypy |
| `auto-svelte` | Svelte 5 gotchas — SSR state, `$state.raw`, `$effect` discipline |

## Architecture

The system is organized in three layers:

### Layer 1: Claude Entry Points

`/summon`, `/stream`, and `/dominion` are the core human entry points in Claude Code.

### Layer 2: Supporting Workflows

Supporting workflows such as `triumvirate`, `review`, `codex-validation`, `plan-validate`, and `design` are intentionally fewer and more deliberate. They are not meant to compete with the primary entry points.

### Layer 3: `auto-*` — Discipline Skills

Auto-triggered coding standards activate based on what you're doing. Writing a database query? `auto-database` loads. Growing a route into a monolith? `auto-testability` and `auto-sanity` should push extraction and cleanup. These skills encode the patterns the model knows but applies inconsistently, making the quality floor more reliable.

## Key Concepts

### Streams

A stream is an independent unit of work within a plan. Each stream has file ownership boundaries (no two streams edit the same file), explicit dependencies on other streams, and a set of tasks. Streams can execute in parallel when they have no dependency relationship.

### Legion Waves (T→I→D→R)

Legion decomposes a stream into phased waves following TDD progression:

- **T (Test)** — Write tests first, in parallel per module
- **I (Implement)** — Implement against the tests, in parallel per module
- **D (Debug)** — Fix any failing tests
- **R (Refine)** — Polish, optimize, clean up

Each wave dispatches multiple background agents with minimal, surgical context. The orchestrator verifies between waves before proceeding.

### Dependency Optimization

Plans declare stream dependencies explicitly. `/dominion` builds a DAG and identifies the maximum parallelism — streams with no shared dependencies run simultaneously. The parallelization section of a plan shows which streams can overlap.

### The Status File

Each plan gets a `.status.json` companion file that tracks which streams are complete, in-progress, or blocked. This allows `/stream` to resume across sessions and `/dominion` to monitor progress across its spawned instances.

### Zero-Tolerance Helper Mode

Corvalis discipline skills don't suggest improvements — they enforce them. When a skill detects a violation (e.g., `SELECT *` in a query, a bare network call without timeout), it corrects the code directly rather than leaving a comment. The quality floor is non-negotiable.

### The Skill Gate

During planning, `/summon` assigns a concrete list of auto-* skills to each stream. A baseline set (`auto-workflow`, `auto-coding`, `auto-errors`, `auto-naming`, `auto-edge-cases`) loads unconditionally for every stream. Additional skills are assigned per-stream based on what that stream touches — the user reviews and approves the assignments. These are written into the plan's `## Required Skills` section and flow into the status file's `baselineSkills` field, so `/stream` loads exactly the right skills without heuristic guessing.

### The Parallelization Gate

Before `/dominion` spawns parallel streams, it validates that file ownership boundaries don't overlap. If two streams touch the same file, they cannot run in parallel regardless of their declared dependencies. This prevents merge conflicts and race conditions in the codebase.
