# Docs guide

This repo keeps the public docs surface intentionally small.

## Start here
- `../README.md` — primary project overview and quick start
- `AGENT-FIRST-DEMO.md` — shortest real agent-first demo transcript path
- `AGENT-PACKS.md` — installable m1nd-first skills and portable agent packs
- `MCP-HOST-REFRESH.md` — diagnose stale or partial MCP host tool surfaces
- `deployment.md` — persistent runtime and production setup
- `IDE-INTEGRATIONS.md` — client and integration notes
- `use-cases.md` — audience-oriented product workflows
- `benchmarks/README.md` — benchmark scenarios, runs, and methodology
- `wiki/` — canonical mdBook source for the public docs site

## Maintainer-facing docs
- `internal/` — design notes, visual reviews, hardening reports, and workflow docs
- `AGENT-TASKNOTES.md` — running capture of real agent friction worth turning into improvements

## Public docs policy
If a doc is primarily:
- user-facing
- setup-facing
- benchmark-facing
- or canonical product reference

it should stay in the public docs surface.

If it is primarily:
- exploratory
- historical
- design-internal
- release-process-specific
- or maintainer-only

it should live under `docs/internal/`.
