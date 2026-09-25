# Rust Module Refactoring

How agents should decompose large Rust modules in this repository without weakening their interfaces or changing behavior accidentally.

For unit-test and integration-test modules, follow `rust-test-refactoring.md` as well. Cargo test targets have distinct crate-root and discovery rules, including an intentional `tests/common/mod.rs` exception to the production layout below.

## Purpose

Refactor a module when its implementation has become difficult to navigate, review, test, or change safely. A source file over roughly 1,000 lines is a strong inspection signal, not an automatic split threshold. A smaller file may need decomposition when it mixes unrelated behavior; a larger cohesive file may not.

The target is a deep module: callers use a small, stable interface while cohesive implementation details remain hidden behind it. Splitting one file into several files is useful only when it improves ownership, dependency direction, and locality of change.

## Module layout

For any non-root module named `<module>`, use a named facade file and a matching directory of private submodules:

```text
src/
├── <module>.rs
└── <module>/
    ├── <responsibility_a>.rs
    ├── <responsibility_b>.rs
    └── <shared_policy>.rs
```

The same rule applies below any directory. For example, `src/domain/order.rs` may own submodules under `src/domain/order/`.

`<module>.rs` is the module entry point and public facade. Prefer this modern layout over `<module>/mod.rs` because named files are easier to identify in editor tabs, search results, diagnostics, and reviews. Never keep both layouts for the same module; Rust treats them as competing definitions.

Crate roots such as `lib.rs` and `main.rs` remain crate roots. Do not rename them to follow this pattern.

## Decide whether and where to split

Before editing, read the whole module, its callers, and its tests. Inventory:

- the public interface, including types, methods, errors, invariants, and ordering constraints;
- major workflows and state transitions;
- private types and helpers;
- external dependencies and side effects;
- tests and the behavior each test protects;
- code that tends to change together.

Propose submodules around cohesive responsibilities. Useful seams often follow:

- a workflow or use case;
- a state lifecycle;
- a policy such as retry, validation, routing, or settlement;
- an adapter to an external system;
- a protocol or serialization concern;
- a group of behavior with a single reason to change.

Do not split mechanically by line ranges or declaration kind. Files named `types.rs`, `errors.rs`, or `constants.rs` are appropriate only when those declarations form a meaningful interface or policy; do not create them merely to make the original file shorter.

Do not create generic dumping grounds such as `utils.rs`, `helpers.rs`, `common.rs`, or `misc.rs`. Keep a helper beside the behavior it serves. Extract shared code only when multiple responsibilities genuinely depend on the same concept, and give that concept a precise name.

Avoid shallow pass-through submodules that expose nearly every implementation detail or merely forward long parameter lists. If two proposed submodules need most of each other's private state, the seam is probably wrong. Reconsider ownership instead of widening visibility in both directions.

## Keep the facade stable

A structural refactor must preserve existing caller paths and semantics unless the task explicitly includes an interface change. If callers currently use:

```rust
use crate::module::PublicType;
```

they should continue to use that path after the split.

Keep small public declarations in the facade when that makes the interface easy to understand. When a public declaration belongs in a child file, re-export it explicitly:

```rust
mod model;

pub use model::{PublicError, PublicRequest, PublicResult};
```

Do not use wildcard re-exports:

```rust
// Avoid: future internal items could become public accidentally.
pub use model::*;
```

The facade should normally contain:

- private `mod` declarations;
- explicit public re-exports;
- module-level documentation;
- the small set of declarations that define the module's interface;
- lightweight construction or delegation needed to connect the interface to its implementation.

Large workflows, detailed policies, adapters, and low-level helpers belong in child modules.

## Control visibility and imports

Keep child modules private unless callers must address them directly. Use the narrowest item visibility that works:

1. private;
2. `pub(super)` for the immediate parent;
3. `pub(crate)` for genuine crate-wide use;
4. `pub` only as part of the intended public interface.

Do not widen visibility merely to make extraction compile. Widespread `pub(crate)` after a split is a sign that the ownership or dependency direction needs another look.

Use explicit imports in production code:

```rust
use crate::module::{PublicError, PublicRequest};
use super::{InternalContext, InternalPolicy};
```

Avoid broad wildcard imports such as `use crate::module::*;` and `use super::*;`. They obscure dependencies and can introduce conflicts as modules grow. `use super::*;` is acceptable in a small test module, and wildcard imports are acceptable from a deliberately designed `prelude`.

## Maintain one-way dependencies

The facade owns the external interface. Workflow submodules may depend on focused policy or adapter submodules. Lower-level submodules must not depend back on higher-level workflows.

A healthy shape is:

```text
public facade
    ├── workflow_a ──┐
    ├── workflow_b ──┼──> shared policy
    └── adapter ─────┘
```

Avoid sibling dependency cycles and large shared context objects created only to move the original file's globals across new file boundaries. Prefer passing the smallest meaningful context or keeping tightly coupled behavior under one owner.

Inherent `impl` blocks may live in child modules when that groups behavior coherently. Do not split one type's methods across many files solely because Rust permits it; each `impl` location should represent a recognizable responsibility.

## Preserve test seams

Treat the module's public interface as the primary test seam.

- Keep integration and caller-facing tests against stable public paths.
- Move private unit tests with the implementation they verify.
- Add tests only when needed to preserve behavior at a newly exposed internal seam.
- Do not expose private implementation solely for tests.
- Do not rewrite tests merely to match the new file layout when their observable behavior is unchanged.

A pure extraction should normally require import and location changes, not changed assertions.

## Refactoring sequence

1. Read the module, callers, tests, and relevant project documentation.
2. Record the current public interface and behavior that must remain stable.
3. Sketch responsibilities and their intended dependency direction.
4. Create the facade and private child modules without changing behavior.
5. Move one responsibility at a time.
6. Compile or check after each meaningful move so errors stay local.
7. Keep imports explicit and reduce any visibility widened during extraction.
8. Relocate private tests and rerun the most relevant tests frequently.
9. Format, run Clippy, and run the full relevant test suite.
10. Review the final diff for interface drift, dependency cycles, and accidental behavior changes.

Prefer a pure structural refactor. Do not combine extraction with unrelated renaming, algorithm changes, error-contract changes, or feature work. If a behavior change is necessary, isolate it as a separate, explicitly tested step.

## Completion checklist

- Existing public paths, errors, invariants, and semantics are unchanged.
- The facade clearly communicates a small interface.
- Each child module owns one recognizable responsibility.
- Dependencies flow in one direction without sibling cycles.
- Imports and re-exports are explicit.
- Visibility is no broader than necessary.
- Tests still exercise stable interfaces.
- No `mod.rs`, production wildcard imports, or generic helper dumping grounds were introduced.
- Formatting, Clippy, relevant tests, and the final test suite pass.
- The diff is structural, or any necessary behavior change is isolated and tested.
