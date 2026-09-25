# Rust Test Refactoring

Required rules for agents that decompose large Rust test modules in this repository.

## Purpose

Refactor a test module when its size or mixed responsibilities make scenarios difficult to find, run, review, or maintain. A file over roughly 1,000 lines is a strong inspection signal, not an automatic split threshold.

The goal is to make behavior and failure ownership visible without changing the test boundary, weakening assertions, hiding test discovery, or creating unnecessary integration-test crates.

## Identify the test kind first

Determine whether the large file contains unit tests or an integration-test target before choosing a layout.

- Unit tests live beside production code under `#[cfg(test)]` and may access private implementation.
- Integration tests live under `tests/`, compile as external crates, and should exercise the library's public interface.

Do not expand a production interface merely to let an integration test reach private implementation. Keep implementation-focused tests as unit tests and caller-facing behavior tests as integration tests.

## Large unit-test modules

For unit tests attached to a production module named `<module>`, use a test facade and behavior modules:

```text
src/
├── <module>.rs
└── <module>/
    ├── tests.rs
    └── tests/
        ├── <behavior_a>.rs
        └── <behavior_b>.rs
```

Declare the test facade from `<module>.rs`:

```rust
#[cfg(test)]
mod tests;
```

Keep `tests.rs` small. It should declare behavior modules and contain only fixtures genuinely shared across those modules. Prefer tests against the production module's interface; direct private-item testing is allowed when it protects focused implementation behavior.

## Large integration-test targets

Every Rust file directly under `tests/` is a separate integration-test crate and executable. Do not create more top-level `tests/*.rs` files merely to shorten an existing file. A new top-level test target is appropriate only when it represents a real crate or execution boundary with independent configuration, isolation, or selection needs.

For a multi-file integration-test target named `<target>`, use Cargo's directory layout:

```text
tests/
└── <target>/
    ├── main.rs
    ├── fixture.rs
    ├── <behavior_a>.rs
    └── <behavior_b>.rs
```

The directory name remains the Cargo test-target name:

```bash
cargo test --test <target>
```

Move `tests/<target>.rs` to `tests/<target>/main.rs` as one structural change. Do not keep both forms: they define competing targets with the same inferred name.

`main.rs` is the test crate root and should normally contain only module declarations:

```rust
mod fixture;
mod behavior_a;
mod behavior_b;
```

Keep `#[test]` and `#[tokio::test]` functions in the behavior modules. Do not replace independently discoverable tests with one root test that calls a sequence of `run()` functions.

### Minimal-migration alternative

When moving the crate root would create unacceptable compatibility risk, keep `tests/<target>.rs` temporarily and load namespaced files explicitly:

```rust
#[path = "<target>/behavior_a.rs"]
mod behavior_a;
```

This is a migration technique, not the preferred long-term layout. A file directly under `tests/` is a crate root, so plain `mod behavior_a;` would search under `tests/`, not automatically under `tests/<target>/`.

## Choose test seams by behavior

Group tests by the behavior they protect and the reason they would fail. Useful seams include:

- a protocol or endpoint;
- a workflow or use case;
- a state lifecycle;
- success, rejection, retry, or termination semantics;
- persistence and recovery behavior;
- an external adapter contract;
- authorization or isolation rules.

Do not split tests mechanically by line range, production source filename, declaration kind, or one file per test. Each behavior module should have a name that explains what its tests protect and should be independently runnable through a test-name filter.

Long test names containing unrelated clauses joined by `and` often indicate multiple behavior contracts. Split them when each contract can have independent setup and failure reporting. Keep them together when they intentionally verify one atomic contract or when shared expensive setup is part of the behavior under test.

## Fixture and test-double ownership

Keep support code at the narrowest scope that uses it:

1. one test: keep it in that test or behavior module;
2. several tests in one behavior module: keep it in that module;
3. several behavior modules in one target: move it to a precisely named target-local module such as `fixture.rs`, `providers.rs`, or `assertions.rs`;
4. several integration-test targets: move it to `tests/common/mod.rs` only after genuine reuse exists.

`tests/common/mod.rs` is an intentional exception to the production rule that avoids `mod.rs`. Cargo does not treat this nested file as a standalone integration-test target. Do not replace it with `tests/common.rs`, because a top-level `.rs` file is discovered as another target.

When a directory-based target needs the repository's shared `tests/common/mod.rs`, import it explicitly from `main.rs`:

```rust
#[path = "../common/mod.rs"]
mod common;
```

Do not let `fixture.rs`, `helpers.rs`, or a giant context object become a dumping ground. Prefer precise names and cohesive facilities:

- builders construct meaningful inputs;
- test doubles model one external role or controlled behavior;
- assertion helpers express a domain contract;
- RAII guards own cleanup and service lifetimes;
- scenario-specific setup stays beside the scenario.

Avoid hidden actions or broad assertions inside generic setup helpers. A reader should still be able to identify arrange, act, and assert from the test body.

## Visibility and imports

Test modules and test functions do not need to be public. For target-local support, use the narrowest visibility that compiles, usually private or `pub(super)`.

Do not build a public re-export facade for test helpers. Scenario modules should import their dependencies explicitly so fixture coupling is visible. `use super::*;` is acceptable for a small unit-test module close to its production implementation; prefer explicit imports in large integration-test modules.

## Preserve discovery and execution semantics

A structural test refactor must preserve:

- the Cargo test-target name;
- the number of discovered tests;
- each test's sync or async harness attribute;
- `#[ignore]`, serialization, feature, and platform attributes;
- environment-dependent skip behavior;
- timeouts and paused-time behavior;
- unique database, cache, bucket, and key-prefix isolation;
- setup, migration, teardown, and cleanup ordering;
- assertions and externally observable behavior.

Moving a test into a child module changes its fully qualified libtest name by adding the module path. Search scripts, CI configuration, documentation, and developer tooling for exact test-name filters before moving it. Preserve the target name and update intentional exact filters when necessary.

Do not assume file separation creates process isolation. Child modules in one integration-test target share the same test executable. Conversely, new top-level targets create new executables and affect compilation and execution behavior.

## Table-driven tests and shared setup

Use a case table or loop only when all cases share the same setup, action, and assertion shape. Include a descriptive case name in failure messages. Do not collapse behaviorally distinct tests merely to reduce lines; doing so weakens test discovery and failure localization.

Expensive Docker-backed or end-to-end setup may justify checking several closely related cases in one test. Keep the cases explicitly labeled and ensure a failure identifies the case. Do not use setup cost as a reason to combine unrelated contracts.

## Refactoring sequence

1. Read the whole test file, production interface, shared test support, and test-running scripts.
2. Record the current target name and discovered tests:

   ```bash
   cargo test -p <package> --test <target> -- --list
   ```

3. Inventory behavior groups, fixtures, test doubles, external resources, attributes, and exact-name filters.
4. Choose unit-test or integration-test layout and sketch dependency direction.
5. Create the new crate root or test facade and move one behavior group at a time.
6. Keep single-use fixtures local; extract shared support only after multiple modules need it.
7. Compile and run the moved module through a test-name filter after every meaningful move.
8. Compare the final `--list` output with the original inventory, accounting only for intended module-path prefixes.
9. Run the complete test target, formatting, and Clippy.
10. Run the repository's relevant integration or release-gate command when required by the change risk.
11. Review the diff for lost tests, weakened assertions, widened visibility, changed isolation, and accidental behavior changes.

Prefer a pure structural refactor first. Do not simultaneously redesign fixtures, rewrite assertions, alter timeouts, or change production behavior. Those improvements may follow as separate, explicitly verified changes.

## Completion checklist

- The test kind and target boundary are explicit.
- The Cargo target name and test count are preserved.
- Behavior modules have meaningful, filterable names.
- Independently discoverable tests remain independently discoverable.
- Fixtures and test doubles live at the narrowest useful scope.
- Cross-target support is limited to genuine reuse in `tests/common/mod.rs`.
- Imports are explicit and visibility is minimal.
- Attributes, skip behavior, isolation, cleanup, timeouts, and assertions are unchanged.
- No unnecessary top-level test target or generic helper dumping ground was introduced.
- `--list`, focused tests, the full target, formatting, and Clippy pass.
- Any unexecuted environment-dependent verification is reported explicitly.
