---
name: stream
description: "Multi-stream plan execution coordinator. Tracks stream progress across sessions, manages dependencies, auto-loads relevant skills, and enforces verification gates. User-invocable via /stream command. Triggers: stream, next stream, stream status, claim stream, continue plan."
---

# /stream — Multi-Stream Plan Executor

Coordinates multi-stream plan execution across sessions. Each session claims one stream, loads the right skills, executes with verification, and marks completion.

## When to Use

- Any plan with `## Stream` headers (1 or more streams)
- Work that benefits from fresh context between streams
- Streams with dependency relationships or file ownership boundaries

## When NOT to Use

- Plans without `## Stream` headers → not a stream-based plan, use `/summon` no-plan path
- Mid-stream work → this skill is for session start/end, not mid-implementation

## Invocation

```
/stream docs/plans/slug.md     # Explicit plan file
/stream                         # Smart auto-detect
/stream --status                # Show progress dashboard
```

---

## Phase 1: Resolve Plan File and Ensure Status File

### Step 1: Resolve the plan

**Explicit path provided:** Read the file.

**Smart auto-detect (no args):** Follow this cascade:

1. **Check active status files:** Glob `docs/plans/*.status.json`. For each, read and check if any streams are not `completed`. If exactly one active plan → use it. If multiple → ask user which plan.

2. **Check git for recent plans:** If no status files found, run:
   ```bash
   git log --diff-filter=A --name-only --pretty="" -5 -- "docs/plans/*.md"
   ```
   Also check: `git diff --name-only HEAD~5 -- "docs/plans/*.md"`. Combine results, deduplicate.

3. **Single recent plan with 2+ streams:** Read the file, check for `## Stream` headers. If it qualifies → offer to initialize.

4. **Multiple recent plans:** List each with title and date, ask user to pick.

5. **No plans found:** `"No multi-stream plans found. Run /summon to create one."`

**`--status` flag:** Read all `.status.json` files in `docs/plans/` and display a dashboard showing each stream's status, blocked/eligible state, and overall progress.

### Step 2: Ensure status file exists

Once a plan is resolved, immediately check for its companion `.status.json`. If it exists, load it and proceed. If not, create it now:

1. Parse all stream headers matching `## Stream N:` or `## Stream N —`
2. For each stream, extract:
   - **Name:** text after the stream number
   - **Dependencies:** from `**Dependencies:**` line
   - **Files owned:** from `**Files owned:**` line
   - **Sub-streams:** from `### NA.` patterns
3. Parse the `## Required Skills` section (if present):
   - Extract **Baseline** skills (apply to all streams)
   - Extract **Per-Stream** skill assignments from the table
   - Combine baseline + per-stream into a `baselineSkills` array for each stream
   - If no `## Required Skills` section exists, set `baselineSkills` to `[]` (triggers conditional loading fallback in Phase 3)
4. Parse the `## Final Validation Mode` section (if present):
   - `Mode: codex` → final validation uses `codex-validation`
   - `Mode: review` → final validation uses classic `review`
   - If absent, default to `review`
5. Write `docs/plans/{slug}.status.json` with all streams set to `pending`, including `baselineSkills` per stream and the selected `finalValidationMode`
6. **Auto-inject final stream:**
   - `review` mode → add a `"final"` stream named `Final Validation`. This stream handles full verification, classic review, git commit/push, and plan cleanup.
   - `codex` mode → add a `"final"` stream named `Final Cleanup`. This stream handles full verification plus Claude-side cleanup and improvement work, but it does **not** run `codex-validation`, commit, push, or clean up the plan/status files. Those are preserved for the Codex `/verify` handoff.
7. Display the dependency graph

See `references/status-schema.md` for the full JSON schema.

---

## Phase 2: Claim a Stream

### Auto-selection

1. Find all streams where `status` is `pending`
2. Filter to streams whose dependencies are ALL `completed`
3. From eligible set, pick the **lowest-numbered** stream
4. If no eligible streams:
   - All completed → announce plan completion (Phase 6)
   - Some `in_progress` → **assume another session is actively working on them** (see below)
   - Dependencies unmet → enter **Dependency Wait** (see below)

### In-Progress Streams (Another Session Is Working)

When streams show `in_progress`, **assume another Claude session is actively working on them.** Do NOT attempt to resume or take over. Report the situation and let the user decide.

**Only resume an `in_progress` stream when the user explicitly says** one of:
- "take over stream N"
- "resume stream N"
- "the other session is done/dead/crashed"

### Multiple eligible streams

**If the user's prompt specifies a stream** (e.g. "claim stream 3", "execute stream 5", or the prompt ends with "— claim and execute Stream N"): claim that specific stream immediately. Do NOT list options or ask — just claim and execute.

**Otherwise** (no stream specified): List eligible streams and note they can run in parallel terminals. Let the user pick.

### Dependency Wait

When the next logical stream has unmet dependencies, report which deps are incomplete and wait for user confirmation before proceeding.

### Override Verification

When the user says a dependency is done:

1. Read the dependency stream's **Files owned** from the plan
2. Check file existence via Glob
3. Run the project's type checker and relevant tests
4. If all pass → mark dependency as `completed` and proceed
5. If any fail → report failures, do NOT proceed

### Claiming

Once a stream is selected and dependencies are met:

1. Set `status: "in_progress"` and record `claimedAt` timestamp in status file
2. Announce the claimed stream, dependencies status, and files owned

---

## Phase 3: Load Skills

This happens BEFORE any implementation.

### Baseline skills from status file (preferred path)

If the status file contains a `baselineSkills` array for this stream, load **exactly** those skills plus the always-load set. Do NOT fall back to keyword matching.

1. Load the always-load set (see below)
2. Load every skill listed in the stream's `baselineSkills` array
3. If the stream has `**Legion:** Yes`, also load `auto-legion`
4. Skip the conditional loading section entirely

### Always load

These load for every stream, regardless of `baselineSkills`:

- `auto-workflow` (execution, TDD, verification)
- `auto-coding` (code quality)
- `auto-errors` (error handling discipline)
- `auto-naming` (naming discipline)
- `auto-edge-cases` (boundary handling)

### Final stream override

If the claimed stream is the auto-generated final stream (`"final"`), load ONLY:
- `auto-workflow`
- `auto-coding`
- `review` when the status/plan mode is `review`

Then proceed directly to Phase 4F.

### Legion loading

If the stream has a `**Legion:** Yes` annotation in the plan (from `/summon`'s legion gate), also load:
- `auto-legion` (orchestrator discipline, wave management)

### Conditional loading (fallback only)

Use this only when the status file has no `baselineSkills` for the stream.

Analyze the stream section from the plan. Extract all file paths and keywords, then match:

| Pattern | Skills to Load |
|---------|---------------|
| `*.svelte`, `+page.svelte`, `+layout.svelte` | auto-svelte, auto-accessibility, auto-layout |
| `*.css`, `*.scss`, Tailwind classes, StyleSheet | auto-layout |
| `*.ts`, TypeScript code | auto-typescript |
| Keywords: auth, session, password, encrypt, permission, sensitive, PII | auto-security |
| Keywords: PII, audit, consent, retention, GDPR | auto-compliance |
| `*.py`, Python code | auto-python |
| Keywords: log, tracing, observability, span, instrument | auto-logging |
| Keywords: comment, docstring, documentation, complex algorithm | auto-comments |
| Keywords: async, spawn, mutex, concurrent, shared state, channel | auto-concurrency |
| Keywords: test, spec, assertion, mock, vitest, pytest, proptest | auto-test-quality |
| Keywords: config, env, fallback, default, optional | auto-silent-defaults |
| Keywords: file, connection, pool, listener, cleanup, shutdown | auto-resource-lifecycle |
| Keywords: url, port, timeout, config, env, secret, api key, localhost | auto-hardcoding |
| Keywords: fetch, request, webhook, retry, timeout, external API, delivery | auto-resilience |
| Keywords: endpoint, handler, route, REST, response, pagination, DTO | auto-api-design |
| Keywords: query, SQL, SELECT, INSERT, UPDATE, JOIN, ORM, sqlx, prisma | auto-database |
| Keywords: migration, rename, schema, breaking change, deprecate, column, evolution | auto-evolution |
| Keywords: serialize, deserialize, serde, json, payload, precision, decimal, timestamp | auto-serialization |
| Keywords: cache, caching, TTL, invalidate, stale, memoize, redis cache | auto-caching |
| Keywords: job, queue, worker, task, background, dequeue, enqueue, retry, dead letter | auto-job-queue |
| Keywords: metrics, health check, tracing, span, SLO, prometheus, opentelemetry, monitor | auto-observability |
| Keywords: file, write file, read file, atomic write, temp file, upload, streaming | auto-file-io |
| Keywords: state machine, state, status, transition, workflow, lifecycle, FSM | auto-state-machines |
| Keywords: i18n, locale, translation, plural, ICU, MessageFormat, Fluent, RTL, Intl, l10n | auto-i18n |
| Keywords: UI, UX, design, component, page, layout, color, typography, style, animation, form, chart, navigation | ui-ux-pro-max, auto-layout, auto-accessibility |

### ui-ux-pro-max integration

When `ui-ux-pro-max` is in a stream's `baselineSkills` (or matched via conditional loading), the stream gains design intelligence capabilities:

1. **At stream start:** Load the skill and check for `**Design domains:**` annotations in the plan's stream section
2. **Before implementation:** Run the specified domain searches to inform design decisions:
   ```bash
   python3 skills/ui-ux-pro-max/scripts/search.py "<keywords>" --domain <domain>
   ```
3. **If a `**Stack:**` annotation exists:** Also run the stack-specific search:
   ```bash
   python3 skills/ui-ux-pro-max/scripts/search.py "<keywords>" --stack <stack>
   ```
4. **During verification:** Apply the ui-ux-pro-max Quick Reference checklist (accessibility, touch, performance, style, layout, typography, animation, forms, navigation, charts) as an additional verification gate alongside type checking and tests
5. **If `design-system/MASTER.md` exists:** Read it at stream start and apply its design system rules throughout implementation

### ui-ux-pro-max execution gates (mandatory when loaded)

When `ui-ux-pro-max` is in the stream's required skills, the stream MUST produce three artifact files as proof of execution. These are not optional documentation — they are verification artifacts that `/dominion` Phase 2.3.5 will check. A stream that skips them will be flagged as incomplete and re-spawned or manually cleaned up.

**Artifact A — Pre-implementation design search log**

File: `docs/plans/.dominion-logs/stream-{N}-design-search.md`

Written BEFORE any component code is edited. This is the stream's first action after loading skills.

Contents:
- The exact search commands run (`python3 skills/ui-ux-pro-max/scripts/search.py ...`)
- The raw results of each search (paste stdout, don't summarize)
- One short paragraph per search noting which patterns the stream plans to apply
- If the stream has a `**Stack:**` annotation, both the domain search AND the stack search must appear
- If `design-system/MASTER.md` exists in the repo, a note confirming it was read and summarizing the constraints the stream will respect
- If the stream intentionally skips a search (e.g., no new components, just copy changes), it must say so explicitly with a reason

Empty or missing file = incomplete stream. Not optional.

**Artifact B — Per-change design decisions log**

File: `docs/plans/.dominion-logs/stream-{N}-design-decisions.md`

Written incrementally as the stream edits files. For every non-trivial component / layout / style edit, append one entry:

```
### <file path>
- Category: <touch | accessibility | performance | style | layout | typography | animation | forms | navigation | charts>
- Decision: <one sentence — what the stream did>
- Rule applied: <cite the ui-ux-pro-max rule name or checklist line>
```

Small mechanical edits (prop threading, type fixes, import adjustments) don't need an entry. Any edit that affects visual behavior, interaction, accessibility, or layout does.

**Artifact C — Final Quick Reference checklist**

File: `docs/plans/.dominion-logs/stream-{N}-checklist.md`

Written at the end, before the stream marks itself complete. Must enumerate all 10 Quick Reference categories and for each, state either:
- `Applied: <specific evidence — which files, which checks>`
- `N/A: <specific reason — why this category didn't apply to this stream>`

The 10 categories: accessibility, touch & interaction, performance, style selection, layout & responsive, typography & color, animation, forms & feedback, navigation patterns, charts & data.

If more than 3 categories are `N/A`, include a top-line note explaining why so little of the skill was relevant. If the answer is "this wasn't really a UI stream," the skill probably shouldn't have been in the required-skills list.

**Completion gate:** A stream with `ui-ux-pro-max` in its required skills CANNOT mark its status as `completed` without all three artifacts existing and being non-empty. Check before the final status-file write in Phase 5.3.

### Load the skills

Invoke each skill via the Skill tool. Do this BEFORE reading any implementation tasks.

**When `ui-ux-pro-max` is loaded**, the stream's first action after loading all skills (before any code reads or edits) must be to write Artifact A (the design search log). This forces design search to happen upstream of implementation, not after the fact as a rationalization.

Hard rule: if this stream session needs any web research, package/library search, vendor-doc lookup, or source-backed recommendation work, load `auto-web-validation` first before doing that research.

---

## Phase 4: Execute the Stream

**If the claimed stream is the Final Validation stream (`"final"`), skip to Phase 4F below.**

**If the stream has `**Legion:** Yes` in the plan, skip to Phase 4L below.**

Hand off to auto-workflow's executing-plans process with these stream-specific additions:

### 4.1 Scope Enforcement

Before starting, explicitly state which files you WILL and will NOT touch. Respect these boundaries throughout the session.

### 4.2 Incremental Verification

Track file edit count. After every **3 file edits** (complex) or **5 file edits** (simple), run the type checker / linter. If it reports errors: **stop and fix** before editing more files.

**Parallel stream awareness:** Before fixing any error, check whether the erroring file is owned by another `in_progress` stream. If so, **skip it** — that stream is responsible.

### 4.3 Sub-stream Sequencing

If the stream has sub-streams (e.g., 2A, 2B):
- Execute in alphabetical order (A before B)
- Run verification between sub-streams
- Update the status file checkpoint after each sub-stream completes

### 4.4 Follow TDD and Execution Standards

All implementation follows auto-workflow's executing-plans process:
- Batch execution (3 tasks default)
- TDD: failing test first, then implement
- Report between batches
- Stop on blockers

### 4.5 Smoke Tests

After implementing each API endpoint or page, run a quick smoke test against the live dev server to verify it works end-to-end. If the dev server isn't running, ask the user to start it.

---

## Phase 4L: Legion Execution Protocol

This phase executes only for streams annotated with `**Legion:** Yes`. You are the **orchestrator** — follow `auto-legion`.

### 4L.1 Context Gathering

Read ALL files in the stream's scope. Build a mental model of:
- Interfaces and types the stream's code must satisfy
- Existing code patterns in the project
- Dependencies between tasks in the stream

This is the only time you read files. After this, craft prompts from what you learned.

### 4L.2 Decompose Into Waves

Follow `auto-legion`'s decomposition algorithm. Using the stream task list and the suggested wave structure, produce the wave plan:

1. Parse the plan's suggested waves (e.g., `Wave T: 3 agents → Wave I: 3 agents → Wave D: 2 agents`)
2. Refine based on what you learned in 5L.1 — the plan's suggestion is a starting point, not gospel
3. Assign specific files and tasks to each agent in each wave
4. Identify interfaces/types to paste into agent prompts

Update the status file with the legion wave structure.

### 4L.3 Execute Waves

For each wave, in order (T → I → D → R):

**Dispatch:** Craft focused prompts and dispatch ALL agents in the wave simultaneously using the Agent tool with `run_in_background: true`. All Agent calls MUST be in a single message.

**Wait:** Agents complete in background. You are notified when each finishes.

**Collect:** Read each agent's output. Note successes, failures, and any reported issues.

**Verify:** Run project-wide verification:
```bash
npx tsc --noEmit                    # Type check
npx vitest run <stream file paths>  # Tests for this stream
```

**For Wave T (tests):** Verification means tests exist and FAIL (red phase). Type errors are blockers; test failures are expected.

**For Wave I (implementation):** Verification means tests PASS (green phase) and type check is clean.

**For Wave D (dependents):** Full verification — types, tests, and build.

**Fix:** If verification fails:
- Read the failing files
- If 1-2 small issues: fix them directly (orchestrator handles it)
- If an agent's entire output is wrong: re-dispatch that single agent with error context (max 2 retries)
- If systemic failure: fall back to solo execution for remaining tasks

Update the status file after each wave completes.

### 4L.4 Assembly Check

After all waves complete, run the full verification gate (same as Phase 5.1). The orchestrator reviews all agent-produced code as a whole:

- Do modules integrate correctly?
- Are imports consistent?
- Any naming conflicts between agent outputs?

Fix any integration issues directly — these are typically small (import paths, naming alignment).

### 4L.5 Solo Fallback

If legion execution cannot complete (2 consecutive wave failures, unresolvable agent conflicts):

1. Update status: `"legion": { "enabled": false, "fallbackReason": "..." }`
2. Load remaining tasks into context
3. Execute remaining tasks directly using standard Phase 4 process
4. This is not a failure — some tasks resist decomposition

---

## Phase 4F: Final Validation Protocol

This phase executes **only** for the auto-generated Final Validation stream (`"final"`).

### 4F.1 Full Project Verification — Zero Tolerance Mode

Run the complete verification suite. **Zero errors AND zero warnings.** Warnings are not acceptable — they indicate code smell, unused imports, type loose ends, or accessibility gaps that compound over time.

| Check | Must |
|-------|------|
| Type check / lint | Exit 0, **zero errors, zero warnings** |
| Tests | All pass, zero skipped |
| Build | Exit 0, **zero warnings** |
| Smoke tests | All endpoints return expected responses |

If any check fails OR produces warnings:

1. **Enter helper mode** — systematically fix every error and warning
2. Group issues by type (unused imports, type mismatches, missing return types, etc.)
3. Fix all issues in the same category together (batch efficiency)
4. Re-run the full suite after each batch of fixes
5. **Loop until completely clean** — no errors, no warnings, no skipped tests
6. Only proceed to 4F.2 when the entire suite is spotless

Helper mode is not optional. The Final Validation stream does not complete until the project is clean.

### 4F.2 Validation Review

Run the selected validation mode on all uncommitted changes:

- **Mode: `review`** → run `/review`

For classic `/review`, fix all **issues** and **suggestions**. Nitpicks are optional.

If mode is `codex`, replace this step with **Claude Cleanup Review**:

- perform one more broad pass over the changed areas and surrounding context
- fix obvious correctness, readability, maintainability, warning-cleanup, and testability issues you can confidently improve
- do **not** run `codex-validation` here
- do **not** treat this as the final nitpick pass; Codex `/verify` is the stricter downstream audit

### 4F.3 Git Commit & Push

If mode is `codex`, skip this step entirely. Do not commit or push. Preserve the working tree for Codex.

1. `git status` and `git diff` to review changes
2. Stage specific files (never `git add .` — skip `.env`, `node_modules/`, build artifacts)
3. Write a conventional commit message (feat/fix/refactor with scope and subject)
4. Commit and push

### 4F.4 Cleanup

If mode is `codex`, skip this step entirely. Do not delete the plan or status files.

After successful commit and push:
1. Delete the plan file: `docs/plans/YYYY-MM-DD-<slug>.md`
2. Delete the status file: `docs/plans/YYYY-MM-DD-<slug>.status.json`

### 4F.5 Announce Completion

If mode is `review`, report: commit hash, branch, push status, summary of all streams completed, files cleaned up.

If mode is `codex`, report:
- Final Cleanup completed
- no commit/push performed
- plan and status files preserved
- next step is to open Codex and run `/verify`

---

## Phase 5: Complete the Stream

When all tasks in the stream are implemented:

### 5.1 Verification Gate (non-negotiable)

Run all of these **project-wide**:

| Check | Scope | Must |
|-------|-------|------|
| Type check / lint | Entire project | Exit 0 — OR all remaining errors belong to other active streams (see 6.2) |
| Tests | Entire project | All pass — OR all failures are in files owned by other active streams |
| Build | Entire project | Exit 0 (no exceptions) |
| Smoke tests | Stream's endpoints | All return expected responses |

### 5.2 Fix Pre-existing Errors (Parallel-Aware)

If verification surfaces errors **outside your stream's files**:

1. Read the status file — identify all `in_progress` streams (other than yours)
2. Read each active stream's **Files owned** from the plan
3. Classify each error:
   - **Owned by another active stream** → **SKIP.** That stream will fix its own errors.
   - **Not owned by any active stream** → **Fix it.**

If all remaining errors belong to other active streams, your stream may pass verification with a note listing the skipped errors.

### 5.3 Mark Complete

**ui-ux-pro-max artifact gate:** If `ui-ux-pro-max` is in this stream's required skills, verify all three artifacts exist and are non-empty before proceeding:
- `docs/plans/.dominion-logs/stream-{N}-design-search.md` — must exist, > 200 bytes
- `docs/plans/.dominion-logs/stream-{N}-design-decisions.md` — must exist, at least one entry per modified `.svelte` / `.tsx` / `.jsx` / `.css` file
- `docs/plans/.dominion-logs/stream-{N}-checklist.md` — must exist, must mention all 10 Quick Reference categories by name

If any artifact is missing or empty, do NOT mark complete. Write the missing artifact(s) first.

Update the status file: set `status: "completed"`, record `completedAt` timestamp and verification results.

### 5.4 Announce and Prompt Next Session

Report verification results, remaining streams and their status, overall progress. Prompt: "Clear context and run /stream to pick up the next stream."

### 5.5 Failed Verification

If any check fails: report the failure, do NOT mark as completed, keep `status: "in_progress"`.

---

## Phase 6: Plan Complete

When ALL streams (including Final Validation) have `status: "completed"`:

If reached via Final Validation, the 4F.5 announcement is the primary output.

If reached because Final Validation was skipped, warn the user to run verification, validation/review, commit/push, and cleanup manually.

---

## Edge Cases

### In-progress stream from another session
Assume another session is working on it. Only resume on explicit user request.

### Parallel-eligible streams
If the prompt specifies which stream to claim, claim it immediately. Otherwise, list them and let the user pick. The status file prevents double-claiming.

### Single-stream plan
Full Phase 1-6 workflow applies. Phase 2 auto-selects the single stream.

### Plan file changed after status file created
Compare stream headers against status file. If mismatched, offer to add new streams while preserving existing progress.

---

## Rules

1. **NEVER** start implementing before loading relevant skills
2. **NEVER** skip incremental verification
3. **NEVER** mark a stream complete without fresh verification evidence
4. **NEVER** touch files owned by other `in_progress` streams
5. **NEVER** skip the Final Validation stream
6. **ALWAYS** read the status file before claiming
7. **ALWAYS** verify dependencies before starting blocked streams
8. **ALWAYS** prompt the user to clear context and run `/stream` after completing a stream
9. **ALWAYS** load `auto-web-validation` before any web search, package search, or vendor/library research in `/stream`
10. The status file is the **single source of truth**
11. The plan file is **read-only**
12. The Final Validation stream deletes both plan and status files after successful commit/push
13. When `ui-ux-pro-max` is in a stream's required skills, **ALL THREE** design artifacts must exist and be non-empty before the stream can mark `completed`

## Rationalization Prevention

| You're thinking... | Reality |
|---|---|
| "I read the ui-ux-pro-max instructions, I don't need to write the search log" | The search log is not documentation — it's proof the search actually ran. Reading the skill into context and running its scripts are two different things. Without the log, the orchestrator assumes the search didn't run. |
| "None of the categories applied, I can skip the checklist" | Then ui-ux-pro-max shouldn't be in the required-skills list for this stream. If it IS in the list, the checklist must enumerate each category with reasoning. Blank checklist = incomplete stream. |
