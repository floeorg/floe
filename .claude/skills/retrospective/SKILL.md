---
name: retrospective
description: >
  Review the session's interactions and propose additions/updates to `.claude/rules/` conventions.
  TRIGGER when: the user asks to review the session, retrospect, or says "retrospective".
  DO NOT TRIGGER when: the user is asking for a general code review or rule check (use rulify instead).
argument-hint: "[focus area or perspective (optional)]"
---

Review this session's interactions and propose patterns that should be added or updated in `.claude/rules/`.

## User-specified focus

$ARGUMENTS

## Processing Flow

### Step 1: Session review

Review the entire conversation and organize **points that could become rules**:

- **Recurring corrections**: Same type of mistake corrected multiple times
- **User feedback/direction**: "do it this way" or "stop doing that"
- **Implicit conventions**: Patterns consistently applied but not documented
- **Build/test failures**: Error patterns that could have been prevented
- **Architectural decisions**: Layer structure or module organization decisions

For each point, describe what happened, why it should become a rule, and the rule summary.

### Step 2: Cross-reference with existing rules

Delegate to an Agent to:
1. Read all `.claude/rules/*.md` files and `CLAUDE.md`
2. For each point, determine: already covered (skip), append to existing rule, create new file, or modify existing
3. Format concrete proposals with type, target file, rationale, and specific changes

### Step 3: Present proposals one at a time

Process proposals one by one:
1. Display proposal details
2. Ask user: "Apply this?" (Apply / Skip)
3. If Apply: delegate to an Agent to make the change
4. Move to next proposal

Report changed files at end.
