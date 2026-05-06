# LadybugDB Migration Agent Guide

Date: 2026-05-06

## Purpose

This document guides an implementation agent through migrating llmparty from the current Vela Kuzu fork to LadybugDB.

The goal is to replace the vendored Kuzu fork with LadybugDB while keeping the application graph feature behavior unchanged.

## Current State

The project currently uses Vela's Kuzu fork as an optional Rust dependency:

```toml
# Cargo.toml
kuzu = { path = "vendor/vela-kuzu/tools/rust_api", default-features = false, optional = true }

[features]
kuzu = ["dep:kuzu"]
```

The submodule is configured as:

```ini
# .gitmodules
[submodule "vendor/vela-kuzu"]
	path = vendor/vela-kuzu
	url = https://github.com/Vela-Engineering/kuzu.git
```

Current checked version:

- Repository: `https://github.com/Vela-Engineering/kuzu.git`
- Commit: `57312c09ac0af27bb8e8bed930576f4ff1ceb36d`
- Tag: `v0.12.0-vela.55cd2b4`

Application usage is mostly in `src/application/graph.rs` behind `#[cfg(feature = "kuzu")]`.

The existing feature name is `kuzu`. Keep this feature name unless there is a strong reason to rename it; preserving it avoids unnecessary changes to tests, CI, and caller expectations.

## Target State

Use LadybugDB as the graph engine.

Preferred target repositories:

- Core engine: `https://github.com/LadybugDB/ladybug`
- Rust binding: `https://github.com/ladybugdb/ladybug-rust`

LadybugDB's Rust crate is named `lbug`, not `kuzu`. To minimize application code changes, prefer Cargo dependency rename:

```toml
kuzu = { package = "lbug", path = "vendor/ladybug-rust", default-features = false, optional = true }
```

With this rename, existing Rust code can continue to use `kuzu::Database`, `kuzu::Connection`, and `kuzu::Value` while the actual package is Ladybug's `lbug` crate.

## Why LadybugDB

LadybugDB appears to be the healthier long-term Kuzu successor:

- Active releases through `v0.16.1`.
- Larger community activity than the Vela fork.
- It explicitly continues the archived Kuzu project.
- It has language bindings under the Ladybug naming scheme.
- It has incorporated Vela's concurrent checkpoint / multi-writer work.

Important: Ladybug has ported Vela's concurrent checkpoint implementation. The merged Ladybug PR #371 states that it ports `Vela-Engineering/kuzu` concurrent multi-writer checkpoint behavior into Ladybug.

## Non-Goals

Do not expand graph product scope during this migration.

Specifically, do not:

- Change the public graph API shape.
- Add new graph node or relationship types.
- Rename the app feature from `kuzu` unless required.
- Change the default graph directory unless migration testing proves it is necessary.
- Make graph projection part of the critical write path.

## Recommended Migration Strategy

Use a two-phase approach.

### Phase 1: Compatibility Spike

Perform the compatibility spike directly on the current main-branch working tree. Do not create a separate branch or worktree unless the user explicitly asks for one later.

Verify that Ladybug can replace Vela without broad application changes.

Expected spike outcomes:

1. Project compiles with `cargo test --features kuzu`.
2. Existing graph tests pass.
3. A new or manual smoke test confirms a Ladybug-backed database can be created, written, queried, closed, and reopened.
4. Existing Vela-created test data either opens successfully or the migration documents that graph projection data must be rebuilt.

Do not delete the Vela submodule until the spike is successful.

### Phase 2: Formal Replacement

After the spike passes, update the repository permanently:

1. Replace vendored source.
2. Update Cargo dependency path and package rename.
3. Update `.gitmodules`.
4. Update lockfile using Cargo, not manual lockfile editing.
5. Run the full verification checklist.
6. Remove stale Vela references where appropriate.

## Detailed Steps

### 1. Confirm main-branch working tree

Work directly on the current main branch. Do not run `git checkout -b`, do not create a worktree, and do not move the task to a separate branch unless the user explicitly changes this instruction.

```bash
git branch --show-current
```

If the current branch is not `main`, stop and ask the user before switching branches.

### 2. Inspect current working tree

```bash
git status --short
git submodule status --recursive
```

Do not proceed if unrelated user changes are present unless they are clearly safe to keep.

### 3. Add Ladybug Rust binding

Preferred vendor layout:

```text
vendor/ladybug-rust
```

The Rust binding repository is separate from the core Ladybug repository:

```bash
git submodule add https://github.com/ladybugdb/ladybug-rust.git vendor/ladybug-rust
```

Then check out a release-compatible tag or commit.

Preferred starting point:

```bash
git -C vendor/ladybug-rust fetch --tags
git -C vendor/ladybug-rust checkout v0.16.0
```

If a newer matching `ladybug-rust` tag exists for the Ladybug core release in use, prefer that matching tag.

Note: The Ladybug main repo's latest release may be `v0.16.1`, while the Rust crate may report `0.16.0`. Verify available tags before pinning.

### 4. Decide whether to vendor core Ladybug separately

`ladybug-rust` can build using its own bundled/source layout or precompiled libraries depending on its build script and environment.

Start with `vendor/ladybug-rust` only. Add `vendor/ladybug` only if the Rust binding requires a local Ladybug source checkout or if reproducibility requirements demand vendoring the core engine.

If adding core source is required:

```bash
git submodule add https://github.com/LadybugDB/ladybug.git vendor/ladybug
git -C vendor/ladybug fetch --tags
git -C vendor/ladybug checkout v0.16.1
```

Then configure the build according to `ladybug-rust` documentation, likely using environment variables such as `LBUG_SOURCE_DIR` if needed.

### 5. Update Cargo.toml

Change the dependency from Vela Kuzu to Ladybug's `lbug` package while preserving the local crate name `kuzu`:

```toml
kuzu = { package = "lbug", path = "vendor/ladybug-rust", default-features = false, optional = true }
```

Keep the feature stanza unchanged:

```toml
[features]
kuzu = ["dep:kuzu"]
```

This minimizes source changes because current application code imports the dependency as `kuzu`.

### 6. Update lockfile via Cargo

Run:

```bash
cargo update -p kuzu || true
cargo test --no-run --features kuzu
```

If Cargo identifies the dependency by package name `lbug`, use:

```bash
cargo update -p lbug
```

Do not manually edit `Cargo.lock` except as a last resort. Cargo should update it.

### 7. Compile without source changes first

Run:

```bash
cargo check --features kuzu
```

Expected likely issues:

- Type names may still match because Ladybug Rust binding is derived from Kuzu's Rust binding.
- Error strings may differ.
- Some enum variants or API functions may differ between Kuzu `0.11/0.12` and Ladybug `0.16`.
- Build scripts may require system tools or local source paths.

Fix only compatibility issues required to compile and pass tests.

### 8. Enable multi-writes explicitly if needed

Current code opens the database with:

```rust
kuzu::Database::new(db_dir, kuzu::SystemConfig::default())
```

Ladybug Rust binding exposes:

```rust
SystemConfig::enable_multi_writes(true)
```

If the application expects concurrent graph projection writes, update `open_graph_connection()` to use:

```rust
let config = kuzu::SystemConfig::default().enable_multi_writes(true);
let db = kuzu::Database::new(db_dir, config)
```

However, first verify that this method exists in the pinned `lbug` version. If it does not, do not invent a wrapper; either upgrade the binding or document that concurrent multi-writes are not enabled through Rust config in this version.

### 9. Revisit database lifecycle

Current `open_graph_connection()` leaks a new `Database` for each call so that it can return a `Connection<'static>`:

```rust
let db = Box::leak(Box::new(db));
kuzu::Connection::new(db)
```

This was acceptable for the first Kuzu seed but is not ideal for long-running processes or concurrent writes.

For this migration, keep behavior unchanged unless tests reveal a problem. If changing lifecycle is necessary, make it a separate, focused refactor:

- Introduce a shared graph database handle.
- Return short-lived connections tied to that handle.
- Avoid opening multiple read-write database instances for the same path in the same process.

Do not mix a large lifecycle refactor with the package migration unless required for correctness.

### 10. Update tests

Run the current test suite first:

```bash
cargo test
cargo test --features kuzu
```

If build time is high, at minimum run:

```bash
cargo test --features kuzu graph
cargo test --features kuzu global_workspace_tasks
cargo test --features kuzu config
```

Then add or adjust tests only if needed:

- Ladybug open/write/query smoke test.
- Reopen persisted graph database test.
- Optional multi-write smoke test if enabling `enable_multi_writes(true)`.

Avoid brittle assertions on exact database error wording unless the user-facing behavior depends on it.

### 11. Verify storage compatibility

Graph data is currently derived projection data, not the system of record. SQLite remains the authoritative store.

Default graph location:

```text
<llmparty_data_dir>/graph/kuzu
```

Test these cases:

1. Empty directory: Ladybug creates a new database.
2. Existing Ladybug directory: reopen succeeds.
3. Existing Vela Kuzu directory: attempt open.

If Vela-created data cannot be opened by Ladybug, choose the safe migration path:

- Delete/rebuild only the graph projection directory, not SQLite data.
- Document that graph projection is disposable and can be regenerated from SQLite facts.
- Do not delete user data automatically without explicit confirmation or a safe backup/rename strategy.

Recommended safe behavior for incompatible graph directories:

```text
rename graph/kuzu -> graph/kuzu.pre-ladybug.<timestamp>
create new graph/kuzu
reproject from SQLite when projection tooling supports it
```

### 12. Update docs and naming references

Update repository references:

- `.gitmodules`
- `Cargo.toml`
- Any migration notes in `spec/`

Keep user-facing config names stable unless changing them is necessary.

In particular, the default path `graph/kuzu` can remain unchanged initially to avoid breaking config expectations. A later cleanup can rename it to `graph/ladybug` with a proper migration path.

### 13. Remove Vela submodule after success

Only after tests pass:

```bash
git submodule deinit -f vendor/vela-kuzu
git rm -f vendor/vela-kuzu
rm -rf .git/modules/vendor/vela-kuzu
```

Be careful: removing `.git/modules/...` is destructive to the submodule metadata. Do this only after `git rm` succeeds and the current main-branch working tree is in a recoverable state.

If uncertain, leave the old submodule in place during the first main-branch pass and remove it in a follow-up cleanup.

## Verification Checklist

Before claiming completion, collect command output for:

```bash
git status --short
git submodule status --recursive
cargo check
cargo check --features kuzu
cargo test
cargo test --features kuzu
```

If any command is skipped, document why.

Also verify:

- `Cargo.toml` points to Ladybug's `lbug` package.
- `.gitmodules` no longer points the active graph dependency at Vela.
- `Cargo.lock` contains `lbug` rather than the old local `kuzu` package, unless dependency rename causes both names to appear in a justifiable way.
- The app still compiles without the `kuzu` feature.
- The app still compiles with the `kuzu` feature.
- Existing graph projection tests pass.

## Rollback Plan

If Ladybug migration fails during spike:

1. Revert `Cargo.toml` dependency to:

   ```toml
   kuzu = { path = "vendor/vela-kuzu/tools/rust_api", default-features = false, optional = true }
   ```

2. Restore `.gitmodules` Vela entry if changed.
3. Run:

   ```bash
   git submodule update --init --recursive
   cargo check --features kuzu
   ```

4. Document the failure cause in this guide, a follow-up note, or an issue.

Do not delete or overwrite existing graph data as part of rollback.

## Common Pitfalls

### Pitfall: manually renaming every `kuzu::` reference

Avoid this initially. Use Cargo dependency rename so the code can keep using `kuzu::`.

### Pitfall: treating graph projection as source of truth

Do not. SQLite facts are authoritative. The graph database is a projection target and can be rebuilt.

### Pitfall: assuming storage compatibility

Do not assume Vela `0.12.0-vela` data opens in Ladybug `0.16.x`. Test it.

### Pitfall: enabling multi-write without testing

If `enable_multi_writes(true)` is enabled, add at least a smoke test or manual verification. Concurrent write behavior is the exact area where database bugs can be costly.

### Pitfall: mixing migration with large refactors

Keep this migration focused. Database lifecycle cleanup, path rename from `graph/kuzu` to `graph/ladybug`, and re-projection tooling can be follow-up tasks.

## Suggested Commit Structure

Prefer small commits:

1. `docs: add ladybug migration guide`
2. `chore: vendor ladybug rust binding`
3. `chore: switch graph dependency to ladybug`
4. `fix: adapt graph projection to ladybug api`
5. `test: add ladybug graph smoke coverage`
6. `chore: remove vela kuzu vendor`

## Success Criteria

Migration is complete when:

- The active optional graph dependency is LadybugDB's Rust binding.
- Existing application graph behavior is unchanged.
- Tests pass with and without the `kuzu` feature.
- The migration path for existing graph directories is documented.
- The old Vela fork is no longer required to build the project.
