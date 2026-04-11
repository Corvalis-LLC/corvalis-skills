---
name: dominion
description: "Autonomous plan executor. Reads a multi-stream plan, orchestrates the entire execution by spawning headless Claude instances that run /stream — one per eligible stream, in parallel where dependencies allow. Monitors status files, spawns next waves when streams complete, and reports final results. User-invocable via /dominion command. The user's alternative to manually running /stream in separate terminals. Triggers: dominion, auto-execute, run all streams, execute plan."
---

# /dominion — Autonomous Plan Orchestrator

`/dominion` is the hands-off execution layer. Where `/stream` executes one stream per session, `/dominion` runs the **entire plan** autonomously by spawning headless `/stream` instances and cascading phase by phase.

```
/summon  → creates and validates the plan
/dominion → executes the entire plan autonomously
/stream  → executes one stream at a time (manual alternative to /dominion)
```

Use `/dominion` for full autonomy and `/stream` for hands-on control.

## When to Use

- Plans with 2+ streams where the user wants to walk away
- Plans with parallel-eligible streams (maximum time savings)
- After `/summon` has finalized a plan with the parallelization section

## When NOT to Use

- Single-stream plans (just run `/stream` — dominion overhead isn't worth it)
- When the user wants to review between streams
- When the plan involves risky operations that need human judgment between streams (destructive migrations, external API changes, production deployments)

## Invocation

```
/dominion                              # Auto-detect plan
/dominion docs/plans/slug.md           # Explicit plan file
/dominion --status                     # Show live progress
/dominion --dry-run                    # Show execution schedule without spawning
```

---

## Phase 1: Resolve Plan, Ensure Status File, and Validate

### Step 1: Resolve the plan

Same cascade as `/stream` Phase 1 — check active status files, then recent plans, then ask user.

### Step 2: Ensure status file exists

Once a plan is resolved, immediately check for its companion `.status.json`. If it exists, load it. If not, create it:

1. Parse all stream headers matching `## Stream N:` or `## Stream N —`
2. For each stream, extract: name, dependencies, files owned, sub-streams
3. Parse the `## Required Skills` section (if present):
   - Extract **Baseline** skills (apply to all streams)
   - Extract **Per-Stream** skill assignments from the table
   - Combine baseline + per-stream into a `baselineSkills` array for each stream
   - If no `## Required Skills` section exists, set `baselineSkills` to `[]` (triggers conditional loading fallback in `/stream` Phase 3)
4. Parse the `## Final Validation Mode` section (if present):
   - `Mode: codex` → final validation uses `codex-validation`
   - `Mode: review` → final validation uses classic `review`
   - If absent, default to `review`
5. Write `docs/plans/{slug}.status.json` with all streams set to `pending`, including `baselineSkills` per stream and the selected `finalValidationMode`
6. Final validation handling depends on mode:
   - `review` mode → auto-inject `Final Validation` stream with dependencies on ALL other stream IDs
   - `codex` mode → auto-inject `Final Cleanup` stream with dependencies on ALL other stream IDs. This is the last Claude stream. After it completes, dominion stops and hands off to Codex `/verify`.

### Step 3: Pre-flight validation

Before spawning anything, verify:

1. **Plan has a `## Parallelization` section** — if not, warn the user and offer to run the legion gate now
2. **Plan has 2+ streams** — if single-stream, suggest `/stream` instead
3. **Execution schedule is parseable** — the `### Execution Schedule` from the parallelization section defines the phases
4. **No streams are currently `in_progress`** — if any are, another dominion/stream session may be active. Ask the user before proceeding.

### Step 4: Show execution preview

```
Plan: docs/plans/2026-03-31-feature-overhaul.md
Execution schedule (from parallelization section):

  Phase 1: Stream 1 (Foundation)              — 1 instance
  Phase 2: Streams 2, 3, 4 (parallel)         — 3 instances
  Phase 3: Streams 5, 6 (parallel)            — 2 instances
  Phase 4: Final Validation                   — 1 instance

If final validation mode is `codex`, show this instead:

  Phase 4: Final Cleanup                      — 1 Claude instance
  Phase 5: Codex handoff                      — no Claude stream spawned

Total: 4-5 phases, max 3 concurrent instances
Estimated: ~4-5 stream durations (vs ~7 sequential)

Proceed? [Y/n]
```

Wait for user confirmation before spawning.

---

## Phase 2: Execute Phases

For each phase in the execution schedule:

### 2.1 Identify Eligible Streams

Read the status file. Find all streams where:
- `status` is `pending`
- All dependencies are `completed`

Cross-reference with the execution schedule to determine which streams belong to the current phase.

### 2.2 Spawn Headless Instances

For each eligible stream, spawn a headless Claude instance:

```bash
cd {project_root} && claude -p "/stream {plan_file} --claim {stream_number}" \
  --model sonnet \
  --allowedTools "Bash,Read,Write,Edit,Glob,Grep,Skill,Agent" \
  < /dev/null > {log_file} 2>&1 &
```

Key details:
- `--claim {stream_number}` — tells `/stream` exactly which stream to claim, bypassing auto-selection and user prompts. **Critical for headless execution** — without this, `/stream` may prompt for user input when multiple streams are eligible, which hangs the headless process (stdin is `/dev/null`).
- `--model sonnet` — headless streams run on Sonnet for cost efficiency. Dominion itself runs on Opus for orchestration judgment, but execution streams follow a plan and don't need the strongest model.
- `< /dev/null` — prevents stdin warning
- `> {log_file}` — captures output for monitoring and debugging
- `&` — backgrounds the process
- Log files go to `docs/plans/.dominion-logs/{stream_id}.log`

Spawn ALL eligible streams in a single Bash command using `&` for each. Capture each PID with `$!`, then set up background wait commands per stream.

### 2.3 Wait for Process Exit

Wait for process exit, then verify with Read. Do not poll the status file in a loop.

For **each spawned stream**, immediately run a background wait command:

```bash
while ps -p $PID -o pid= > /dev/null 2>&1; do sleep 30; done; echo "STREAM $STREAM_ID COMPLETE"
```

Run this with `run_in_background: true`. When the task notification arrives, read the status file once to confirm completion and check results.

For **parallel phases** with multiple streams, you have two options:

**Option A — Per-stream notifications (preferred):** One `while ps -p` per stream, each with `run_in_background: true`. You get notified as each stream finishes and can report incrementally.

**Option B — Phase-level wait:** Collect all PIDs and wait for the entire phase:

```bash
wait $PID1 $PID2 $PID3; echo "PHASE COMPLETE"
```

Use Option A when the user is watching; Option B when they've walked away.

**On process exit notification:**

1. Read `docs/plans/{slug}.status.json` once with the Read tool
2. Check stream status — confirm `completed` or detect failure
3. Run `git diff --stat` to summarize what the stream produced
4. Report results to the user

```
[03:45:12] Phase 2 — 3 instances spawned
[03:52:30] Stream 4: completed ✓ (took 8m, 4 files changed)
[03:54:15] Stream 2: completed ✓ (took 11m, 7 files changed)
[03:56:40] Stream 3: completed ✓ (took 12m, 3 files changed)
[03:56:40] Phase 2 complete — all 3 streams done
```

### 2.3.5 Claim Verification (MANDATORY)

**Run this for EVERY stream between process exit and phase transition. Never skip for "trusted" streams — there are no trusted streams.**

Stream self-reports are unreliable. A real run across 7 parallel Sonnet streams produced: every stream reported "completed" with clean tsc/test results, but 6 of 7 had material deviations from the plan's post-review amendments — including Stripe-impossible coupon codes, test files in `src/routes/` that broke SvelteKit, migration number collisions, and silent-fallback patterns the amendments explicitly forbade.

**For each completed stream:**

**a. Read the full stream log** at `docs/plans/.dominion-logs/stream-{N}.log` — not skim, read all of it. Extract every concrete claim the stream made (files touched, decisions taken, values used, tests added).

**b. Extract stream requirements from the plan.** Open the plan file and find that stream's section. Extract every requirement from Sub-tasks, Files, Smoke Test. Since the /summon inline-amendment flow puts all amendments directly into the stream sections, the stream section IS the authoritative requirements list — there is no separate amendments section to cross-reference.

**c. Verify compliance by reading the shipped code.** For each requirement:

| Amendment says... | Verification action |
|---|---|
| "Use X not Y" | `grep -rn 'X' src/` (confirm present), `grep -rn 'Y' src/` (confirm absent) |
| "Delete Z" | `grep -rn 'Z' src/` should return zero hits |
| Column constraint (NOT NULL, default) | Read the schema/migration file, confirm the constraint |
| Specific file path or env var | `ls` the path or `grep` for the env var gate |
| Function signature (takes A, B, C) | Read the function, count parameters |
| Hardcoded value to remove | `grep -rn 'VALUE' src/ docs/` should return zero hits |
| Specific test file location | `ls` the directory to verify no misplaced files |
| Cap/limit (e.g., "fan-out at 10") | Read the handler, confirm the constant and slice/limit |

**d. Build a deviation list.** For each deviation, note:
- Stream number
- Requirement (from plan)
- What the stream actually shipped
- Severity: **blocker** (breaks production/Stripe/builds), **correctness** (wrong behavior), **quality** (style/naming/minor)

**e. Act on deviations:**

| Deviation list | Action |
|---|---|
| Empty | Proceed to phase transition |
| Quality items only | Report to user, proceed. User decides whether to fix post-hoc |
| Blockers or correctness issues | **STOP.** Do not proceed to next phase. Report full list with `file:line` evidence. Ask user: (1) manual cleanup in this session, (2) spawn targeted cleanup stream with deviation-list prompt, (3) skip and accept, (4) cancel dominion. Default recommendation: (1) |

#### Verification Grep Patterns (Reference)

These are real patterns from the dominion run that revealed the problem:

```bash
# Amendment says coupon is TEST50CENT
grep -rn 'TEST1CENT' src/ docs/         # should return zero hits

# Amendment says column is NOT NULL
# Read schema file, look for .notNull() on the column

# Amendment says delete DEFAULT_X
grep -rn 'DEFAULT_X' src/               # should return zero hits

# Amendment says SameSite=Lax
grep -rn 'sameSite' src/                # confirm value is 'lax'

# Amendment says cap fan-out at 10
# Read the handler, confirm a constant and a slice

# Stream placed a test file in wrong location
ls src/routes/                           # verify no .test.ts files with + prefix
```

### 2.4 Phase Transition

When ALL streams in the current phase are `completed` **and have passed claim verification (2.3.5)**:

1. Read the status file one more time to confirm
2. Check if any new streams are now eligible (deps met)
3. If yes → proceed to next phase (back to 2.1)
4. If all streams complete → proceed to Phase 3

### 2.5 Failure Handling

**Stream process exits but status is still `in_progress`:**
- The process crashed or timed out before updating the status file
- Read the stream's log file to diagnose the failure
- Ask user: "Stream N process exited without completing. Options: (1) I'll diagnose and fix it, (2) retry the stream"
- If user chooses (1): read the log, diagnose, and fix directly
- If user chooses (2): reset stream to `pending`, spawn a fresh instance

**Headless process crashes immediately (wait loop exits within seconds):**
- The background wait notification arrives almost immediately after spawn
- Status file still shows `pending` — the process never claimed the stream
- Re-spawn the instance
- Max 2 retries per stream before asking the user

**Multiple streams fail in same phase:**
- If 2+ streams in the same phase fail, pause and ask the user
- Could indicate a systemic issue (broken dependency, bad plan)

---

## Phase 3: Final Validation

**Phase 3 is still required even if Phase 2.3.5 found zero deviations.** Claim verification (2.3.5) checks stream-level correctness against plan requirements. Final Validation checks cross-stream integration — the 9-dimension review, full build, commit, push, and cleanup. They are complementary, not redundant.

When all non-final streams are `completed` and verified (2.3.5):

If final validation mode is `review`:

1. Spawn one headless instance for the Final Validation stream
2. Monitor until complete
3. The Final Validation stream handles: full verification, selected validation mode (`review`), git commit/push, plan+status cleanup

If final validation mode is `codex`:

1. Spawn one headless instance for the Final Cleanup stream
2. Monitor until complete
3. The Final Cleanup stream handles: full verification, broad cleanup, and obvious improvement passes, but it does **not** run `codex-validation`, commit, push, or delete plan/status artifacts
4. After Final Cleanup completes, stop orchestration and hand off to the user to run Codex for the stricter final validation pass

Use language like:

```
Implementation and Claude cleanup streams complete.

Final validation mode is `codex`, so dominion will not spawn a Claude Codex-validation stream.

Next step:
  Open Codex in the same repo and run `/verify`

Artifacts preserved for Codex:
  - docs/plans/{slug}.md
  - docs/plans/{slug}.status.json
  - docs/plans/.dominion-logs/
```

---

## Phase 4: Report Results

After Final Validation completes (or if it fails):

### Success

```
/dominion complete ✓

Plan: docs/plans/2026-03-31-feature-overhaul.md
Streams: 7/7 completed
Duration: 34 minutes (vs ~2h sequential estimate)
Commit: abc1234
Branch: main

Phase breakdown:
  Phase 1 (Stream 1):        6 min
  Phase 2 (Streams 2,3,4):  12 min (parallel)
  Phase 3 (Streams 5,6):     9 min (parallel)
  Phase 4 (Final Validation): 7 min

Plan and status files cleaned up by Final Validation.
Logs available at: docs/plans/.dominion-logs/
```

### Codex Handoff

```
/dominion implementation and cleanup phases complete ✓

Plan: docs/plans/2026-03-31-feature-overhaul.md
Implementation streams: 6/6 completed
Final Cleanup: completed
Final validation mode: codex

No Claude Codex-validation stream was spawned.

Next step:
  Open Codex and run `/verify`

Preserved for Codex:
  - docs/plans/2026-03-31-feature-overhaul.md
  - docs/plans/2026-03-31-feature-overhaul.status.json
  - docs/plans/.dominion-logs/
```

### Failure

```
/dominion stopped — Stream 3 failed verification after 2 retries

Completed: Streams 1, 2, 4 (3/7)
Failed: Stream 3
Blocked: Streams 5, 6, Final Validation

Status file preserved: docs/plans/2026-03-31-feature-overhaul.status.json
Logs: docs/plans/.dominion-logs/

You can:
  1. Fix Stream 3 manually, then run /dominion to resume
  2. Run /stream to take over Stream 3 interactively
```

---

## Log Management

### Log directory

```
docs/plans/.dominion-logs/
  stream-1.log
  stream-2.log
  stream-3.log
  stream-final.log
```

Create this directory at dominion start. These logs are the debugging trail for failed streams.

### Log rotation

If a stream is retried, rename the old log:
```
stream-3.log → stream-3.attempt-1.log
```

### Cleanup

Logs are NOT deleted by Final Validation (unlike the plan and status files). They persist until the user deletes them manually. This is intentional — they're the audit trail.

---

## The Headless Command

The exact command spawned for each stream:

```bash
cd {project_root} && claude -p "/stream {plan_file} --claim {stream_number}" \
  --model sonnet \
  --allowedTools "Bash,Read,Write,Edit,Glob,Grep,Skill,Agent" \
  < /dev/null > docs/plans/.dominion-logs/stream-{id}.log 2>&1
```

### Why `--model sonnet`

Dominion runs on Opus (orchestration requires strong reasoning about phases, failures, and dependency resolution). Streams run on Sonnet — they follow an explicit plan with defined file ownership, loaded skills, and verification gates. The plan does the thinking; the stream does the work.

### Why `--claim {stream_number}`

Without `--claim`, `/stream` auto-selects the lowest-numbered eligible stream. When multiple streams are eligible (common in parallel phases), it prompts the user to pick. Headless instances have no user — stdin is `/dev/null` — so the prompt hangs indefinitely. `--claim` bypasses selection entirely: dominion knows exactly which stream each instance should run, so it tells them directly.

### Why these allowed tools

| Tool | Why |
|------|-----|
| `Bash` | Running tests, type checks, build, git |
| `Read` | Reading source files, plan file, status file |
| `Write` | Creating new files |
| `Edit` | Modifying existing files |
| `Glob` | Finding files by pattern |
| `Grep` | Searching code |
| `Skill` | Loading auto-* skills (critical — /stream loads many skills) |
| `Agent` | Legion mode — spawning background agent waves within a stream |

### Why NOT `--dangerouslySkipPermissions`

We use `--allowedTools` to whitelist specific tools rather than blanket-skipping permissions. Unexpected destructive actions stay blocked.

---

## Status File as Coordination Layer

`/dominion` and all spawned `/stream` instances share the status file as their coordination mechanism:

```
/dominion (this session)
  waits for process exit, then reads status.json to confirm
  writes: nothing (read-only coordinator)

/stream instance A
  reads status.json → claims Stream 2 → writes in_progress
  completes → writes completed

/stream instance B  
  reads status.json → claims Stream 3 → writes in_progress
  completes → writes completed
```

The status file's optimistic concurrency (read → check → write) prevents double-claiming.

---

## Resumability

`/dominion` is fully resumable. If the terminal closes or dominion is interrupted:

1. Status file preserves all progress
2. Running `/dominion` again reads the status file
3. Already-completed streams are skipped
4. In-progress streams are flagged (user decides: wait or take over)
5. Pending streams with met dependencies are spawned

This means `/dominion` is safe to interrupt and restart at any time.

---

## Dry Run Mode

`/dominion --dry-run` shows the full execution plan without spawning anything:

```
Dry run for: docs/plans/2026-03-31-feature-overhaul.md

Phase 1 (sequential):
  → Stream 1: Foundation — 1 headless instance
  
Phase 2 (parallel, after Stream 1):
  → Stream 2: Financial Ops — legion (T:2 → I:2 → D:1)
  → Stream 3: Contract & Sales — legion (T:3 → I:3)
  → Stream 4: Admin UI — solo

Phase 3 (parallel, after Streams 2-4):
  → Stream 5: Integration — legion (T:2 → I:2 → D:2)
  → Stream 6: Polish — solo

Phase 4 (after all):
  → Final Validation — verification + selected validation mode + commit

Commands that would be spawned:
  Phase 1: 1 instance
  Phase 2: 3 concurrent instances
  Phase 3: 2 concurrent instances
  Phase 4: 1 instance

Max concurrent: 3
```

---

## Rules

1. **ALWAYS** show the execution preview and get user confirmation before spawning
2. **ALWAYS** use `--claim {stream_number}` — headless instances cannot prompt for stream selection
3. **ALWAYS** use `--model sonnet` — streams execute plans, they don't need Opus
4. **ALWAYS** use `--allowedTools`, never `--dangerouslySkipPermissions`
5. **ALWAYS** redirect stdin from `/dev/null`
6. **ALWAYS** log output to files
7. **NEVER** modify the status file
8. **NEVER** spawn more instances than the phase allows
9. Wait for process exit, don't poll
10. Max 2 retries per stream
11. Trust process exit for completion
12. Logs persist after cleanup
13. **ALWAYS** load `auto-web-validation` before any web search, package search, or vendor/library research in `/dominion`
14. In `codex` final validation mode, ALWAYS run the Claude `Final Cleanup` stream first, then stop and hand off to Codex `/verify`
15. **ALWAYS** run claim verification (Phase 2.3.5) between a stream exiting and the next phase beginning. Never skip this for "trusted" streams — there are no trusted streams

## Rationalization Prevention

| You're thinking... | Reality |
|---|---|
| "I'll just run /stream myself, it's simpler" | For 3+ streams with parallelism, /dominion saves 40-60% wall clock time. Spawn and walk away. |
| "I should poll the status file while waiting" | Don't. `claude -p` buffers all output until completion — the status file has almost no useful mid-stream updates. Wait for process exit, then read once. |
| "I should skip the preview for simple plans" | Always preview. The user deserves to see what will be spawned before it happens. |
| "I'll skip logging to keep it clean" | Logs are the only debugging path when headless instances fail. Never skip. |
| "This stream is taking too long, I'll kill it" | Trust the process. If it's still running, it's still working. If it exits with failure, you'll be notified. |
| "The stream reported completed; I should trust it" | Streams trust their own self-reports. Sonnet streams systematically miss post-review amendments and ship work that matches the earlier Sub-tasks text instead. EVERY /dominion run must verify claims against the plan's requirements, not against the stream's report. Run Phase 2.3.5 for every stream, every time. |
