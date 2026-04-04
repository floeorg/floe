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

1. Get changed files the same way as `/rulify`
2. Group changed files by layer/directory

If there are no changed files, report "No changed files found" and stop.

### Step 2: For each layer, compare against siblings

Launch an Agent per group to compare changed files against 2-3 existing sibling files in the same directory.

Check for inconsistencies in:
- **Naming**: function names, variable names, type names, file names
- **Structure**: code organization, ordering of imports/types/functions
- **Patterns**: error handling, conversions, trait implementations, query building
- **API surface**: how public functions/types are exposed
- **Imports**: importing from the same places siblings do

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

Fix 🔴 (should fix) inconsistencies using Edit. Leave 🟡 (minor/cosmetic) items as reported only.

### Step 5: Report

```
## Alignify Complete

### Fixed
- {file}: {what was aligned}

### Minor (cosmetic, not fixed)
- {file}: {what's different}

### All clear
All changed code follows existing patterns.
```
