# Recon Output Interpretation

How summon consumes `corvalis-recon analyze` output during Phase A1 brainstorming.

## JSON Schema Reference

```json
{
  "version": "0.1.0",
  "project": {
    "name": "my-app",
    "root": ".",
    "languages": { "typescript": { "file_count": 42, "lines_of_code": 3200 }, ... },
    "entry_points": ["src/main.ts", "src/server.ts"],
    "config_files": ["tsconfig.json", "package.json"],
    "directory_tree": [{ "path": "src/auth", "file_count": 6 }, ...]
  },
  "files": [{
    "path": "src/auth/middleware.ts",
    "language": "typescript",
    "symbols": [{ "name": "authenticate", "kind": "function", "line": 5, "end_line": 20, "exported": true, "signature": "(req: Request): boolean" }],
    "imports": [{ "source": "./tokens", "specifiers": ["verifyJwt"], "kind": "named", "line": 1 }],
    "exports": [{ "name": "authenticate", "kind": "named", "line": 5 }],
    "metrics": { "total_lines": 45, "code_lines": 38, "cyclomatic_complexity": 8, "max_nesting_depth": 3, "functions": [...] }
  }],
  "graph": {
    "adjacency": { "src/main.ts": [{ "target": "src/auth/middleware.ts", "specifiers": ["authenticate"], "resolved": true, "external": false }] },
    "entry_points": ["src/main.ts"],
    "leaf_nodes": ["src/utils/constants.ts"],
    "cycles": [["src/a.ts", "src/b.ts", "src/a.ts"]],
    "stats": { "total_files": 42, "total_edges": 87, "most_imported": ["src/lib/index.ts"] }
  },
  "hotspots": [{ "path": "src/auth/middleware.ts", "function": "authenticate", "metric": "cyclomatic_complexity", "value": 12, "threshold": 10 }],
  "warnings": [{ "path": "src/generated.ts", "message": "skipped: file exceeds 1MB" }],
  "summary": { "total_files": 42, "total_symbols": 310, "total_lines_of_code": 3200, "avg_complexity": 4.2 }
}
```

---

## Mapping Recon Output to Plan Decisions

### 1. File Ownership Boundaries for Streams

Use the dependency graph to draw stream boundaries along natural module seams:

- **Directory clusters:** Group files in the same directory that import each other heavily into a single stream. The `directory_tree` field shows where files concentrate.
- **Incoming edge count:** Files in `graph.stats.most_imported` are hub files. A hub file and everything that depends on it often form a natural stream boundary.
- **Entry points:** `graph.entry_points` are roots of the dependency tree. Each entry point and its transitive dependency subtree is a candidate stream.
- **Leaf nodes:** `graph.leaf_nodes` are utilities with no outgoing project imports. They can be assigned to whichever stream needs them, or bundled into a foundation stream.

**Ownership conflict check:** If two candidate streams would both need to modify the same file, either:
- Assign exclusive ownership to one stream and make the other depend on it
- Split the file's concerns so each stream owns a distinct part

Use the `adjacency` map to verify: if Stream A's files import from Stream B's files, that's a real dependency.

### 2. Dependency Graph for the Swarm Gate

Recon's dependency graph replaces guesswork with evidence during dependency optimization:

**Real dependency test:** A stream dependency is real only when `graph.adjacency` shows an import edge from one stream's files to another stream's files. If no edge exists, the dependency is false — remove it.

**Procedure:**
1. For each proposed inter-stream dependency, check whether any file in the downstream stream imports any file in the upstream stream via `graph.adjacency`
2. If no import edge exists, the dependency is false
3. If the edge targets only a type or interface (check the `specifiers` against the upstream file's `symbols` where `kind` is `interface` or `type_alias`), the dependency can be unlocked by extracting types into a sub-stream

**Cycle detection:** If `graph.cycles` is non-empty, streams that span files in the same cycle cannot be fully parallelized. Either break the cycle (refactor) or enforce sequential execution for those streams.

### 3. Complexity Hotspots for Legion Viability

The `hotspots` array identifies functions exceeding complexity thresholds. Use these to assess legion viability per stream:

| Hotspot pattern | Legion implication |
|----------------|-------------------|
| Hotspots spread across many files in the stream | Good candidate — agents work on different files |
| Hotspots concentrated in one file | Poor candidate — agents would conflict on the same file |
| No hotspots in the stream | Neutral — legion viability depends on task count and file independence |
| High nesting depth hotspots | Complex algorithmic work — may need full context, not agent-friendly |

**Refactoring signals:** A stream with many hotspots might benefit from a pre-refactoring sub-stream that simplifies complex functions before feature work begins.

### 4. Language Breakdown and Entry Points for Skill Assignment

Use `project.languages` and file-level `language` fields to assign per-stream skills:

| Recon field | Skill assignment signal |
|------------|------------------------|
| `language: "typescript"` or `"tsx"` | `auto-typescript` |
| `language: "svelte"` | `auto-svelte`, `auto-accessibility`, `auto-layout` |
| Files with `imports` from external packages like `express`, `hono`, `fastify` | `auto-api-design`, `auto-resilience` |
| Files with `imports` referencing `prisma`, `drizzle`, `sqlx`, `knex` | `auto-database` |
| Files in directories named `auth`, `session`, `permission` | `auto-security` |
| Files with symbols named `*Schema`, `*Validator`, `validate*` | `auto-serialization` |
| `project.config_files` containing `Dockerfile`, `.env` | `auto-hardcoding` |

Entry points (`graph.entry_points`) indicate the application's main execution paths. Streams containing entry points are foundational and should be scheduled early.

### 5. Token Budget Guidance

Recon's `--budget <tokens>` flag controls output size. Choose the budget based on plan complexity:

| Codebase size | Recommended budget | Rationale |
|--------------|-------------------|-----------|
| < 50 files | No budget (full output) | Fits comfortably in context |
| 50-200 files | `--budget 16000` | Full detail for important files, summaries for the rest |
| 200-500 files | `--budget 8000` | Focus on hub files and hotspots |
| > 500 files | `--budget 4000` | Hotspots and graph structure only |

When budget truncation is active, recon preserves these sections unconditionally:
- Project overview (languages, entry points, config files)
- Full dependency graph (adjacency, cycles, stats)
- All hotspots

File detail is progressively reduced: top-scored files get full symbols and metrics, middle-tier files get summary-only (name, kind, line), low-scored files are omitted.

---

## Validation Rules

Before consuming recon output, summon verifies:

1. **`version` field exists** — confirms the output is from a compatible recon version
2. **`files` array is non-empty** — an empty array means the project has no supported source files (recon only covers TypeScript/JavaScript/Svelte)
3. **JSON parses successfully** — malformed output triggers immediate fallback

If any check fails, summon falls back to organic `Glob`/`Grep`/`Read` exploration with zero degradation. The single-line warning goes to stderr; the user sees no disruption.
