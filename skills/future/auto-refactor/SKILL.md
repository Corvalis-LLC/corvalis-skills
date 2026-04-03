---
name: auto-refactor
description: "Structural refactoring discipline: file splitting, code extraction, concern separation, and the specific monolith patterns Claude produces during autonomous execution. Catches oversized files, duplicated logic across call sites, inline switch/map patterns that should be data-driven, and mixed concerns in single files. Use when reviewing stream output, splitting large components, extracting shared logic, reorganizing file structure, or running the pre-review refactoring stream. Triggers: refactor, split, extract, reorganize, abstract, file too large, monolith, mixed concerns, dedup, reuse."
---

# Structural Refactoring — What Autonomous Streams Get Wrong

Streams optimize for "get it working." They produce correct code that's structurally lazy — everything in one file, logic inlined where it's used, identical patterns copy-pasted across streams. This skill catches those structural problems and fixes them before review.

**This is not about code quality** (auto-coding handles that). This is about **where code lives and how it's organized.**

---

## When to Trigger

This skill activates as a dedicated refactoring stream **after all implementation streams complete but before Final Validation**. It operates on the full codebase with all dependencies resolved.

It also auto-triggers when you encounter files exceeding the split thresholds during any code review or modification.

---

## Split Thresholds

| File Type | Review At | Must Split At | Split By |
|-----------|-----------|---------------|----------|
| Component (.svelte, .tsx, .vue) | 200 lines | 400 lines | UI section, form step, feature panel |
| Module (.ts, .py, .go, .rs) | 250 lines | 500 lines | Domain concern, public API surface |
| Route handler / controller | 150 lines | 300 lines | HTTP method, resource, middleware |
| Entry point (index.ts, main.go) | 100 lines | 200 lines | Re-exports only — move logic to domain modules |
| Test file | 400 lines | 800 lines | Test suite per feature or fixture group |

**"Must Split" is non-negotiable.** A 1900-line Svelte component is never the right answer — no matter how "related" the sections feel. This applies to the orchestrator file too — if the parent component still exceeds the threshold after extraction, you haven't finished.

---

## Extraction Patterns

### 1. Component Decomposition (UI Files)

Large components are almost always multiple components glued together. Split by **user-visible section**, not by code structure.

| Signal | Extract To |
|--------|-----------|
| Template section with its own heading/title | Child component |
| Form step in a multi-step flow | `FormStep{Name}.svelte` / `{Name}Step.tsx` |
| Repeated input group pattern (3+ fields) | Shared field group component |
| Conditional block with >30 lines of markup | Dedicated component with props |
| Mobile vs desktop duplicate markup | Responsive component or slot pattern |
| Modal/overlay content | Separate modal component |

**How to split a component:**
1. Identify self-contained sections (look for HTML comments, heading elements, conditional blocks)
2. Determine which reactive state each section reads/writes
3. Extract section → new component, pass state as props or use bind
4. Keep orchestration logic (form submission, navigation) in the parent
5. Move section-specific derived state and handlers into the child
6. **Check the orchestrator size after extraction** — if it still exceeds the threshold, extract further: fee/pricing logic → `pricing.ts`, validation → `validators.ts`, API calls → service functions, `$effect` blocks → named helper functions. "It uses reactive state" is not a reason to keep 500 lines of logic in a component script.

```
BEFORE: registration-page.svelte (1900 lines)
  - Parent info form (lines 1-200 of template)
  - Guardian form (lines 200-280)
  - Children forms (lines 280-500)
  - Payment section (lines 500-650)
  - Consent + signature (lines 650-750)
  - Success/receipt view (lines 750-850)
  - Mobile sticky bar (lines 850-900)

AFTER:
  registration-page.svelte (200 lines — orchestration + layout)
  ParentInfoSection.svelte
  GuardianSection.svelte  
  ChildrenFormSection.svelte
  PaymentSection.svelte
  ConsentSection.svelte
  RegistrationReceipt.svelte
  MobileStickyBar.svelte
```

### 2. Module Decomposition (Logic Files)

| Signal | Extract To |
|--------|-----------|
| Entry file with function implementations | Domain modules, entry re-exports only |
| Multiple exported functions sharing no state | Separate modules by domain |
| Helper functions used by 1 exported function | Inline or co-locate with that function |
| Validation logic mixed with business logic | `validation.ts` / `validators/` |
| Data transformation pipelines | `transforms.ts` per domain |

**Entry point files (index.ts, main.go, etc.) should contain only:**
- Initialization (app setup, DB connections)
- Registration (route mounting, handler binding)
- Re-exports

All logic belongs in domain-specific modules.

```
BEFORE: functions/src/index.ts (2100 lines)
  - 15 scheduled functions defined inline
  - 5 HTTP handlers with full implementations
  - 3 Firestore triggers
  - Inline helper functions scattered throughout

AFTER:
  functions/src/index.ts (80 lines — re-exports only)
  functions/src/scheduled/reminders.ts
  functions/src/scheduled/lead-timeouts.ts
  functions/src/scheduled/lease-expiration.ts
  functions/src/handlers/sms.ts
  functions/src/handlers/notifications.ts
  functions/src/triggers/document-created.ts
```

### 3. Logic Extraction (Cross-Cutting)

| Signal | Action |
|--------|--------|
| Same validation logic in 3+ places | Extract to shared validator |
| Switch/match mapping values → results | Replace with lookup table (Map, Record, dict) |
| Same API call pattern in 3+ places | Extract to typed API client method |
| Same error handling wrapper in 3+ places | Extract to utility or middleware |
| Inline formatting (currency, dates, phones) | Extract to shared formatter |
| Same computed/derived pattern in 3+ components | Extract to shared hook/store/derived |

**The Rule of Three applies strictly.** Two instances may be coincidental. Three is a pattern. Don't extract at two.

---

## Switch/Match → Data-Driven Refactoring

This is the single most common miss. Streams produce switch cases and if/else chains where data structures are the right answer.

**Before:**
```typescript
function getStatusLabel(status: string): string {
  switch (status) {
    case 'pending': return 'Awaiting Review';
    case 'approved': return 'Approved';
    case 'rejected': return 'Declined';
    case 'expired': return 'Expired';
    default: return 'Unknown';
  }
}

function getStatusColor(status: string): string {
  switch (status) {
    case 'pending': return 'yellow';
    case 'approved': return 'green';
    case 'rejected': return 'red';
    case 'expired': return 'gray';
    default: return 'gray';
  }
}
```

**After:**
```typescript
const STATUS_CONFIG = {
  pending:  { label: 'Awaiting Review', color: 'yellow' },
  approved: { label: 'Approved',        color: 'green' },
  rejected: { label: 'Declined',        color: 'red' },
  expired:  { label: 'Expired',         color: 'gray' },
} as const satisfies Record<string, { label: string; color: string }>;

type Status = keyof typeof STATUS_CONFIG;

function getStatusLabel(status: Status) { return STATUS_CONFIG[status]?.label ?? 'Unknown'; }
function getStatusColor(status: Status) { return STATUS_CONFIG[status]?.color ?? 'gray'; }
```

**When to apply this pattern:**
- 2+ switch/match blocks that branch on the same discriminator
- A single switch with >5 cases that maps input → output with no side effects
- If/else chains comparing the same variable to string/enum literals

**When NOT to apply:**
- Switch cases with side effects (function calls, mutations, I/O)
- Pattern matching with destructuring (Rust `match`, TS discriminated unions)
- 2-3 case switches where the data structure adds more complexity than it removes

---

## The Orchestrator Trap

The most common failure mode: you extract 7 child components and declare victory, but the parent file is still 1000+ lines because all the script logic stayed behind. Extracting template sections without extracting logic is a half-finished refactor.

**What stays in the orchestrator script:** reactive state declarations (`$state`, `useState`), the top-level form submit handler (as a thin orchestrator calling extracted functions), component wiring/layout.

**What gets extracted to modules:**

| Logic Type | Extract To | Call From Component As |
|-----------|-----------|----------------------|
| Fee/pricing calculations | `pricing.ts` | `$derived(calculateTotal(inputs))` |
| Validation functions | `validators.ts` | `const errors = validateParent(fields)` |
| API calls (submit, validate coupon, poll) | `api.ts` or domain service | `await submitRegistration(payload)` |
| Error-clearing logic | `error-helpers.ts` or inline in validators | `clearResolvedErrors(errors, fields)` |
| Data transformation (build payload) | `transforms.ts` | `const payload = buildSubmissionPayload(state)` |
| Session/draft persistence | `draft-persistence.ts` | `saveDraft(key, state)` |

These are all pure functions that happen to be called inside reactive contexts. Moving `calculateTotal` to a module doesn't break reactivity — `$derived(calculateTotal(children, coupons))` works identically.

**After extraction, the orchestrator should contain:** state declarations, imports, thin wiring (`$effect` calling extracted functions), and template layout with child components. Target: under the "Must Split" threshold.

---

## What NOT to Refactor

This is as important as knowing what to refactor. Over-refactoring is worse than under-refactoring.

| Leave It Alone If... | Why |
|---|---|
| File is under the "Review At" threshold | It's fine. Don't create 50-line files for the sake of it. |
| Logic is used in exactly one place | Extraction adds indirection with zero reuse benefit. |
| The "duplication" is coincidental (same shape, different domain) | These will diverge. Coupling them creates pain later. |
| Extracting requires passing 6+ props/params | The coupling proves these belong together. |
| The file is a generated file or config | Don't split `schema.prisma` or generated types. |
| It's a test file under the test threshold | Test files are allowed to be longer — readability > DRY in tests. |

---

## Refactoring Process

When running as a dedicated stream:

1. **Scan** — `find` files exceeding split thresholds in the changed file set (`git diff --name-only` against the base branch)
2. **Prioritize** — Rank by severity: files over "Must Split" first, then "Review At" files with mixed concerns
3. **Plan splits** — For each file, determine the split strategy before writing any code
4. **Extract** — One file at a time. Move code, update imports, verify no broken references
5. **Verify** — After each extraction: run the project's type checker and test suite. A refactor that breaks types is not a refactor.
6. **Dedup** — After all splits, scan for cross-file duplication introduced by different streams writing similar logic independently
7. **Don't touch what works** — If a file is under threshold and has no duplication issues, leave it alone

---

## Anti-Patterns This Skill Catches

| Pattern | Problem | Fix |
|---------|---------|-----|
| 1500-line component with 6 form sections | Unnavigable, untestable, merge-conflict magnet | Split into section components |
| index.ts with 20 function implementations | Entry point is a dumping ground | Move to domain modules, re-export |
| Same fetch-parse-validate in 4 API calls | Copy-paste across streams | Extract typed API client |
| 3 switch blocks on the same `status` field | Parallel maintenance burden | Consolidate into config object |
| Validation logic inlined in submit handler | Untestable, repeated across forms | Extract to validator module |
| Mobile + desktop markup duplicated | Double maintenance for responsive UI | Single responsive component or slot pattern |
| 80-line `$effect` or `useEffect` | Side effect doing too much | Extract to named function or custom hook |
| Formatting functions scattered across files | `formatCurrency()` defined 3 times | Shared `formatters.ts` |

---

## Rationalization Prevention

| You're thinking... | Reality |
|---|---|
| "These sections are all related, they belong in one file" | Related ≠ same file. A form's parent section and payment section are related but are separate concerns with separate state. |
| "Splitting will create too many files" | 8 focused 150-line files are easier to navigate than 1 unfocused 1200-line file. Every editor has file search. |
| "I'll just extract the one worst function" | If the file exceeds "Must Split," extract sections, not individual functions. Function extraction without structural decomposition just makes the monolith slightly less monolithic. |
| "This duplication is only in 2 places" | Then leave it. Rule of Three. Come back when there's a third instance. |
| "I should create a utils/ directory for shared code" | Name it by domain, not by "shared." `formatters.ts`, `validators.ts`, `api-client.ts` — not `utils.ts`. |
| "The tests still pass so the refactor is correct" | Types must also pass. Run the type checker. A refactor that compiles but has `any` leaks is incomplete. |
| "The orchestrator needs all this logic because it's reactive/stateful" | Reactive state declarations stay. Derived calculations, validation logic, API calls, and effect bodies are pure functions — extract them to modules and call them from the component. `$derived(calculateTotal(children, coupons))` is cleaner than 80 lines of inline arithmetic. |
| "I extracted 7 components, the refactor is done" | Check the orchestrator line count. If it still exceeds the split threshold, you moved the template but left the script bloated. Extract logic modules, not just UI components. |
