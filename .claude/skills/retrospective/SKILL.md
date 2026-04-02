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

### Step 1: Session review (executed by the skill itself)

Review the entire conversation in this session and organize **points that could become rules** as a bulleted list, from the following perspectives:

- **Recurring corrections**: Patterns where the same type of mistake was corrected multiple times
- **User feedback/direction**: Things the user said like "do it this way" or "stop doing that"
- **Implicit conventions**: Patterns consistently applied in the codebase but not yet documented
- **Build/test failures**: Error patterns that could have been prevented
- **Architectural decisions**: Layer structure or module organization decisions that should apply going forward

For each point, describe:
- **What happened**: Concrete description of the situation
- **Why it should become a rule**: Reason for preventing recurrence or improving quality
- **Rule summary**: Overview of what the rule should say

If no points are found, report "No new patterns worth adding as rules were found in this session" and stop.

### Step 2: Delegate cross-referencing with existing rules to an Agent

Pass the points organized in Step 1 to an Agent to cross-reference against existing rules and create concrete update proposals.

Agent tool:
  description: "Cross-reference with existing rules"
  prompt: Use the prompt template below
  subagent_type: "general-purpose"

The agent's prompt must include the following:

```
Cross-reference the following points against existing rules in `.claude/rules/` and create concrete update proposals.

## Candidate points for rules

[Paste the bulleted list from Step 1 here]

## Procedure

1. Read all files in `.claude/rules/*.md` to understand existing rule content
2. Also read CLAUDE.md to check for overlap with already-documented content
3. For each point, determine:
   - Already covered by existing rules -> Skip
   - Should be appended to an existing rule -> Propose append
   - Should be created as a new rule file -> Propose new file
   - Existing rule needs modification -> Propose modification
4. Format each proposal as shown below

## Output format

If no proposals, respond with "Existing rules already provide sufficient coverage."

If there are proposals, output in the following format:

### Proposal N: [Proposal title]

- **Type**: New addition / Append to existing rule / Modify existing rule
- **Target file**: `.claude/rules/[filename].md`
- **Rationale**: Why this rule is needed
- **Changes**: Specific rule text to add or change (in a format ready to write directly to the file)
  - For appending to existing rules: Where to insert (after which section) and the content to add
  - For new files: Full file content (including YAML frontmatter)
  - For modifications: Before and after

## Important constraints

- Match the style and tone of existing rule files
- Avoid over-rulemaking (do not create rules for one-off special cases)
- Keep rules concise (focused and clearly stated)
- New rule files must include appropriate `paths` scope in YAML frontmatter
- Do not duplicate content already in CLAUDE.md
```

### Step 3: Present proposals one at a time -> confirm -> apply loop

Process proposals returned by the Agent **one at a time, in order**. Repeat the following loop for each proposal:

#### 3a. Display proposal details

Show the current proposal's details:
- Proposal title
- Type (new addition / append / modification)
- Target file
- Rationale
- Specific changes

#### 3b. Confirm with AskUserQuestion

**Ask about only one proposal per AskUserQuestion.** Do not batch them.

```
AskUserQuestion:
  question: "Proposal N: [title] - Apply this?"
  options:
    - label: "Apply"
      description: "[type] to [target file]"
    - label: "Skip"
      description: "Do not apply this proposal"
```

#### 3c. Process based on response

- **"Apply"**: Delegate the change to an Agent, and **wait for completion before moving to the next proposal**.
  - The agent's prompt must include the target file path, type, and specific changes
  - The agent must use the Edit tool for updating existing files, and only use the Write tool when creating new files
- **"Skip"**: Do nothing and move to the next proposal.

After all proposals are processed, briefly report the list of changed files. If no changes were made, report that instead.
