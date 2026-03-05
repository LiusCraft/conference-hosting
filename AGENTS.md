# AGENTS Guide

## Purpose

This file gives coding agents repository-specific operating guidance.
Follow this before making any change.

## Repository Snapshot (as of 2026-03-05)

- Repo currently contains design documentation only.
- Primary content: `docs/ai_meeting_hosting_design.md`.
- No application source tree detected (`src/`, `app/`, `server/`, etc.).
- No build metadata detected (`package.json`, `pyproject.toml`, `go.mod`, `Cargo.toml`, `Makefile`).
- No test folders or test runners detected.

## Rule Files Check

The following rule files were explicitly checked and are not present:

- `.cursor/rules/`
- `.cursorrules`
- `.github/copilot-instructions.md`

If any of the above are added later, treat them as high-priority instructions and update this file.

## Working Norms for Agents

- Do not invent repository commands that do not exist.
- Prefer minimal, auditable changes.
- Keep documentation and implementation in sync.
- If introducing a toolchain, document it in this file immediately.
- Preserve existing language in docs unless asked to translate.
- Agents must respond to users in Chinese by default.

## Build / Lint / Test Commands

Current status: no build/lint/test toolchain is configured in this repository.

### Build

- Command: `N/A`
- Notes: no build system detected.

### Lint

- Command: `N/A`
- Notes: no linter configuration detected.

### Test (all)

- Command: `N/A`
- Notes: no test runner or test files detected.

### Test (single test)

- Command: `N/A`
- Notes: single-test execution is not available until a test framework is added.

## If You Introduce a Toolchain

When adding code, also add explicit scripts/targets and update this file.
At minimum, provide all of the following:

- Install dependencies command.
- Build command.
- Lint command.
- Full test command.
- Single test command with a concrete example.

Preferred pattern (choose one ecosystem and keep it consistent):

- JS/TS via npm scripts (`npm run build`, `npm run lint`, `npm test -- <file>`).
- Python via pytest (`pytest`, `pytest path/to/test_file.py::test_case`).
- Go via `go test` (`go test ./...`, `go test ./pkg -run TestName`).
- Rust via cargo (`cargo test`, `cargo test test_name`).

## Code Style Guidelines

There is no enforced formatter/linter yet.
Use the conventions below for any new code until project-specific rules are added.

### General

- Favor clarity over cleverness.
- Keep functions focused and small.
- Avoid hidden side effects.
- Remove dead code instead of commenting it out.
- Write deterministic code where possible.

### Imports / Dependencies

- Use absolute or project-standard import style consistently.
- Group imports by standard library, third-party, then local modules.
- Keep imports sorted; avoid unused imports.
- Do not add heavy dependencies without clear justification.
- Prefer mature, well-maintained libraries.

### Formatting

- Use the formatter standard for the chosen language once configured.
- Default indentation: 2 spaces for JS/TS, 4 spaces for Python.
- Keep line length readable (target <= 100 chars unless language norms differ).
- Use trailing commas where formatter expects them.
- Keep markdown lists and headings consistent.

### Types and Interfaces

- Prefer explicit types at module boundaries (public APIs, IO, events).
- Validate external input at runtime (network payloads, files, env vars).
- Encode domain concepts with named types/structs/interfaces, not ad-hoc maps.
- Avoid `any`/untyped escape hatches unless unavoidable.
- Document non-obvious units and constraints (ms, bytes, sample rate, channels).

### Naming Conventions

- Use descriptive, domain-driven names.
- Keep acronyms consistent (`ws`, `tts`, `asr`, `vad`, `aec`), avoid mixed forms.
- Prefer verb phrases for functions (`sendAudioFrame`, `decodeTtsChunk`).
- Prefer noun phrases for data models (`AudioFrame`, `SessionConfig`).
- Avoid ambiguous names like `data`, `obj`, `tmp` except in tiny local scopes.

### Error Handling

- Fail fast on invalid configuration.
- Return structured errors with actionable context.
- Never swallow exceptions/errors silently.
- Log enough detail for diagnosis but avoid leaking secrets/tokens.
- Differentiate retryable vs non-retryable network/audio errors.

### Logging and Observability

- Use structured logs when possible.
- Include correlation/session IDs in connection flows.
- Log state transitions (connect, hello sent, stream start/stop, reconnect).
- Keep high-frequency audio-frame logs sampled or disabled by default.

### Testing Expectations (for future code)

- Add unit tests for protocol framing and parsing.
- Add integration tests for WebSocket handshake/hello flow.
- Cover error paths (disconnects, invalid frames, timeout handling).
- Keep tests isolated; avoid network dependence unless integration-labeled.
- Ensure single-test execution is documented in project scripts.

### Documentation Expectations

- Update `docs/ai_meeting_hosting_design.md` when architecture changes.
- Keep protocol examples versioned and internally consistent.
- Record assumptions (audio format, frame duration, latency budgets).
- Prefer concise decision records for major tradeoffs.

## PR / Change Checklist for Agents

- Confirm whether toolchain commands exist before running them.
- If no command exists, say so explicitly in updates.
- Keep edits scoped to the task.
- Add or update tests when code exists.
- Update this file when conventions or commands change.

## Ownership Note

This guide is intentionally strict about not fabricating commands.
As the repository evolves from design-only to implementation, treat AGENTS.md as a living contract.
