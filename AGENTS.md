# brick-poc local agent instructions

Local-only guidance for AI assistant interactions with this repository.

## Writing and style

Before returning any prose, ensure clarity, precision, and professional tone.
This applies to responses, documents, reports, PR and commit text, and code
comments. Preserve facts, quotations, technical meaning, and necessary legal,
medical, financial, or security qualifications.

## Output units

Prefer octet-based units for data sizes and transfer rates, such as `8 MB/s`,
rather than bit-based alternatives. Use bits when the measured data amount is
less than one octet or the source specifically measures bits. Use metric units
and prefixes for other measurements.

## Code review guidelines

When reviewing changes, evaluate for:

- **Scope and correctness:** Does it solve the stated problem? Check assumptions,
  edge cases, invalid input, failure paths, and compatibility of APIs.
- **Architecture and maintainability:** Does it fit the project's patterns and
  responsibilities? Look for avoidable duplication, unnecessary complexity,
  unclear naming, and missing documentation.
- **Tests:** Are important behavior, edge cases, and failure paths covered at
  the right level? Prefer focused, reliable tests.
- **Security and data protection:** Check validation at trust boundaries,
  server-side authorization, injection risks, dependency changes, secrets,
  personal data, and sensitive information in logs or responses.
- **Data integrity and concurrency:** Review migrations, data lifecycle,
  retention, ownership, race conditions, duplicate work, and consistency.
- **Performance and reliability:** Look for avoidable queries, network calls,
  memory use, or repeated work. Check timeout behavior and bounded retries.
- **Release and recovery:** Consider deployment order, compatibility,
  migration safety, staged rollout, and rollback or recovery.

Prioritize actionable defects over style preferences. For each finding, give
the affected location, explain the concrete impact and evidence, and suggest a
practical correction.

## Git commits and tool tracking

- **Do not attribute authorship to AI tools or agents.** Tools are assistants,
  not authors. Never use `Co-authored-by:` or `Signed-off-by:` for AI assistance.
- Do not include any mention of AI tools, LLMs, or assistant names in commit
  messages, file content, or any repository artifact as authors or contributors.
- The use of AI tools for assistance is acceptable but must not be represented
  as authorship or co-authorship in any form.

## Delivery scope

Keep changes focused on agreed acceptance criteria. Log additions outside
those criteria as proposals before implementation. Do not commit unagreed
scaffolding or exploratory tooling unless explicitly approved.

## File and directory naming

- Use lowercase hyphen-separated words for new, freely named developer commands,
  operational files, documentation, and general-purpose directories.
- Choose naming by purpose, not separators by grammar.
- Commands: prefer action-target names; retain useful scope qualifiers when needed.
- Documents/configuration: prefer descriptive noun phrases.
- Do not create one-file intermediary directories merely to separate words.
- Preserve required casing, extensions, plural directories, and filenames for
  tool-prescribed, generated, bundled, or externally consumed names.

## Changes and validation

- Prefer existing scripts/helpers and the smallest complete change.
- Follow established conventions and patterns in the codebase.
- Run relevant checks in the current worktree before claiming success.
- Never claim unrun checks passed.
- Keep handoffs concise: outcome, meaningful changes, and unresolved blockers.
- Do not commit or push local setup artifacts.
