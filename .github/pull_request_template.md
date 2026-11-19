## What

Brief description of the change.

## Why

Motivation and context.

## Testing

- [ ] `cargo nextest run --workspace --all-targets` and `cargo test --workspace --doc` pass (or `cargo test --workspace --all-targets`)
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo fmt --check` clean
- [ ] `python3 -m unittest discover -s tests` passes
- [ ] `npm test` passes (when npm/skills files change)
- [ ] `cd m1nd-ui && npm ci && npm test && npm run lint && npm run lint:soft && npm run build` passes (when UI changes, with `dist/` committed)
- [ ] Documentation updated if needed
- [ ] New tools registered in `server.rs` dispatch and `action_routes.rs`

## Breaking Changes

List any breaking changes to MCP tool signatures, or write "None".
