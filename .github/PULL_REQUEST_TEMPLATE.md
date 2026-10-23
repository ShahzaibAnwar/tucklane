## What does this change?

## Why?

## How was it tested?

- [ ] `npm run lint && npm run typecheck && npm test`
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` (in `src-tauri`)
- [ ] Tried in the app on Windows using a **scratch folder** (not my real Downloads)

## Safety checklist

- [ ] All filesystem access goes through `AuthorizedRootGuard`; no new frontend permissions
- [ ] Nothing can overwrite or permanently delete a user file
- [ ] No placeholder data or non-functional UI
- [ ] Docs / CHANGELOG updated if behavior changed
