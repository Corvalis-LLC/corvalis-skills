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
6. Auto-inject Final Validation stream with dependencies on ALL other stream IDs

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

Total: 4 phases, max 3 concurrent instances
Estimated: ~4 stream durations (vs ~7 sequential)

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
cd {project_root} && claude -p "/stream {plan_file}" \
  --allowedTools "Bash,Read,Write,Edit,Glob,Grep,Skill,Agent" \
  < /dev/null > {log_file} 2>&1 &
```

Key details:
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

### 2.4 Phase Transition

When ALL streams in the current phase are `completed`:

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

When all non-final streams are `completed`:

1. Spawn one headless instance for the Final Validation stream
2. Monitor until complete
3. The Final Validation stream handles: full verification, selected validation mode (`codex-validation` or `review`), git commit/push, plan+status cleanup

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
cd {project_root} && claude -p "/stream {plan_file}" \
  --allowedTools "Bash,Read,Write,Edit,Glob,Grep,Skill,Agent" \
  < /dev/null > docs/plans/.dominion-logs/stream-{id}.log 2>&1
```

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
2. **ALWAYS** use `--allowedTools`, never `--dangerouslySkipPermissions`
3. **ALWAYS** redirect stdin from `/dev/null`
4. **ALWAYS** log output to files
5. **NEVER** modify the status file
6. **NEVER** spawn more instances than the phase allows
7. Wait for process exit, don't poll
8. Max 2 retries per stream
9. Trust process exit for completion
10. Logs persist after cleanup

## Rationalization Prevention

| You're thinking... | Reality |
|---|---|
| "I'll just run /stream myself, it's simpler" | For 3+ streams with parallelism, /dominion saves 40-60% wall clock time. Spawn and walk away. |
| "I should poll the status file while waiting" | Don't. `claude -p` buffers all output until completion — the status file has almost no useful mid-stream updates. Wait for process exit, then read once. |
| "I should skip the preview for simple plans" | Always preview. The user deserves to see what will be spawned before it happens. |
| "I'll skip logging to keep it clean" | Logs are the only debugging path when headless instances fail. Never skip. |
| "This stream is taking too long, I'll kill it" | Trust the process. If it's still running, it's still working. If it exits with failure, you'll be notified. |
