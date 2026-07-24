# Dashboard message edit replay verification

Date: 2026-07-24

Result: passed

## Environment

- Pontia was built from commit `146afb2`.
- Dashboard actions were driven through the production build in headless Chromium.
- The runtime used tmux 3.7b and Pi 0.82.0 with only the current checkout's
  Pi extension loaded.
- Pi used `gpt-5.6-sol` for short deterministic replies.
- The exercise used an isolated Pontia home, database, workspace, port, tmux
  session, and browser profile. The locally running stable Pontia server was
  not modified.

## Evidence

The authoritative External API projection began with this native branch:

```text
CLEAN_ALPHA (root)
└── CLEAN_BETA
    └── CLEAN_GAMMA
```

Editing `CLEAN_BETA` from the Dashboard to `CLEAN_BETA_DASH` produced:

```text
CLEAN_ALPHA (root)
├── CLEAN_BETA
│   └── CLEAN_GAMMA
└── CLEAN_BETA_DASH
```

- The Dashboard submitted a branch-targeted Inbox Message.
- Instrumentation at the tmux `load-buffer` boundary captured exactly
  `/pontia-edit <inbox-message-id>`; the replacement text was not sent through
  tmux.
- Immediately after dispatch, the Inbox Message was `dispatched` with a null
  `turn_id`. The three existing Turns were unchanged.
- Pi displayed native navigation and submitted `CLEAN_BETA_DASH` as a normal
  user message.
- Existing callbacks reported `turn.started`, `turn.output`, and
  `turn.completed`. The replacement lifecycle events were sourced from the
  agent adapter/client path, not from command dispatch.
- The projected replacement Turn had `CLEAN_ALPHA` as its parent. The original
  `CLEAN_BETA -> CLEAN_GAMMA` suffix remained available through the Turn list
  and direct Turn queries.
- The Dashboard converged to `CLEAN_ALPHA -> CLEAN_BETA_DASH` and continued to
  show the divergent historical branch in its branch summary.

The remaining semantics were checked in the same real Pi Session:

| Operation | Replacement parent | Result |
| --- | --- | --- |
| Dashboard root edit of `CLEAN_ALPHA` | none | new root sibling |
| Dashboard resend of the latest root Turn | none | alternative root sibling |
| Manual Pi `/tree` edit of `CLEAN_BETA` | `CLEAN_ALPHA` | sibling of `CLEAN_BETA` |

The manual edit produced the same parent and projected branch shape as the
Dashboard middle edit. Its callback events also drove the Dashboard to the new
branch without a Dashboard-specific lifecycle.

## Automated checks

Targeted checks passed:

- Backend branch replay API tests: 2 passed.
- Pi client tests: 70 passed; TypeScript typecheck passed.
- Dashboard message-edit and projected-tree tests: 65 passed.
- Dashboard Svelte and TypeScript checks passed with no diagnostics.

Final repository verification also passed:

- `just check`
- `pnpm --dir apps/dashboard run build`
- `pnpm --dir clients/pi test`
- `pnpm --dir clients/pi typecheck`

## Residual risks

- This was one local run against Pi 0.82.0 and one available model/provider.
  Other Pi versions and providers were not exercised.
- Headless Chromium drove the real rendered controls, but visual layout and
  pointer behavior were not assessed.
- Tmux capture and the Pi screen were used only as command-path diagnostics.
  Lifecycle and topology conclusions came from External API projections and
  callback-sourced events, in accordance with ADR-0001.
