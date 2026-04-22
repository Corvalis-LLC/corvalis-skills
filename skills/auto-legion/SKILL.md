---
name: auto-legion
description: "Legion execution discipline: orchestrator-driven parallel agent waves within a single stream. Decomposes stream tasks into TDD-phased waves, crafts focused minimal-context prompts, dispatches background agents, verifies between waves, and assembles results. Loaded by /stream when a stream has legion annotation. Triggers: legion, wave, parallel agents, orchestrator, dispatch."
---

# Legion — Orchestrator-Driven Parallel Execution

You are the **orchestrator**. You do not implement — you read, decompose, dispatch, and verify. Legion agents are your hands. Your job is to give each agent the smallest possible context window to produce correct code, then verify their work between waves.

## Mode-Specific Interpretation

Legion annotations in plans (`**Legion:** Yes — T:3 → I:3 → D:2`) are **mode-agnostic**. The executor decides how to interpret them based on how the plan is being run. Plan authors write the annotation once; two interpretations exist:

| Plan declares                   | Manual `/stream` executes as...                                                                                              | Dispatched `/dominion` primary agent executes as...                                                                              |
| ------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `Legion: Yes — T:4 → I:4 → D:1` | Spawn 4 test sub-agents in parallel, then 4 impl sub-agents, then 1 integrator. User-driven session with background fan-out. | **Sequential phases inside the primary agent's own turn loop** — finish tests first, then impls, then integration. No nested Agent dispatch. |
| `Legion: No`                    | Single-threaded work in the user's session.                                                                                  | Single-threaded work in the primary agent's turn loop.                                                                           |

Why no nesting under dominion: dominion-in-Agent-mode IS plan-level legion already. The orchestrator dispatches a legion of background Agent-tool primaries (one per stream), with phase gates as wave boundaries. A dispatched primary that then tries to dispatch more legion sub-agents is nesting legions for no gain — the waves are already parallelized at the outer layer.

Everything below assumes **manual `/stream` mode** (the orchestrator is the user's main session). For the dominion interpretation, treat the "dispatch" steps as "run these phases sequentially in your own context, verifying between phases, without calling the Agent tool."


## Why Legion Works

Benchmark data (single-file problems, scales better with more files):

| Metric | Solo Agent | Legion | Improvement |
|--------|-----------|--------|-------------|
| Wall clock | baseline | -36% | Parallel dispatch |
| Token usage | baseline | -18% | Focused prompts |
| Tool calls | baseline | -25% | No redundant reads |
| Correctness | 100% | 100% | Orchestrator pre-analysis |

The savings **compound with stream size**. A 12-file stream where a solo agent holds all 12 files in context becomes 4 waves of 3 agents each holding 2-3 files.

## Core Principle: Orchestrator Intelligence

The orchestrator's value is **pre-analysis**. Before dispatching any agent:

1. **Read all relevant files** in the stream's scope
2. **Identify the patterns** needed (data structures, interfaces, dependencies)
3. **Craft surgical prompts** that give each agent ONLY what it needs
4. **Include the implementation pattern** — don't make agents figure out the approach

An agent prompt should contain:
- The exact file path(s) to edit/create
- The interfaces/types the code must satisfy (paste them in, don't make the agent read them)
- The specific pattern to implement (not "fix the bug" — "use a per-key promise chain as an async mutex")
- The verification command to run after

An agent prompt should NOT contain:
- Full test files (extract the requirements instead)
- Other streams' context
- Planning history or rationale
- Files the agent won't touch

## Wave Structure: TDD Phases

Every stream decomposes into waves following the TDD cycle. The orchestrator determines what goes in each wave based on the stream's task list.

### Wave Types

**Wave T (Tests)** — Write failing tests
- Agents write test files in parallel (each test file is independent)
- Prompt includes: interfaces to test against, expected behaviors, file path to create
- Orchestrator verifies: tests exist, tests FAIL (red phase), no type errors

**Wave I (Implementation)** — Make tests pass
- Agents implement production code in parallel (each module is independent)
- Prompt includes: the failing test expectations, interfaces to satisfy, implementation pattern
- Orchestrator verifies: tests PASS (green phase), type check clean

**Wave D (Dependents)** — Build on stable APIs
- Agents implement code that depends on Wave I output (frontend, consumers, integrations)
- Prompt includes: the now-stable API signatures, component requirements
- Orchestrator verifies: full verification gate (types + tests + build)

**Wave R (Refactor)** — Optional cleanup
- Only if the orchestrator spots duplication or quality issues across agent outputs
- Can be done by orchestrator directly if changes are small

### Wave Ordering Rules

```
T → I → D → R (within a stream)
     ↑
     Each wave completes and verifies before the next begins
```

Not every stream needs all wave types:
- API-only stream: T → I → R
- Full-stack stream: T → I → D → R  
- Frontend-only stream (API already exists): T → D → R
- Bug fix stream: T → I (often just 1 agent per wave)

## Decomposition Algorithm

When the orchestrator claims a stream with legion enabled:

### Step 1: Read and Categorize Tasks

Read the stream's tasks from the plan. Categorize each task:

| Category | Examples | Wave |
|----------|----------|------|
| Test | "Write tests for capacity service" | T |
| Service/Logic | "Implement capacity check with mutex" | I |
| API Route | "Create POST /api/registrations endpoint" | I |
| Migration | "Add capacity column to products table" | I (early) |
| Component | "Build registration form component" | D |
| Page | "Create admin dashboard page" | D |
| Integration | "Wire form to API endpoint" | D |

### Step 2: Group Into Waves

Within each wave type, group tasks that can run in parallel:

**Parallelizable** (no shared file mutations):
- Different test files
- Different service modules
- Different API route files
- Different Svelte components

**Must be sequential** (same file or ordering dependency):
- Migration files (must run in order)
- Barrel export files (additive but potential conflict)
- Shared utility files being created

### Step 3: Size the Waves

Each wave should have **2-5 agents**. More than 5 increases orchestrator verification burden. Fewer than 2 isn't worth the legion overhead.

If a wave has only 1 task, the orchestrator can execute it directly instead of dispatching an agent.

If a wave has more than 5 parallelizable tasks, split into sub-waves (T1, T2) with verification between them.

## Dispatching Agents

### Agent Tool Configuration

```
Agent(
  description: "Legion W{wave}-A{n}: {3-5 word task}",
  prompt: <focused prompt>,
  run_in_background: true  // ALWAYS background for legion agents
)
```

**Always dispatch all agents in a wave simultaneously** — put all Agent tool calls in a single message.

### Prompt Template

Every legion agent prompt follows this structure:

```
You are implementing a focused task. Do not read files beyond what's listed here.

## Your Task
{one sentence: what to create/modify}

## Files to Edit
{exact paths}

## Context You Need
{paste relevant interfaces, types, schemas — NOT file paths to read}

## Implementation Pattern
{specific pattern to follow, with code sketch if complex}

## Constraints
- Do not modify any file not listed above
- {stream-specific constraints from plan's "Files owned"}

## Verify
{exact test/check command to run after}

Report: pass/fail count and any errors.
```

### What to Paste vs What to Reference

**Paste into the prompt** (agent doesn't waste a tool call reading it):
- Interface definitions the agent's code must satisfy
- Type definitions the agent must import
- Zod schemas the agent must match
- Small config snippets (< 30 lines)

**Let the agent read** (too large to paste, agent needs full context):
- Existing implementation files being modified (> 50 lines)
- Test files the agent is extending

## Verification Between Waves

After ALL agents in a wave complete:

### 1. Collect Results
Read each agent's output. Check for:
- Did all agents report success?
- Any agents that failed or reported errors?

### 2. Run Project-Wide Verification
```bash
npx tsc --noEmit          # Type check
npx vitest run <paths>    # Tests for this stream's files
```

### 3. Fix Conflicts
If two agents wrote conflicting code (rare with proper decomposition):
- Read both files
- Determine which is correct based on test results
- Fix the conflict directly (orchestrator handles this, no agent needed)

### 4. Proceed or Retry
- All green → advance to next wave
- Failures in specific agents → re-dispatch ONLY those agents with error context
- Systemic failure (wrong pattern) → stop, reassess decomposition, potentially fall back to solo execution

**Max retries per agent: 2.** If an agent fails twice, the orchestrator takes over that task directly.

## Status Tracking

Legion execution adds a `legion` field to the stream's status entry:

```json
{
  "legion": {
    "enabled": true,
    "currentWave": "I",
    "waves": {
      "T": {
        "status": "completed",
        "agents": [
          { "task": "Write capacity service tests", "files": ["src/lib/server/services/capacity.test.ts"], "status": "completed" },
          { "task": "Write registration API tests", "files": ["src/routes/api/registrations/+server.test.ts"], "status": "completed" }
        ],
        "verification": { "typeCheck": true, "tests": "2 failing (expected — red phase)", "timestamp": "..." }
      },
      "I": {
        "status": "in_progress",
        "agents": [
          { "task": "Implement capacity service with mutex", "files": ["src/lib/server/services/capacity.ts"], "status": "completed" },
          { "task": "Implement registration endpoint", "files": ["src/routes/api/registrations/+server.ts"], "status": "in_progress" }
        ],
        "verification": null
      }
    }
  }
}
```

## When NOT to Use Legion

Legion adds orchestration overhead. Skip it when:

- Stream has ≤ 2 tasks (solo is faster)
- Stream is purely sequential (each task depends on the previous)
- Stream is a bug fix with 1-2 files
- Stream modifies a single complex file extensively

The `/summon` legion gate annotates streams with `legion: true/false` based on this analysis.

## Fallback to Solo

If legion execution hits problems:
1. Two waves fail verification consecutively
2. Agent conflicts can't be resolved by orchestrator
3. The domain requires deep context that can't be summarized in a prompt

The orchestrator **falls back to solo execution** — loads all context and implements remaining tasks directly. This is not a failure; it's the right call when the problem resists decomposition. Update the status file: `"legion": { "enabled": false, "fallbackReason": "..." }`.

## Rationalization Prevention

| You're thinking... | Reality |
|---|---|
| "I'll just do it myself, it's faster" | For > 3 tasks, legion is measurably faster. Dispatch. |
| "The agent needs the full context" | It doesn't. Paste the interfaces, describe the pattern. |
| "I should read the test files for them" | Extract requirements into the prompt. Don't paste full test files. |
| "One more wave won't hurt" | If the task is 1 file, do it yourself. Don't dispatch a wave of 1. |
| "I'll skip verification between waves" | Verification catches conflicts early. Never skip. Always verify between waves. |
