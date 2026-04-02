---
name: rulify
description: >
  Cross-check modified code against each rule in .claude/rules/, detect violations, and auto-fix them. Use as a self-check before code review.
  TRIGGER when: the user wants to check rule compliance, asks for a self-review, or says "rulify".
  DO NOT TRIGGER when: the user is asking for a general code review unrelated to .claude/rules/.
argument-hint: "[PR number or focus area (optional)]"
---

Inspect whether modified code complies with the rules defined in `.claude/rules/`, and automatically fix clear violations.

## User-specified focus

$ARGUMENTS

## Processing Flow

### Step 1: Collect changed files and applicable rules

Collect the following information:

1. **Get changed files**: Retrieve using the appropriate method based on the argument:
   - **If a PR number is specified** (e.g., `#123`, `123`): Get via `gh pr diff {PR_number} --name-only`
   - **Otherwise (default)**: Get all diffs from the branch's origin + uncommitted changes
     - `git fetch origin` to get the latest remote info
     - `git merge-base HEAD origin/main` to identify the base commit (try origin/master if origin/main doesn't exist)
     - `git diff --name-only {base_commit}...HEAD` to get committed changes
     - `git diff --name-only` to get unstaged changes
     - `git diff --name-only --cached` to get staged changes
     - `git ls-files --others --exclude-standard` to get untracked files
     - Deduplicate and merge all results
2. **List rule files**: List all `.claude/rules/*.md` files.
3. **Check CLAUDE.md**: If `CLAUDE.md` exists at the project root, read it and use it as an additional rule source.

If there are no changed files, report "No changed files found" and stop.

### Step 2: Launch agents in parallel for each rule

Launch an Agent for each collected rule file. **Independent rules must be launched in parallel.**

Each agent's prompt must include the following:

```
Inspect the changed files for violations against the following rule.

## Rule
Rule name: {rule_file_name}

{Rule file contents}

## Files to inspect
{List of changed files (paths only)}

## Additional rules from CLAUDE.md (if applicable)
{Relevant sections from CLAUDE.md. Omit if no CLAUDE.md exists}

## Procedure

1. Understand the rule's content
2. Read each changed file and inspect for violations against the rule
3. If the rule has a `paths` scope, skip files outside that scope
4. Report inspection results in the format below

## Output format

### If no violations
Report only: ✅ {rule_name}: No violations

### If violations found
Report in the following format:

#### ❌ {rule_name}: Violations found

For each violation:
- **File**: target_file_path:line_number
- **Violation**: What violates the rule
- **Severity**: 🔴 Clear violation / 🟡 Gray area
- **Fix**: Specific fix description

## Important constraints

- Do not flag anything not explicitly stated in the rule
- Do not report "nice to have" improvements
- Only report items that clearly violate the rule
- Do not flag unchanged parts of files (referencing them for context is OK)
- This rule inspection agent only reads code -- it does not modify code (actual auto-fixes are handled in Step 4 by the main agent)
```

Agent settings:
- `subagent_type`: "general-purpose"
- `model`: "sonnet" (for speed)
- All rule inspection agents must be **launched in parallel in a single message**

### Step 3: Aggregate and display results

Aggregate all agent results and display in the following format:

```
## Rulify Results

### Summary
- Rules inspected: N
- ✅ No violations: N
- ❌ Violations found: N

### Violation details
(Display results from rules with violations here)
```

### Step 4: Auto-fix

If 🔴 clear violations exist, execute auto-fixes:

1. Review the fix details for 🔴 (clear violations)
2. Apply fixes to each file using the Edit tool
3. After fixes, run formatters/tests/builds as needed

🟡 Gray area violations are reported only -- no fixes applied.

### Step 5: Report fix results

```
## Rulify Complete

### Auto-fixed
- {file_path}: {fix summary}
- ...

### Needs review (gray area)
- {file_path}: {issue description}
- ...

### No fixes needed
(If all rules were satisfied)
All rules passed! No violations found.
```

## Important Rules

1. **Parallel execution**: Rule inspection agents must always be launched in parallel. Do not run sequentially.
2. **Strict scoping**: Only inspect changed files.
3. **Avoid false positives**: Do not flag anything not explicitly stated in the rules. If ambiguous, mark as 🟡 gray area.
4. **Auto-fix safety**: Only auto-fix 🔴 clear violations. 🟡 items are reported only.
5. **Separation from formatters**: Leave formatting issues to formatters; focus on rule violation inspection.
