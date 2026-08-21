# M15 Rust Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `yanc build` that lowers every M2-M13 `VerifiedProgram` to a fixed Cargo project, invokes Cargo, and publishes an executable whose output equals the MIR interpreter.

**Architecture:** `yan-rust-backend` consumes only `yan_mir::VerifiedProgram` and returns generated Rust text plus a fixed package layout. `yan-runtime` owns the generated program's closed value representation and `console.println` intrinsic. `yanc` owns files, deterministic `target/yan/<entry-hash>` directories, Cargo invocation, binary publishing, and stable Yan diagnostics.

**Tech Stack:** Rust 2021 workspace; standard library only; Cargo invoked as an external child process; no third-party dependencies.

---

## File Structure

- Create: `crates/yan-runtime/{Cargo.toml,src/lib.rs}` - fixed generated-program runtime, never a Yan-facing API.
- Create: `crates/yan-rust-backend/{Cargo.toml,src/lib.rs}` - Verified MIR to Rust source and fixed package manifest generation.
- Modify: `Cargo.toml` - add only the two owned workspace crates.
- Modify: `crates/yanc/{Cargo.toml,src/main.rs}` - add build CLI, reserved namespace, build directories, Cargo execution, output and diagnostics.
- Modify: `docs/yan-language-design.md` - add the exact build usage, output and exit-code contract before code accepts the command.
- Modify: `docs/milestones/m15-rust-backend.md` - update checked status only from fresh final evidence.
- Test: crate-local `#[cfg(test)]` modules and `crates/yanc/src/main.rs` CLI integration tests; no generated output is checked in.

### Task 1: Fix the Public Build Contract and Workspace Boundaries

**Files:**
- Modify: `docs/yan-language-design.md`
- Modify: `Cargo.toml`
- Create: `crates/yan-runtime/Cargo.toml`
- Create: `crates/yan-runtime/src/lib.rs`
- Create: `crates/yan-rust-backend/Cargo.toml`
- Create: `crates/yan-rust-backend/src/lib.rs`

- [ ] **Step 1: Write the failing CLI contract tests in `crates/yanc/src/main.rs`**

```rust
#[test]
fn build_help_and_missing_argument_follow_the_cli_contract() {
    assert_eq!(USAGE, "Usage:\n  yanc check <file.yan>\n  yanc run <file.yan>\n  yanc build <file.yan>\n  yanc --help");
}
```

Add a test-only command dispatcher returning an exit code and captured output so `build` without a path is asserted to write this usage to stderr and exit `2`.

- [ ] **Step 2: Run the focused test and record the missing build usage failure**

Run: `cargo test -p yanc build_help_and_missing_argument_follow_the_cli_contract`

Expected: FAIL because `USAGE` and the dispatcher do not contain `build`.

- [ ] **Step 3: Add the two crate manifests and empty public boundaries**

```toml
# crates/yan-rust-backend/Cargo.toml
[dependencies]
yan-mir = { path = "../yan-mir" }
yan-runtime = { path = "../yan-runtime" }
```

```rust
/// 将已验证 MIR 生成固定 Rust 构建单元的入口。
pub fn generate(program: &yan_mir::VerifiedProgram) -> Result<GeneratedProgram, BackendError> {
    let _ = program;
    Err(BackendError::UnsupportedProgram)
}
```

Add both crates to `[workspace].members`. `yan-runtime` must have no dependencies; `yan-rust-backend` must not depend on `yan-hir`, `yan-typeck`, `yan-syntax` or `yanc`.

- [ ] **Step 4: Document and implement the exact build usage contract**

Add the `build` usage and these rules to `docs/yan-language-design.md`: success is `<path>: build succeeded: <binary-path>` with exit `0`; invalid arguments use stderr usage and exit `2`; Cargo/link failure is exactly `error: <entry-path>:1:1: backend build failed`. Update `USAGE` without changing check/run behavior.

- [ ] **Step 5: Run boundary checks**

Run: `cargo test -p yanc build_help_and_missing_argument_follow_the_cli_contract`

Expected: PASS.

Run: `cargo test -p yan-runtime; cargo test -p yan-rust-backend`

Expected: PASS with zero tests until the next task adds coverage.

- [ ] **Step 6: Commit the contract boundary**

Run: `git add Cargo.toml docs/yan-language-design.md crates/yan-runtime crates/yan-rust-backend crates/yanc/src/main.rs; git commit -m "feat(cli): 定义原生构建命令契约"`

### Task 2: Implement the Closed Runtime Value Model

**Files:**
- Modify: `crates/yan-runtime/src/lib.rs`
- Test: `crates/yan-runtime/src/lib.rs`

- [ ] **Step 1: Write failing runtime behavior tests**

```rust
#[test]
fn displays_collections_and_result_values_like_the_mir_interpreter() {
    assert_eq!(Value::list(vec![Value::int(1), Value::int(2)]).display(), "[1, 2]");
    assert_eq!(Value::ok(Value::string("Yan")).display(), "Ok(Yan)");
}

#[test]
fn integer_addition_reports_overflow_without_panicking() {
    assert_eq!(Value::int(i64::MAX).add(Value::int(1)), Err(RuntimeError::IntegerOverflow));
}
```

- [ ] **Step 2: Run the runtime tests**

Run: `cargo test -p yan-runtime`

Expected: FAIL because `Value` and `RuntimeError` do not exist.

- [ ] **Step 3: Implement the non-public generated-program ABI**

Define documented `pub enum Value` for `int`, `float`, `bool`, `string`, `bytes`, `List`, `Map`, tuple, `Option`, `Result`, struct fields keyed by numeric IDs and enum variants keyed by numeric IDs. Add checked integer add/multiply, equality, display, tuple/field access, list iterator state, hex decode, string-to-int, and `console_println(&Value)`. Return `RuntimeError` values instead of panicking; generated `main` maps them to a controlled process failure.

- [ ] **Step 4: Run the runtime suite**

Run: `cargo test -p yan-runtime`

Expected: PASS, including display, arithmetic overflow, constructors, fields, tuple, iterator, bytes and string conversion tests.

- [ ] **Step 5: Commit the runtime**

Run: `git add crates/yan-runtime; git commit -m "feat(runtime): 提供受控 Yan 值运行时"`

### Task 3: Generate Straight-Line Verified MIR

**Files:**
- Modify: `crates/yan-rust-backend/src/lib.rs`
- Modify: `crates/yan-rust-backend/Cargo.toml`
- Test: `crates/yan-rust-backend/src/lib.rs`

- [ ] **Step 1: Write failing generation tests from verified fixtures**

```rust
#[test]
fn generates_values_locals_calls_and_aggregates_from_verified_mir() {
    let source = generate(verified_fixture("fn main() -> unit { let values = [1, 2] console.println(values) }")).unwrap();
    assert!(source.main_rs.contains("yan_runtime::Value::list"));
    assert!(source.main_rs.contains("yan_runtime::console_println"));
}
```

Also assert `generate` cannot be called with `yan_mir::Program`: its parameter type is `&VerifiedProgram`.

- [ ] **Step 2: Run the focused backend test**

Run: `cargo test -p yan-rust-backend generates_values_locals_calls_and_aggregates_from_verified_mir`

Expected: FAIL because `GeneratedProgram` has no source renderer.

- [ ] **Step 3: Implement deterministic symbol and operand rendering**

Create private renderers for `FunctionId`, `LocalId`, `ValueId`, `FieldId` and `VariantId` that use only numeric IDs. Render constants, `Assign`, `StoreLocal`, `Binary`, string/list/map/tuple/struct construction, tuple/field loads, `Call`, and `Phi` into explicit `yan_runtime::Value` operations. Map `CallTarget::{Some, Ok, Err, BytesFromHex, ConsolePrintln, StringToInt, Newtype, Variant, Function}` without source-name lookup.

- [ ] **Step 4: Run backend unit tests**

Run: `cargo test -p yan-rust-backend`

Expected: PASS; generated text contains no source identifier except the fixed entry symbol and no user Cargo dependency.

- [ ] **Step 5: Commit straight-line lowering**

Run: `git add crates/yan-rust-backend; git commit -m "feat(backend): 生成顺序 MIR Rust 代码"`

### Task 4: Generate CFG, Match, Loop and Result Propagation

**Files:**
- Modify: `crates/yan-rust-backend/src/lib.rs`
- Test: `crates/yan-rust-backend/src/lib.rs`

- [ ] **Step 1: Write failing control-flow generation tests**

```rust
#[test]
fn generates_all_m15_control_flow_terminators() {
    let generated = generate(verified_fixture("fn unwrap(value: Result<int, unit>) -> Result<int, unit> { let item = value? Ok(item) } fn main() -> unit { }")).unwrap();
    assert!(generated.main_rs.contains("match block_id"));
    assert!(generated.main_rs.contains("return Err("));
}
```

Add fixtures and assertions for `Branch`, `Match`, `Goto`, `Return`, `IterInit`/`IterNext`, and Phi predecessor selection.

- [ ] **Step 2: Run the test**

Run: `cargo test -p yan-rust-backend generates_all_m15_control_flow_terminators`

Expected: FAIL because terminators are not rendered.

- [ ] **Step 3: Render each MIR function as a block-state loop**

Generate `let mut block_id: u32 = 0`, predecessor state, local slots and temporary slots. Emit a `loop { match block_id { ... } }` where each block renders instructions then exactly one terminator. `Match` extracts bindings through runtime APIs; `PropagateErr` returns the original error from the generated function; `IterNext` follows the existing List-only semantics. Phi selects only the verified predecessor value. Do not introduce unsafe code, Rust recursion or Rust pattern semantics as a substitute for MIR IDs.

- [ ] **Step 4: Run all backend tests**

Run: `cargo test -p yan-rust-backend`

Expected: PASS for if, match, for, return, Result propagation, nested calls and mutation ordering.

- [ ] **Step 5: Commit CFG lowering**

Run: `git add crates/yan-rust-backend/src/lib.rs; git commit -m "feat(backend): 生成验证控制流图"`

### Task 5: Materialize the Fixed Cargo Project and Execute Cargo

**Files:**
- Modify: `crates/yan-rust-backend/src/lib.rs`
- Modify: `crates/yanc/Cargo.toml`
- Modify: `crates/yanc/src/main.rs`
- Test: `crates/yanc/src/main.rs`

- [ ] **Step 1: Write failing build-directory tests**

```rust
#[test]
fn build_writes_only_a_deterministic_owned_cargo_project() {
    let output = temporary_entry("fn main() -> unit { console.println(\"Yan\") }");
    let result = build_command(&output);
    assert_eq!(result.exit, ExitCode::SUCCESS);
    assert!(result.stdout.contains(": build succeeded: "));
    assert!(result.binary_path.is_file());
}
```

Use a test-only temporary directory. Assert the generated manifest pins only a path dependency on the workspace `yan-runtime`, and that the project root is under `target/yan/<stable-hash>/cargo`.

- [ ] **Step 2: Run the test**

Run: `cargo test -p yanc build_writes_only_a_deterministic_owned_cargo_project`

Expected: FAIL because `build_command` does not exist.

- [ ] **Step 3: Implement atomic materialization and Cargo invocation**

Add `yan-rust-backend` as a `yanc` dependency. Compute a deterministic hash from canonical entry path and source text using a standard-library hasher encoded as lowercase hex. Replace only `target/yan/<hash>/cargo` after generation succeeds; use `std::process::Command::new("cargo")` with fixed `build --quiet --manifest-path <generated/Cargo.toml>`. On success copy the compiled binary to `target/yan/<hash>/bin/<entry-name>[.exe]` and print the specified success line. On nonzero status or process start failure render `backend build failed` at the entry `1:1`; never forward child stderr.

- [ ] **Step 4: Run build integration tests**

Run: `cargo test -p yanc build_writes_only_a_deterministic_owned_cargo_project`

Expected: PASS and no generated fixture directory is inside the repository source tree.

- [ ] **Step 5: Commit build orchestration**

Run: `git add crates/yanc/Cargo.toml crates/yanc/src/main.rs crates/yan-rust-backend/src/lib.rs Cargo.lock; git commit -m "feat(cli): 调用 Cargo 构建 Yan 二进制"`

### Task 6: Reserve the M16 Standard-Library Boundary

**Files:**
- Modify: `crates/yanc/src/main.rs`
- Test: `crates/yanc/src/main.rs`

- [ ] **Step 1: Write failing reserved-namespace tests**

```rust
#[test]
fn rejects_user_import_of_reserved_yan_std_namespace() {
    let result = check_source("module app\nimport yan.std.text\nfn main() -> unit { }");
    assert_eq!(result.stderr, "error: <fixture>:2:1: reserved module namespace `yan.std`\n");
}
```

Add a test-only internal `ModuleInput` with a `yan.std.fixture` module and assert it compiles through the same graph, type-check and backend generation path.

- [ ] **Step 2: Run the focused tests**

Run: `cargo test -p yanc reserved_yan_std`

Expected: FAIL because `yan.std` is currently treated as an ordinary missing import.

- [ ] **Step 3: Add the minimal boundary check**

Reject user source imports whose first two path segments are `yan.std` before file resolution, at the import span, with exactly `reserved module namespace ` + "`yan.std`". Keep platform imports unchanged. Add an explicit internal-only module collection parameter used only by compiler-owned tests; do not create a standard-library directory, API, package format or user escape hatch.

- [ ] **Step 4: Run the boundary suite**

Run: `cargo test -p yanc reserved_yan_std`

Expected: PASS.

- [ ] **Step 5: Commit the M16 seam**

Run: `git add crates/yanc/src/main.rs; git commit -m "feat(modules): 预留内置标准库命名空间"`

### Task 7: Establish Binary Parity and Close M15

**Files:**
- Modify: `crates/yanc/src/main.rs`
- Modify: `docs/milestones/m15-rust-backend.md`

- [ ] **Step 1: Write the full binary parity matrix before changing the milestone status**

```rust
for (fixture, expected) in executable_m2_to_m13_fixtures() {
    let interpreted = run_fixture(fixture);
    let binary = build_then_run_fixture(fixture);
    assert_eq!(interpreted, expected, "interpreter fixture: {}", fixture.display());
    assert_eq!(binary, expected, "compiled fixture: {}", fixture.display());
}
```

Use all existing executable M2-M13 examples plus dedicated temporary fixtures for cross-module public calls, `mut`, struct field access, enum/Option/Result match, tuple destructuring, if, for, early return and `?`. Add assertions for frontend error preservation and forced Cargo failure mapping to `backend build failed` without child stderr.

- [ ] **Step 2: Run the parity matrix and record the first unsupported MIR shape**

Run: `cargo test -p yanc build_matches_interpreter_for_m2_to_m13_fixtures`

Expected: FAIL until every runtime and emitter case is complete.

- [ ] **Step 3: Close only emitter/runtime gaps exposed by the matrix**

For each failing fixture, add a narrowly named unit test to `yan-runtime` or `yan-rust-backend`, then implement the exact missing existing MIR operation. Do not add a new Yan feature, Cargo option, standard-library API or fallback to the interpreter.

- [ ] **Step 4: Run final repository verification**

Run: `cargo fmt --all -- --check`

Expected: PASS.

Run: `cargo test --workspace`

Expected: PASS, including every compiled-binary fixture.

Run: `git diff --check`

Expected: PASS with no `target/`, `dist/`, `.yan/` or generated Cargo project tracked.

- [ ] **Step 5: Mark M15 complete and commit the acceptance result**

Update `docs/milestones/m15-rust-backend.md` to state that only after the commands above pass: all M2-M13 fixtures compile to binaries, outputs equal the interpreter, diagnostics are stable, and the M16 internal-module boundary is verified. Do not claim standard-library APIs or M16 completion.

Run: `git add crates/yanc/src/main.rs crates/yan-runtime crates/yan-rust-backend docs/milestones/m15-rust-backend.md Cargo.lock; git commit -m "test(m15): 验收 Rust 原生后端"`
