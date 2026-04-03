---
name: triumvirate
description: "Plan review system with three adversarial subagents (Advocate, Analyst, Critic) that debate a proposed plan from different perspectives. Use when reviewing implementation plans, architecture decisions, or significant changes. Triggers: triumvirate, debate plan, review plan, argue plan, adversarial review, devil's advocate, plan critique, three perspectives, plan debate."
---

# Triumvirate Plan Review

<command-name>triumvirate</command-name>

## Overview

Three adversarial subagents debate a proposed plan, then the plan is amended with the strongest arguments. The user decides how to proceed.

## The Three Reviewers

| Reviewer | Role | Perspective |
|----------|------|-------------|
| **Advocate** (+) | Argue FOR the plan | Strengths, opportunities, enhancements, counter-arguments to criticism |
| **Analyst** (=) | Objective tradeoff analysis | Assumptions, alternatives comparison, metrics, risk/reward |
| **Critic** (-) | Find weaknesses constructively | Risks, edge cases, tech debt, failure modes — every criticism gets a mitigation |

## Workflow

```
User Plan → [Advocate + Analyst + Critic in parallel] → Synthesis → Amended Plan → User Decision
```

## Implementation

### Step 1: Parse the Plan

Extract the plan from current conversation context:
- Search for plan files, implementation proposals, architecture decisions
- If no plan found, review the most recent significant discussion topic
- Do NOT prompt the user — always work with current context

### Step 2: Launch Subagents in Parallel

Launch all three with the Task tool simultaneously:

```
Task(subagent_type='general-purpose', prompt=advocate_prompt)
Task(subagent_type='general-purpose', prompt=analyst_prompt)
Task(subagent_type='general-purpose', prompt=critic_prompt)
```

For the full prompt for each reviewer, see:
- **[Advocate Prompt](prompts/advocate.md)**
- **[Analyst Prompt](prompts/analyst.md)**
- **[Critic Prompt](prompts/critic.md)**

Replace `{plan_text}` in each prompt with the actual plan content.

### Step 3: Synthesize and Amend

After all three return, synthesize arguments and amend the plan. For templates, see **[templates.md](references/templates.md)**.

Present findings directly — do NOT use AskUserQuestion. Wait for user response.

### Step 4: User Decision

Options presented after review:
- **Re-debate**: `/triumvirate` again on the amended plan
- **Approve**: Begin implementation
- **Modify**: User edits manually
- **Reject**: Start over

## Research Requirements

Each subagent MUST research before forming arguments:

| Type | Minimum | Tools |
|------|---------|-------|
| Codebase research | 3 searches | Glob, Grep, Read |
| Web research | 2 searches minimum, always required | WebSearch, WebFetch |
| File reads | 2 files | Read |

Web research is not optional. Each reviewer must back its arguments with research into standard industry practices for similar systems or features. Prefer strong engineering sources when available:
- large-scale engineering blogs or documentation from companies such as Google, Meta, Netflix, Stripe, Shopify, Airbnb, Uber, Vercel, Cloudflare, GitHub, and similar mature teams
- official framework or platform documentation
- postmortems or architecture writeups for comparable products

If exact FAANG-style analogues do not exist, use the closest high-quality engineering sources and say so explicitly.

## Quality Standards

- **Advocate**: Specific evidence, not optimism. Acknowledge limitations while arguing strengths.
- **Analyst**: Genuinely neutral. Quantifiable comparisons. All major assumptions identified.
- **Critic**: Constructive, not destructive. Mitigations for every criticism. Prioritized by severity.
- **All reviewers**: Arguments must cite both codebase evidence and web-researched industry practice for similar features/applications.

## Integration with Plan Mode

1. User enters plan mode → agent creates plan
2. `/triumvirate` → three subagents debate
3. Plan amended → user approves or iterates
4. Exit plan mode with final plan

## Reference Files

- **[prompts/advocate.md](prompts/advocate.md)** — Full Advocate subagent prompt
- **[prompts/analyst.md](prompts/analyst.md)** — Full Analyst subagent prompt
- **[prompts/critic.md](prompts/critic.md)** — Full Critic subagent prompt
- **[references/templates.md](references/templates.md)** — Synthesis, amendment, and presentation templates
- **[references/example-output.md](references/example-output.md)** — Complete example of triumvirate output
