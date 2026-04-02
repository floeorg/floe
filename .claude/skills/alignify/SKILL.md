---
name: alignify
description: >
  Check that changed code follows the same patterns as existing code in each layer it touches.
  Compares against sibling files in the same directories to catch inconsistencies in naming,
  structure, error handling, and conventions.
  TRIGGER when: the user says "alignify", "align", or asks to check consistency with the codebase.
  DO NOT TRIGGER when: the user is asking for a general code review or rule check.
argument-hint: "[focus area (optional)]"
---

Check that changed code is consistent with existing code in the same layers. Find pattern drift and fix it.

## User-specified focus

$ARGUMENTS

## Processing Flow

### Step 1: Identify changed files and their layers

1. Get changed files the same way as `/rulify`:
   - `git fetch origin`
   - `git merge-base HEAD origin/main` (or the epic branch)
   - `git diff --name-only {base}...HEAD` + unstaged + staged + untracked
   - Deduplicate
2. Group changed files by layer/directory (e.g. `packages/domain/src/models/`, `packages/infrastructure/src/db/repositories/`, `apps/api/src/presentation/`, `apps/client/src/routes/`, etc.)

If there are no changed files, report "No changed files found" and stop.

### Step 2: For each layer, compare against siblings

For each group of changed files, launch an Agent to compare them against existing files in the same directory.

Launch all agents in parallel. Each agent's prompt:

```
Compare the changed files against existing sibling files in the same directory to find pattern inconsistencies.

## Changed files in this layer
{list of changed file paths}

## Procedure

1. Read each changed file
2. Read 2-3 existing sibling files in the same directory (pick ones that look mature/representative, not other recently changed files)
3. Compare and look for inconsistencies in:
   - **Naming**: function names, variable names, type names, file names — do they follow the same convention as siblings?
   - **Structure**: is the code organized the same way? Same ordering of imports, types, functions?
   - **Patterns**: does it use the same patterns for error handling, conversions, trait implementations, query building, etc.?
   - **API surface**: are public functions/types consistent with how siblings expose theirs?
   - **Imports**: does it import from the same places siblings do, or does it reach into wrong layers?

4. Report findings in this format:

## Output format

### If consistent
Report: ✅ {directory}: Consistent with siblings

### If inconsistencies found
Report:

#### ⚠️ {directory}: Inconsistencies found

For each inconsistency:
- **File**: path:line_number
- **Issue**: What's different from the sibling pattern
- **Sibling example**: How the sibling does it (file + line)
- **Severity**: 🔴 Should fix / 🟡 Minor, cosmetic
- **Fix**: What to change

## Important constraints

- Only flag things where siblings are consistent and the changed file deviates
- If siblings themselves are inconsistent, don't flag it — there's no established pattern
- Don't flag things that are intentionally different (e.g. a model with different fields is fine, but a model with a different struct layout convention is not)
- Focus on patterns and conventions, not business logic
```

Agent settings:
- `subagent_type`: "general-purpose"
- `model`: "sonnet"
- All agents launched in parallel

### Step 3: Aggregate results

```
## Alignify Results

### Summary
- Layers checked: N
- ✅ Consistent: N
- ⚠️ Inconsistencies: N

### Details
(Show inconsistencies here)
```

### Step 4: Auto-fix

Fix 🔴 inconsistencies using Edit. Leave 🟡 items as reported only.

### Step 5: Report

```
## Alignify Complete

### Fixed
- {file}: {what was aligned}

### Minor (cosmetic, not fixed)
- {file}: {what's different}

### All clear
(If everything was consistent)
All changed code follows existing patterns.
```
