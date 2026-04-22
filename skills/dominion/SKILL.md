---
name: dominion
description: "Autonomous plan executor. Reads a multi-stream plan, orchestrates the entire execution by dispatching background Agent-tool instances — one primary agent per eligible stream, plus verification and remediation agents per stream. Monitors status files, spawns next waves when streams complete, and reports final results. User-invocable via /dominion command. The user's alternative to manually running /stream in separate terminals. Triggers: dominion, auto-execute, run all streams, execute plan."
---

# /dominion — Autonomous Plan Orchestrator

`/dominion` is the hands-off execution layer. Where `/stream` executes one stream per session (manual), `/dominion` runs the **entire plan** autonomously by dispatching background Agent-tool instances and cascading phase by phase.

```
/summon   → creates and validates the plan
/dominion → executes the entire plan autonomously
/stream   → executes one stream at a time (manual alternative to /dominion)
```

Use `/dominion` for full autonomy and `/stream` for hands-on control.

## Design Spine — Adversarial Handoffs Against the Plan Contract

The unifying principle holding the pipeline together: **every handoff in dominion is an adversarial review against the original plan contract, not against the prior agent's self-report.** The baton never carries "this is done"; it carries "does this still match the plan's original requirement?"

| Handoff                           | Receiver's job                                                                                                              |
| --------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| Stream agent → verification agent | Read the shipped code. Does it match the plan's stream section? Not: does it match what the stream said it did?             |
| Verification → remediation        | Treat findings as an attack surface. The remedial agent re-derives fixes from plan + skill rules, not from trust.           |
| Remediation → re-gate             | Re-run the gate adversarially against the same original contract. Fixes don't get a pass because remediation made them.    |
| Dominion → Codex `/verify`        | Final independent adversarial pass before anything merges.                                                                  |

No stage inherits trust from the prior stage. Every stage plays Critic against the plan. The mechanisms below — Agent-mode execution, three-input remediation, the briefing packet model, phase gates — all exist to make this concrete.

## Orchestrator-Bears-the-Skills — Briefing Packet Model

Dominion owns exactly one context. N agents don't multiply that work; they inherit it. Agents receive pre-digested briefing packets instead of "load these skills, read these files, then do X" preambles.

### Dominion's one-time work per plan

- Load `CLAUDE.md` + baseline skills into its own context
- Read the plan file end-to-end
- For each stream, precompute a **briefing packet**:
  - Stream section text (verbatim or extracted key passages)
  - **Skill-rule excerpts** — just the enforceable rules from each declared skill, not the full skill doc (~300 tokens per skill vs. ~2–3k for the full doc)
  - Line-range excerpts from reference files the stream will touch (e.g., `sessions.ts:40-95` + `:130-200`, not the whole file)
  - Cross-stream intake items from prior streams' verification

### What goes in each agent prompt

Primary, verification, and remediation agents all receive the relevant briefing packet inline + the specific work for their role. No "read these files" preamble. No "load these skills" step. The context is handed to them.

### Why this is canonical, not an optimization

1. **Adversarial handoff gets sharper.** Dominion, which owns the original plan contract, curates each receiver's briefing against that contract. No stage slips in its own interpretation of "what matters."
2. **Curation needs pipeline history.** Dominion has seen the plan AND earlier stages' findings. It can hand the verifier exactly which skill rules to check against which files. It can hand the remedial agent exactly which rules to apply to which finding. A fresh agent loading skills on its own has no way to make that call.
3. **Skill updates propagate cleanly.** Dominion re-reads skills on each run; every new briefing picks up the latest rules. No agent-side version drift.
4. **Token efficiency compounds** — skill loads are paid once by dominion, not N times across agents.

### Fallback: agent loads its own skills

- Manual `/stream` (no orchestrator exists): stream loads its own skills — unchanged from today
- Rare agent that hits an unknown mid-work: it can still call the Skill tool for a skill dominion didn't anticipate
- 1–2 stream plans where orchestrator overhead isn't worth it

## When to Use

- Plans with 2+ streams where the user wants to walk away
- Plans with parallel-eligible streams (maximum time savings)
- After `/summon` has finalized a plan with the parallelization section

## When NOT to Use

- Single-stream plans (just run `/stream` — dominion overhead isn't worth it)
- When the user wants to review between streams
- Plans involving risky operations that need human judgment between streams (destructive migrations, external API changes, production deployments)

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
   - If no `## Required Skills` section exists, set `baselineSkills` to `[]`
4. Parse the `## Final Validation Mode` section (if present):
   - `Mode: codex` → final validation uses `codex-validation`
   - `Mode: review` → final validation uses classic `review`
   - If absent, default to `review`
5. Write `docs/plans/{slug}.status.json` with all streams set to `pending`, including `baselineSkills` per stream and the selected `finalValidationMode`
6. Final validation handling depends on mode:
   - `review` mode → auto-inject `Final Validation` stream with dependencies on ALL other stream IDs
   - `codex` mode → auto-inject `Final Cleanup` stream with dependencies on ALL other stream IDs. After it completes, dominion stops and hands off to Codex `/verify`.

### Step 3: Pre-compute briefing packets (NEW)

Before spawning anything, pre-digest each stream's briefing packet so primary/verification/remediation agents can be dispatched cheaply:

1. **Load declared skills into dominion's own context.** For each unique skill across all streams, load it once. Extract the enforceable rules (the "must/must not" lines and concrete anti-patterns) into a compact rule-excerpt block per skill.
2. **Read reference files.** For each file a stream will touch, identify the relevant line ranges (interfaces, call sites, schemas the stream must respect). Store these as excerpts.
3. **Build per-stream packets** — one dict per stream containing:
   - `stream_section`: verbatim markdown of that stream's section in the plan
   - `skill_rules`: map of `{skill_name: rule_excerpt}` for each declared skill
   - `reference_excerpts`: map of `{file_path: {line_range: text}}`
   - `cross_stream_intake`: items from upstream streams' verification findings (populated as earlier phases complete)

Cache these in memory for the dominion run. They are reused by primary, verification, and remediation agents — digest once, hand out N times.

### Step 4: Pre-flight validation

Before spawning anything, verify:

1. **Plan has a `## Parallelization` section** — if not, warn the user and offer to run the legion gate now
2. **Plan has 2+ streams** — if single-stream, suggest `/stream` instead
3. **Execution schedule is parseable** — the `### Execution Schedule` from the parallelization section defines the phases
4. **No streams are currently `in_progress`** — if any are, another dominion/stream session may be active. Ask the user before proceeding.

### Step 5: Show execution preview

```
Plan: docs/plans/2026-04-22-feature-overhaul.md
Execution schedule (from parallelization section):

  Phase 1: Stream 1 (Foundation)              — 1 primary + 1 verifier + ≤1 remediator
  Phase 2: Streams 2, 3, 4 (parallel)         — up to 9 concurrent agents
  Phase 3: Streams 5, 6 (parallel)            — up to 6 concurrent agents
  Phase 4: Final Validation / Final Cleanup   — 1 primary + 1 verifier

Per-stream agent cap: 3 (primary + verification + remediation)
  Cap may rise to 4 if dominion dispatches ONE surgical follow-up after remediation fails.
  Past that, dominion handles inline or escalates to user.

Estimated: ~4-5 phases
Proceed? [Y/n]
```

Wait for user confirmation before spawning.

---

## Phase 2: Execute Phases

For each phase in the execution schedule:

### 2.1 Identify Eligible Streams

Read the status file. Find all streams where:
- `status` is `pending`
- All dependencies are `completed` AND have passed remediation (2.4)

Cross-reference with the execution schedule to determine which streams belong to the current phase.

### 2.2 Dispatch Primary Stream Agents (Agent Tool, Background)

For each eligible stream, dispatch a **background Agent-tool agent** with the pre-computed briefing packet. No subprocess, no stdio plumbing, no log-tail parsing.

**Mechanism:** use the Agent tool with `subagent_type: "general-purpose"` and `run_in_background: true`. Dispatch ALL eligible streams in a single message (multiple Agent calls in one response) so they run concurrently.

**Prompt template — Primary Stream Agent:**

```
You are the primary agent for Stream {id} of plan `{plan_path}`.

## Briefing Packet (read first; do not edit anything yet)

### Stream section
{stream_section_verbatim}

### Skill rules (apply these to every decision and every file you touch)
{per_skill_rule_excerpts}

### Reference file excerpts (what you need from other files)
{line_range_excerpts}

### Cross-stream intake (reconciliations from upstream streams)
{cross_stream_intake}

## Coordination protocol

1. Read `{status_path}`. Confirm Stream {id} is still `pending`; abort if another orchestrator claimed it.
2. Edit `{status_path}`: set status to `in_progress` with `claimedAt: <ISO-UTC-now>`.
3. On completion: set status to `completed` with `completedAt: <ISO-UTC-now>` and a `verification` object summarizing gate results.

## File ownership

You own:
{file_list_from_plan}

Do NOT edit files outside this list unless the plan's cross-stream intake explicitly directs you to. If you find a gap in another stream's file that blocks your work, record it in your `deferrals` return field — do not patch it yourself.

## Execution

Implement the stream's sub-tasks using TDD where applicable. If the stream has `Legion: Yes` annotation, execute the waves SEQUENTIALLY within your own turn loop (do not dispatch further sub-agents — you are already a dispatched agent). Test wave first, then impl wave, then dependent wave.

After each cluster of file edits, run `pnpm exec vitest run <touched files>` and the type checker. Fix issues before the next cluster.

## Self-audit before marking completed (mandatory)

Before setting status to `completed`, walk the files you touched and re-check each declared skill's rules against your diff. Fix obvious violations now — you're the cheapest place to catch them.

## Verification gate (must pass before marking completed)

Run these commands directly; capture exit codes. Do NOT pipe `pnpm test` to `head`/`tail`/`grep` — the `| tail -N` pattern hangs if any new test file leaves a handle open (a fresh promise, an unreleased timer, a redis reconnect loop). Use `pnpm exec vitest run` directly.

1. `pnpm check`                           → 0 errors required
2. `pnpm lint`                            → 0 errors required
3. `pnpm exec vitest run {test_glob}`     → all tests pass

Stream-specific smoke checks, if the plan defines any, go last.

## Return format (structured, < 300 words)

SUMMARY
- files: <git diff --stat, abbreviated>
- gates: { check: pass|fail, lint: pass|fail, tests: N/M passed }
- deferrals: [
    { "item": "<what was deferred>",
      "reason": "<why>",
      "owner_suggested": "<which stream/role should handle>" }
  ]
- notes: <anything Phase 2.3.5 verification should know>

No narration beyond the structured fields.
```

**Key details:**
- The Agent tool returns the agent's final message directly to dominion's context — no log parsing, no `while ps -p` polling
- Structured `deferrals` field feeds Phase 2.4 remediation
- The self-audit instruction turns declared skills into Layer 2 enforcement (see Layered Enforcement below)

### 2.3 Await Agent Completion

Each Agent-tool call with `run_in_background: true` notifies dominion automatically when the agent finishes. Do NOT sleep, poll, or chain `ScheduleWakeup` calls waiting for agents — the notification fires on its own.

When a notification arrives:

1. Read the agent's returned SUMMARY
2. Read `docs/plans/{slug}.status.json` to confirm the agent actually wrote `completed`
3. Run `git diff --stat` to see what files actually changed
4. Record the structured deferrals from the agent's return (they feed 2.4)
5. Proceed to 2.3.5 for that stream (do NOT wait for sibling streams to finish)

```
[03:45:12] Phase 2 — 3 primary agents dispatched
[03:52:30] Stream 4 primary: completed (8m, 4 files, 0 deferrals)
[03:52:31] Stream 4 → dispatching verification agent
[03:54:15] Stream 2 primary: completed (11m, 7 files, 2 deferrals)
[03:54:16] Stream 2 → dispatching verification agent
[03:56:40] Stream 3 primary: completed (12m, 3 files, 1 deferral)
[03:56:41] Stream 3 → dispatching verification agent
```

### 2.3.5 Verification Agent (Independent Adversarial Pass — MANDATORY)

**Run this for EVERY stream between primary exit and remediation. Never skip for "trusted" streams — there are no trusted streams.**

Stream self-reports are unreliable. A real run across 7 parallel Sonnet streams produced: every stream reported "completed" with clean gate results, but 6 of 7 had material deviations from the plan's post-review amendments — including Stripe-impossible coupon codes, test files in `src/routes/` that broke SvelteKit, migration number collisions, and silent-fallback patterns the amendments explicitly forbade.

Dispatch a **verification agent** as a fresh Agent-tool call (fresh context, new conversation — it has NOT seen the primary's work). The agent's only job is adversarial verification of shipped code against the plan contract.

**Prompt template — Verification Agent:**

```
You are the verification agent for Stream {id}. Your job is adversarial: assume the stream shipped code that deviates from the plan, and find the deviations.

## Briefing Packet

### Stream section (the authoritative contract)
{stream_section_verbatim}

### Skill rules relevant to this stream's files
{per_skill_rule_excerpts}

### Files the stream claims to own
{file_list_from_plan}

### Primary agent's self-reported summary
{primary_agent_summary}

### Primary agent's self-reported deferrals
{primary_agent_deferrals}

## Your task

DO NOT trust the primary's self-report. Re-derive every requirement from the plan's stream section, then verify against shipped code.

For each requirement:
- Use grep/Read to verify it was implemented (e.g., amendment says "use X not Y" → confirm X present, Y absent)
- Check the primary's deferrals — are they legitimate (genuinely out of scope) or rationalizations (the primary punted on in-scope work)?
- For each declared skill, walk the diff and flag rule violations as findings

Verification grep patterns to use as appropriate:
- "Use X not Y" → grep for X (must be present), grep for Y (must be absent)
- "Delete Z" → grep for Z (zero hits)
- Column NOT NULL/default → read schema file, confirm constraint
- Env var gate → grep for var + presence of boot-time gate
- Function signature → read the function, count parameters
- Hardcoded value to remove → grep src/ and docs/ for it (zero hits)
- Test file location → ls the directories
- Cap/limit → read handler, confirm constant + slice/limit

## Return format (structured, < 500 words)

FINDINGS
- [ { "severity": "blocker" | "correctness" | "quality",
      "skill": "<declared skill name or 'plan-contract'>",
      "file": "<path>",
      "line": <number if applicable>,
      "evidence": "<what's there vs what plan requires>",
      "fix_scope": "<minimal description of fix>" },
    ... ]

DEFERRAL_ASSESSMENT
- { "id_or_item": "<from primary's deferrals>",
    "verdict": "legitimate" | "rationalization" | "partial",
    "reasoning": "<one sentence>" }

SUMMARY
- blockers: N, correctness: N, quality: N
- overall: proceed | remediate | halt
```

### ui-ux-pro-max Artifact Verification

When `ui-ux-pro-max` is in the stream's required skills, the verification agent additionally checks these artifacts. Missing or empty artifacts are **BLOCKING** findings:

| Artifact                                    | Check                                                                                                                     |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `stream-{N}-design-search.md`               | Exists and is > 200 bytes                                                                                                  |
| `stream-{N}-design-decisions.md`            | Exists and contains at least one `###` entry per modified `.svelte` / `.tsx` / `.jsx` / `.css` file                        |
| `stream-{N}-checklist.md`                   | Exists and mentions all 10 Quick Reference categories by name (accessibility, touch, performance, style, layout, typography, animation, forms, navigation, charts) |

If any artifact is missing or empty, add it to FINDINGS as a blocker. The stream either didn't run the design searches or didn't document its design decisions — both indicate ui-ux-pro-max was loaded but not executed.

**Acting on findings:**

| Findings                               | Action                                                                                               |
| -------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| Empty (nothing found)                  | Still dispatch a lightweight remediation agent with Input 3 only (free audit); then proceed         |
| Quality items only                     | Dispatch remediation agent                                                                           |
| Blockers or correctness issues         | Dispatch remediation agent; do NOT proceed to phase transition until remediation re-gate passes      |
| Halt verdict (impossible plan, contract conflict) | **STOP.** Report to user with full evidence. Ask: manual cleanup / skip / cancel                     |

### 2.4 Remediation Agent (Three-Input Adversarial Wave)

Dispatch ONE remediation agent per stream. The agent takes **three inputs** and acts on all three — a mechanical fix-list applier is not enough.

**Input 1 — Primary's deferrals:** What the stream self-reported as out-of-scope or deferred.
**Input 2 — Verification findings:** Skill-tagged, file:line-anchored findings from 2.3.5.
**Input 3 — Free-form standards audit:** The remediator walks the stream's diff against its declared skills and reports any clear rule violations the primary's self-audit AND the verification agent both missed.

Input 3 is the difference between a fix-list applier and a real QA layer. Primary agents miss things; verification agents miss things; a fresh scan from a fresh agent loaded with just the stream's declared skills catches the third-order misses.

**Guardrails on the free-form scan:**

- **File scope:** only files the stream declared ownership of — never reach into other streams' files
- **Skill scope:** only skills the stream declared — `auto-typescript` can fire if declared; `auto-accessibility` only fires if declared
- **Edit scope:** one violation = one minimal edit. No "while I'm here, let me refactor this function." Restraint is a feature.
- **Escalation rule:** if the free scan finds a genuine blocker not already in 2.3.5's list (security hole, data-loss path, broken invariant), surface it as a new finding in the return and STOP. Do not silently patch. Dominion decides.

**Prompt template — Remediation Agent:**

```
You are remediating Stream {id}. Three input lists follow; act on all three.

## Scope (hard constraints)

- Files: {stream_files}  (never edit outside this set)
- Skills: {stream_skills}  (never apply rules outside this set)

## Briefing Packet

### Stream section (the contract)
{stream_section_verbatim}

### Skill rules
{per_skill_rule_excerpts}

## Input 1 — Deferrals from primary

{primary_deferrals_list}

For each: if addressable with a minimal edit inside scope, fix it. If genuinely out-of-scope, repeat it unchanged in your return.

## Input 2 — Verification findings

{findings_list_with_skill_tags}

For each: fix with a minimal edit. If you believe the finding is wrong, argue back in your return — do not silently skip.

## Input 3 — Free-form standards audit

For each file in scope, walk the diff against each declared skill's rules. Report and fix any clear violations the primary and verifier missed. Minimal edits only.

## Re-run the gate after fixing

pnpm check
pnpm lint
pnpm exec vitest run {stream_test_glob}

(Direct commands — do NOT pipe to head/tail/grep if you need to see full output.)

## Return format (structured, < 400 words)

REMEDIATION_RESULT
- fixed: [ { "input": "1|2|3", "file": "<path>", "change": "<one-line>" }, ... ]
- skipped-with-reason: [ { "input": "1|2|3", "item": "<...>", "reason": "<...>" } ]
- new-blockers: [ { "file": "<path>", "issue": "<...>" } ]
- gate-result: { check: pass|fail, lint: pass|fail, tests: N/M passed }
- free-audit-summary: "<one sentence on what the free scan found>"
```

**After remediation returns:**

| Remediation return                                                    | Dominion action                                                              |
| --------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| Gate: pass; no new blockers                                            | Mark stream fully complete. Proceed to phase transition.                     |
| Gate: fail; narrow scope (1–3 files, well-understood)                  | **Dominion handles inline** using its own Read/Edit/Bash. No additional agent. |
| Gate: fail; broad scope where inline burn would be costly              | Dispatch ONE more surgical agent with explicit, narrow prompt. Raises per-stream cap to 4. |
| New blockers surfaced (plan-level or cross-stream)                     | **STOP. Escalate to user** with full evidence. Do not dispatch more agents.  |

Per-stream agent cap is **3 in the normal path** (primary + verification + remediation) and **4 at maximum** (one surgical follow-up after remediation gate fails). Past 4, dominion keeps judgment in the loop — no infinite loops, no silent budget burn.

### 2.5 Phase Transition

When ALL streams in the current phase have:
- `status: "completed"` in the status file
- Verification (2.3.5) findings resolved
- Remediation (2.4) re-gate passed OR dominion inline handled the remainder

1. Read the status file one more time to confirm
2. Propagate any cross-stream intake items (e.g., Stream 1's shape that downstream streams must respect) into the packets for streams in the next phase
3. Check if any new streams are now eligible (deps met)
4. If yes → proceed to next phase (back to 2.1)
5. If all streams complete → proceed to Phase 3

### 2.6 Failure Handling

**Primary agent returns with gate failure AND empty deferrals list (it tried but couldn't pass):**
- The primary is saying "I shipped what I could; the gate still fails and I don't know why"
- Dispatch verification and remediation as normal — they may catch what primary missed

**Primary agent returns with a crash or abort:**
- Read the partial diff on disk; read status.json
- If status is still `in_progress`, reset to `pending` and re-dispatch the primary ONCE
- Max 2 primary retries per stream before escalating to user

**Remediation agent returns with new blockers:**
- These indicate plan-level problems (contract conflict, impossible requirement, cross-stream gap)
- STOP dominion. Escalate to user with full findings.

**Multiple streams fail in same phase:**
- If 2+ streams in the same phase fail verification AND remediation, pause and ask the user
- Could indicate a systemic issue (broken dependency, bad plan)

---

## Layered Skill Enforcement (Four Depths)

Declared skills must shape each stream's output, not just get loaded and forgotten. Four enforcement depths, each catching what the previous missed:

1. **Primary work — skills loaded up front** (handled by the primary agent's briefing packet). Shapes decisions during implementation.
2. **Self-audit pass before marking completed** (part of the primary agent's contract, see template). Catches easy-to-see violations the primary introduced while focused on feature work.
3. **Verification agent (2.3.5)** — adversarial read against plan + skills. Catches what self-audit missed.
4. **Remediation agent (2.4) — three inputs** (deferrals + findings + free audit). Catches what verification missed AND fixes what was flagged.

Skills never sit inert. Each layer has a specific job: primary shapes design, self-audit catches rough edges, verification catches cross-stream gaps, remediation fixes what escaped.

---

## Phase 3: Final Validation

**Phase 3 still runs even if Phase 2 finished with zero remediation work.** 2.3.5 checks stream-level correctness against plan requirements; 2.4 fixes what escaped; Phase 3 checks cross-stream integration — the 9-dimension review, full build, commit, push, and cleanup. They are complementary.

When all non-final streams are `completed` and verified:

**If final validation mode is `review`:**

1. Dispatch a primary agent for the Final Validation stream (same Agent-tool mechanism as Phase 2)
2. Dispatch a verification agent after it completes
3. Optionally dispatch a remediation agent if verification flags anything
4. The Final Validation stream handles: full verification, `/review` pass, git commit/push, plan+status cleanup

**If final validation mode is `codex`:**

1. Dispatch a primary agent for the Final Cleanup stream
2. Dispatch a verification agent after it completes
3. The Final Cleanup stream handles: full verification, broad cleanup, obvious improvement passes — but does NOT run `codex-validation`, commit, push, or delete plan/status artifacts
4. After Final Cleanup completes, stop orchestration and hand off to Codex `/verify`

Use language like:

```
Implementation and Claude cleanup streams complete.

Final validation mode is `codex`, so dominion will not spawn a Claude
Codex-validation stream.

Next step:
  Open Codex in the same repo and run `/verify`

Artifacts preserved for Codex:
  - docs/plans/{slug}.md
  - docs/plans/{slug}.status.json
  - docs/plans/.dominion-logs/
```

---

## Phase 4: Report Results

### Success

```
/dominion complete ✓

Plan: docs/plans/2026-04-22-feature-overhaul.md
Streams: 7/7 completed
Agents dispatched: 18 (avg 2.6/stream)
Duration: 34 minutes (vs ~2h sequential estimate)
Commit: abc1234
Branch: main

Phase breakdown:
  Phase 1 (Stream 1):          6 min   (primary + verification, no remediation needed)
  Phase 2 (Streams 2,3,4):    12 min   (parallel; Stream 2 needed remediation)
  Phase 3 (Streams 5,6):       9 min   (parallel; both clean)
  Phase 4 (Final Validation):  7 min

Plan and status files cleaned up by Final Validation.
Briefing packets and logs: docs/plans/.dominion-logs/
```

### Codex Handoff

```
/dominion implementation and cleanup phases complete ✓

Plan: docs/plans/2026-04-22-feature-overhaul.md
Implementation streams: 6/6 completed
Final Cleanup: completed
Final validation mode: codex

No Claude Codex-validation stream was spawned.

Next step:
  Open Codex and run `/verify`

Preserved for Codex:
  - docs/plans/2026-04-22-feature-overhaul.md
  - docs/plans/2026-04-22-feature-overhaul.status.json
  - docs/plans/.dominion-logs/
```

### Failure

```
/dominion stopped — Stream 3 remediation surfaced new blockers

Completed: Streams 1, 2, 4 (3/7)
Failed: Stream 3 (see findings below)
Blocked: Streams 5, 6, Final Validation

Findings:
  - src/lib/server/auth/sessions.ts:142 — primary shipped plaintext
    session tokens; amendment required sha256 hashing
  - migrations/0042: collides with existing migration number 0042

Status file preserved: docs/plans/2026-04-22-feature-overhaul.status.json
Briefings + agent returns: docs/plans/.dominion-logs/

You can:
  1. Fix Stream 3 manually, then run /dominion to resume
  2. Run /stream to take over Stream 3 interactively
```

---

## Artifact Management

### Artifacts directory

```
docs/plans/.dominion-logs/
  briefing-stream-1.json          # The briefing packet dominion built
  briefing-stream-2.json
  ...
  return-stream-1-primary.md      # What the primary agent returned
  return-stream-1-verify.md       # What the verification agent returned
  return-stream-1-remediate.md    # What the remediation agent returned (if dispatched)
  stream-{N}-design-search.md     # (ui-ux-pro-max artifact, if applicable)
  stream-{N}-design-decisions.md  # (ui-ux-pro-max artifact, if applicable)
  stream-{N}-checklist.md         # (ui-ux-pro-max artifact, if applicable)
```

Create this directory at dominion start. Dominion writes briefing packets and collects agent returns here. These are the audit trail — they persist until the user deletes them manually.

### Retry handling

If a primary is retried, suffix the previous return file:
```
return-stream-3-primary.md → return-stream-3-primary.attempt-1.md
```

### Cleanup

Artifacts are NOT deleted by Final Validation (unlike the plan and status files). Intentional — they're the audit trail.

---

## Status File as Coordination Layer

`/dominion` and all dispatched agents share the status file as their coordination mechanism:

```
/dominion (orchestrator)
  reads status.json to confirm agent completion
  reads/propagates cross-stream intake
  writes: nothing (read-only coordinator)

Primary agent (background)
  reads status.json → confirms claim → writes in_progress
  completes → writes completed

Verification agent (background)
  reads status.json (read-only) — doesn't modify status
  returns findings to dominion

Remediation agent (background)
  reads status.json (read-only)
  may update the stream's `verification` sub-object if it patches issues
```

The status file's optimistic concurrency (read → check → write) prevents double-claiming.

---

## Resumability

`/dominion` is fully resumable. If the session closes or dominion is interrupted:

1. Status file preserves all progress
2. Running `/dominion` again reads the status file
3. Already-completed streams are skipped
4. `in_progress` streams are flagged (user decides: wait or take over)
5. Pending streams with met dependencies are re-dispatched
6. Dominion re-builds briefing packets on resume — they're not persisted as source-of-truth, just as audit artifacts

Safe to interrupt and restart at any time.

---

## Dry Run Mode

`/dominion --dry-run` shows the full execution plan without dispatching anything:

```
Dry run for: docs/plans/2026-04-22-feature-overhaul.md

Phase 1 (sequential):
  → Stream 1: Foundation — 1 primary, 1 verifier

Phase 2 (parallel, after Stream 1):
  → Stream 2: Financial Ops — legion (T:2 → I:2 → D:1) — run sequentially within primary
  → Stream 3: Contract & Sales — legion (T:3 → I:3) — run sequentially within primary
  → Stream 4: Admin UI — solo

Phase 3 (parallel, after Streams 2-4):
  → Stream 5: Integration — legion (T:2 → I:2 → D:2) — run sequentially within primary
  → Stream 6: Polish — solo

Phase 4 (after all):
  → Final Validation — verification + selected validation mode + commit

Normal-path agent count: 14 (2 per stream × 7)
Worst-plausible-case: 28 (4 per stream × 7)
Max concurrent: 3 streams × 3 agents = 9 agents simultaneously
```

---

## Fallback: Headless Subprocess Mode (Deprecated, On-Demand Only)

The historic `claude -p` headless mechanism remains available for users who explicitly request it or in runtimes where the Agent tool is unavailable. Do NOT use by default.

```bash
cd {project_root} && claude -p "/stream {plan_file} --claim {stream_number}" \
  --model sonnet \
  --allowedTools "Bash,Read,Write,Edit,Glob,Grep,Skill,Agent" \
  < /dev/null > docs/plans/.dominion-logs/stream-{id}.log 2>&1
```

**Known issue:** headless streams that run `pnpm test <files> | tail -N` as their verification gate have been observed to hang indefinitely when a new test file in the stream's own diff leaks a timer/promise/connection. `tail` buffers until EOF; if node never exits, the pipeline never unblocks. This was the original motivation for moving to Agent-tool bg-mode.

If forced to use headless, ensure the spawned `/stream` uses `pnpm exec vitest run` rather than `pnpm test | tail -N`.

---

## Rules

1. **ALWAYS** show the execution preview and get user confirmation before dispatching
2. **ALWAYS** pre-compute briefing packets before dispatching any agent — no "read these files, load these skills" preambles in agent prompts
3. **ALWAYS** dispatch via Agent tool with `run_in_background: true` — never use headless `claude -p` by default
4. **ALWAYS** dispatch all eligible primary agents for a phase in a single message (parallel execution)
5. **ALWAYS** run verification (2.3.5) for EVERY stream — no trusted streams
6. **ALWAYS** run remediation (2.4) if verification finds anything, even quality-only
7. **ALWAYS** run a lightweight remediation with Input 3 (free audit) even when verification finds nothing — it's Layer 4 insurance
8. **NEVER** modify the status file from dominion's own context; only read
9. **NEVER** dispatch more than 3 agents per stream in the normal path; 4 maximum after a failed remediation gate; past that, handle inline or escalate
10. **NEVER** pipe `pnpm test` output through `| tail` / `| head` / `| grep` in agent prompts — the pipe hangs on leaky teardown
11. **ALWAYS** load `auto-web-validation` into dominion's own context before any web research or vendor/library lookup
12. In `codex` final validation mode, run the Claude `Final Cleanup` stream first (primary + verify + optional remediation), then hand off to Codex `/verify`
13. Trust the Agent tool's completion notification; do not sleep/poll for agent completion
14. Briefing packet artifacts persist after cleanup; they're the audit trail

## Rationalization Prevention

| You're thinking...                                                                 | Reality                                                                                                                                                                                                       |
| ---------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| "I'll just spawn `claude -p` like before, Agent tool is extra work"                | Agent tool is strictly simpler: no stdio plumbing, no log parsing, no `while ps -p` polling, no `| tail` hangs. The return value lands directly in your context. The only reason to use `claude -p` is fallback. |
| "The primary agent's summary looks clean, I can skip verification"                 | Self-reports are unreliable. 6/7 Sonnet streams shipped material deviations while reporting "completed." Always run 2.3.5.                                                                                    |
| "Verification found nothing, I can skip remediation"                               | Run the lightweight Input-3-only remediation anyway. It's cheap and catches what both the primary's self-audit and the verifier missed. Three layers, not two.                                                |
| "Remediation failed — I'll dispatch another remediation agent"                     | Don't. At that point dominion has strictly more information than a fresh remediator. Handle inline, dispatch ONE surgical follow-up, or escalate. Never dispatch a second generic remediation.                |
| "I should make each agent load its own skills for clean context separation"        | That's the old model. Agent context is fresh but the SKILLS are dominion's responsibility — precomputed, handed out as rule excerpts. Makes curation against the plan contract possible.                      |
| "I'll poll the status file to track agent progress"                                | Don't. Agent-tool `run_in_background: true` notifies automatically. Polling wastes cycles and breaks cache efficiency.                                                                                        |
| "This stream is taking too long, I'll kill the agent"                              | Trust the agent. If it's still running, it's still working. Agent tool notifies on completion — you'll hear when it's done.                                                                                   |
| "I'll let the primary self-certify — verification is just overhead"                | Verification catches the things the primary can't see (it's too close to its own work). The independent fresh-context read is the point.                                                                     |
| "The primary said it deferred X because it's out of scope — I should trust that"   | Run Input 1 through the remediation agent. The deferral may be legitimate; it may also be the primary punting on in-scope work. The remediator assesses.                                                      |
