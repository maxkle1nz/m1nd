# Documentation

## Get a first useful result

- [Signed native 1.6.3 installation](INSTALL-1.6.3.md): exact artifact, signature verification, and explicit binary selection

- [README](../README.md#get-started): what m1nd does, published setup, and source-preview differences
- [Quick Start](wiki/src/tutorials/quickstart.md): connect a host and recover common failures
- [First query](wiki/src/tutorials/first-query.md): orient → find → inspect → remember
- [Agent installation](../llms-install.md): setup for coding agents
- [Host integration matrix](HOST-INTEGRATION-MATRIX.md): exact host configuration and hook behavior

## Choose a workflow

- [Use cases](use-cases.md): unfamiliar code, change preparation, and continuity
- [Examples](../EXAMPLES.md): tool calls and their authority boundaries
- [Deployment](deployment.md): local owners, attached clients, and persistence
- [Agent autonomy](AGENT-AUTONOMY.md): source-preview isolated startup, root identity, and shutdown
- [Host troubleshooting](MCP-HOST-REFRESH.md): missing or stale tool surfaces
- [Contributor development kit](DEVELOPMENT.md): source builds and checkout-private runtime state

## Reference and evidence

- [Wiki source](wiki/src/SUMMARY.md) / [published wiki](https://m1nd.world/wiki/)
- [API overview](wiki/src/api-reference/overview.md): menu versus registry, guidance, and policy limits
- [Architecture](wiki/src/architecture/overview.md): six-crate workspace
- [Evidence guide](wiki/src/benchmarks.md): what measurements do and do not establish
- [Benchmark methodology](benchmarks/README.md)
- [Test portfolio](TEST-PORTFOLIO.md): scoped PR gates and full integration proof
- [Changelog](../CHANGELOG.md) / [published releases](https://github.com/maxkle1nz/m1nd/releases)

## Read status labels before following a document

The current source line is `1.6.5`; publication is separate. PRDs, UML gap ledgers, dated benchmark captures, and release-process notes can describe design intent or historical state. They are not interchangeable with current setup instructions. Consult installed schemas and explicit runtime refusals when a historical example disagrees.

Maintainer-facing design/history belongs under `internal/` where practical. This documentation refresh does not relocate historical records or expose any sealed benchmark material. The public guides above are the entry points for users.
