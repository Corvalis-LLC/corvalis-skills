# Status File Schema

> Reference for `/stream` skill. See SKILL.md for overview.

## Location

Status files are companions to plan files:

```
docs/plans/2026-02-13-vendmaster-feature-parity.md           # Plan (read-only)
docs/plans/2026-02-13-vendmaster-feature-parity.status.json  # Status (managed by /stream)
```

The slug is derived from the plan filename by removing the `.md` extension.

## Schema

```json
{
  "planFile": "docs/plans/2026-02-13-vendmaster-feature-parity.md",
  "finalValidationMode": "codex",
  "createdAt": "2026-02-13T10:00:00Z",
  "updatedAt": "2026-02-16T14:30:00Z",
  "streams": {
    "1": {
      "name": "Foundation — Universal Notes + Activity Log UI",
      "status": "completed",
      "dependencies": [],
      "baselineSkills": ["auto-typescript", "auto-database", "auto-evolution"],
      "claimedAt": "2026-02-13T10:05:00Z",
      "completedAt": "2026-02-13T12:30:00Z",
      "verification": {
        "typeCheck": true,
        "tests": true,
        "build": true,
        "timestamp": "2026-02-13T12:28:00Z"
      }
    },
    "2": {
      "name": "Financial Operations — Collections + Owners",
      "status": "in_progress",
      "dependencies": ["1"],
      "baselineSkills": ["auto-typescript", "auto-compliance", "auto-serialization", "auto-security"],
      "claimedAt": "2026-02-14T09:00:00Z",
      "completedAt": null,
      "verification": null
    },
    "3": {
      "name": "Contract & Sales Workflows",
      "status": "pending",
      "dependencies": ["1"],
      "baselineSkills": ["auto-typescript", "auto-api-design", "auto-resilience", "auto-state-machines"],
      "claimedAt": null,
      "completedAt": null,
      "verification": null
    }
  }
}
```

## Field Reference

### Top-level

| Field | Type | Description |
|-------|------|-------------|
| `planFile` | string | Relative path to the plan markdown file |
| `finalValidationMode` | enum | `codex` or `review` — controls the auto-injected final stream |
| `createdAt` | ISO 8601 | When the status file was first created |
| `updatedAt` | ISO 8601 | Last modification timestamp |
| `streams` | object | Map of stream ID → stream status object |

### Stream object

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Stream title from plan header |
| `status` | enum | `pending` \| `in_progress` \| `completed` |
| `dependencies` | string[] | Stream IDs this stream depends on |
| `baselineSkills` | string[] | Auto-* skills this stream must load (from plan's `## Required Skills` section) |
| `claimedAt` | ISO 8601 \| null | When this stream was claimed by a session |
| `completedAt` | ISO 8601 \| null | When this stream was marked complete |
| `verification` | object \| null | Verification results (see below) |

### Verification object

| Field | Type | Description |
|-------|------|-------------|
| `typeCheck` | boolean | `npm run check` passed |
| `tests` | boolean | Relevant tests passed |
| `build` | boolean | `npm run build` passed |
| `timestamp` | ISO 8601 | When verification was run |

### Legion object (optional)

Present only on streams with `**Legion:** Yes` annotation in the plan.

| Field | Type | Description |
|-------|------|-------------|
| `enabled` | boolean | Whether legion mode is active (false = fell back to solo) |
| `fallbackReason` | string \| null | Why legion was disabled, if applicable |
| `currentWave` | string \| null | Active wave type: `"T"`, `"I"`, `"D"`, `"R"`, or null |
| `waves` | object | Map of wave type → wave status object |

### Wave status object

| Field | Type | Description |
|-------|------|-------------|
| `status` | enum | `pending` \| `in_progress` \| `completed` |
| `agents` | array | List of agent task objects |
| `verification` | object \| null | Post-wave verification results |

### Agent task object

| Field | Type | Description |
|-------|------|-------------|
| `task` | string | Short description of what this agent does |
| `files` | string[] | File paths this agent owns |
| `status` | enum | `pending` \| `in_progress` \| `completed` \| `failed` |
| `retries` | number | Number of retry attempts (max 2) |

Example:

```json
"legion": {
  "enabled": true,
  "fallbackReason": null,
  "currentWave": "I",
  "waves": {
    "T": {
      "status": "completed",
      "agents": [
        { "task": "Write capacity service tests", "files": ["src/lib/server/services/capacity.test.ts"], "status": "completed", "retries": 0 },
        { "task": "Write registration API tests", "files": ["src/routes/api/registrations/+server.test.ts"], "status": "completed", "retries": 0 }
      ],
      "verification": { "typeCheck": true, "tests": false, "build": true, "timestamp": "2026-03-31T..." }
    },
    "I": {
      "status": "in_progress",
      "agents": [
        { "task": "Implement capacity service", "files": ["src/lib/server/services/capacity.ts"], "status": "completed", "retries": 0 },
        { "task": "Implement registration endpoint", "files": ["src/routes/api/registrations/+server.ts"], "status": "in_progress", "retries": 0 }
      ],
      "verification": null
    }
  }
}
```

## Status Transitions

```
pending → in_progress    (stream claimed by a session)
in_progress → completed  (verification gate passed)
in_progress → pending    (session abandoned, stream unclaimed)
```

## Stream ID Format

Stream IDs match what appears in the plan headers:
- Simple: `"1"`, `"2"`, `"3"`
- Sub-streams: `"4A"`, `"4B"`, `"5A"`
- **Special:** `"final"` — auto-injected Final Validation & Cleanup stream

Sub-streams are tracked as independent entries. The parent stream (e.g., "4") is not tracked separately — only the sub-streams appear in the status file.

### Final Validation Stream

The `"final"` stream is automatically injected during initialization (Phase 2). It does NOT correspond to a `## Stream` header in the plan markdown — it is generated by the `/stream` skill. Its dependencies are set to ALL other stream IDs, ensuring it always runs last. Its behavior is controlled by the top-level `finalValidationMode` field, which is parsed from the plan's `## Final Validation Mode` section.

Example:
```json
"final": {
  "name": "Final Validation & Cleanup",
  "status": "pending",
  "dependencies": ["1", "2", "3", "4A", "4B"],
  "claimedAt": null,
  "completedAt": null,
  "verification": null
}
```

When the Final Validation stream completes, it deletes both the plan file and this status file as part of cleanup (Phase 5F.4).

## Parsing Rules

Extract streams from plan markdown using these patterns:

```
## Stream N: Title        →  id: "N",  name: "Title"
## Stream N — Title       →  id: "N",  name: "Title"
### NA. Sub-title         →  id: "NA", name: "Sub-title" (under parent stream N)

**Dependencies:** Stream 1 (notes component)  →  dependencies: ["1"]
**Dependencies:** Streams 1 and 2              →  dependencies: ["1", "2"]
**Dependencies:** None                         →  dependencies: []
```

## Concurrency

The status file uses optimistic concurrency. Before writing a claim:
1. Read the file
2. Check the target stream is still `pending`
3. Write the updated file

Two sessions claiming different streams simultaneously is safe — they write to different keys. Two sessions claiming the same stream is unlikely (human coordination) but if it happens, the last write wins.
