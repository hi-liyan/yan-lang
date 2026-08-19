# M14 Production IR Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver a verified, executable MIR for every M2-M13 semantic without changing the CLI contract.

**Architecture:** `yan-source` owns the session source table and `SourceLocation`; `Span` remains a file-local byte interval. `yan-hir` owns session IDs and resolved modules; `yan-typeck` owns complete Typed HIR; `yan-mir` owns typed CFG lowering and validation. `yan-eval` executes only verified MIR, while `yanc` reads files, orchestrates stages, and renders diagnostics.

**Tech Stack:** Rust 2021, standard library collections, and the existing Yan workspace crates.

---

## File Structure

- Modify: `crates/yan-hir/src/lib.rs` - resolved module graph and ID-only references.
- Modify: `crates/yan-source/src/lib.rs` - session-local source IDs and source-file lookup.
- Modify: `crates/yan-typeck/src/lib.rs` - complete Typed HIR without string fallback.
- Modify: `crates/yan-mir/src/lib.rs` - values, instructions, CFG lowering, verifier.
- Modify: `crates/yan-eval/{Cargo.toml,src/lib.rs}` - verified-MIR-only execution.
- Modify: `crates/yanc/src/main.rs` - graph orchestration, verification, regressions.
- Modify: `docs/milestones/m14-typed-hir-and-mir.md` - status only after green evidence.

No runtime crate, backend trait, optimizer, CLI command, or Yan source feature is added. Tests remain in the implementation crate.

### Task 1: Lock the Existing Contract

**Files:** `crates/yan-hir/src/lib.rs:1170-1410`, `crates/yan-typeck/src/lib.rs:2730-2965`, `crates/yan-mir/src/lib.rs:203-235`

- [ ] **Step 1: Write failing HIR and MIR shape tests**

```rust
#[test]
fn resolves_semantic_references_to_session_ids() {
    let program = lower_program("struct User { name: string } fn label(user: User) -> string { user.name } fn main() -> unit { let user = User { name: \"Yan\" } label(user) }");
    assert!(matches!(program.functions[0].statements.last(), Some(Statement::Expression(Expression::FieldAccess { field_id: Some(_), .. }))));
    assert!(matches!(program.functions[1].statements.last(), Some(Statement::Expression(Expression::Call { function: Some(_), .. }))));
}
```

Add identical focused coverage for interpolation, enum/match binding, mutation, tuple destructuring, `if`, `for`, `return`, and `?`. Each typed assertion checks IDs and `Type`; each MIR assertion rejects `TypedExpression`, `TypedStatement`, and HIR `Expression` payloads.

- [ ] **Step 2: Run the failing contract tests**

Run: `cargo test -p yan-hir resolves_semantic_references_to_session_ids; cargo test -p yan-mir`

Expected: new MIR-shape tests fail because the current MIR embeds Typed HIR; existing tests pass.

- [ ] **Step 3: Commit the test contract**

Run: `git add crates/yan-hir/src/lib.rs crates/yan-typeck/src/lib.rs crates/yan-mir/src/lib.rs; git commit -m "test(m14): 固定已解析和中间表示契约"`

### Task 2: Carry Source Locations Across Module Boundaries

**Files:** `crates/yan-source/src/lib.rs`, `crates/yan-hir/src/lib.rs`, `crates/yan-typeck/src/lib.rs`, `crates/yan-mir/src/lib.rs`, `crates/yan-eval/src/lib.rs`, `crates/yanc/src/main.rs`

- [ ] **Step 1: Extend the source-table test into an API contract**

```rust
let location = SourceLocation::new(second, Span::new(4, 9));
assert_eq!(sources.file(location).map(SourceFile::path), Some(Path::new("second.yan")));
```

Run: `cargo test -p yan-source resolves_identical_spans_to_their_own_session_source_files`

Expected: PASS after `SourceMap::file(SourceLocation)` is introduced; `Span` remains unchanged.

- [ ] **Step 2: Introduce source-bearing HIR module and diagnostic boundaries**

Add `source: SourceId` to `yan_hir::Program` and use `SourceLocation` in `LowerError`, `TypeError`, MIR instructions/terminators, verifier errors, and evaluator errors. Do not add `SourceId` to `Span` or to lexer/parser tokens. A module's `SourceId` is attached once after syntax lowering, and each diagnostic location is built from its owning module source plus the existing span.

- [ ] **Step 3: Build one source map per `yanc` compilation**

Make `read_module` insert each read `SourceFile` into the compilation `SourceMap`; retain the assigned ID in `ModuleFile`. `render_diagnostic` must look up `diagnostic.location.source`, then compute line/column from `diagnostic.location.span`. Unknown IDs yield the stable English internal diagnostic `invalid source location`.

- [ ] **Step 4: Add cross-module error regression before graph refactoring**

Create an existing-style temporary module fixture whose imported public function has an undefined variable. Assert `yanc check` reports the imported file path and its original line/column, not the entry file path.

Run: `cargo test -p yan-source; cargo test -p yanc cross_module_diagnostic_uses_imported_source_file`

Expected: PASS; two files with the same byte offset render distinct paths.

- [ ] **Step 5: Commit source-location propagation**

Run: `git add crates/yan-source/src/lib.rs crates/yan-hir/src/lib.rs crates/yan-typeck/src/lib.rs crates/yan-mir/src/lib.rs crates/yan-eval/src/lib.rs crates/yanc/src/main.rs Cargo.lock; git commit -m "feat(source): 传递跨模块诊断来源位置"`

### Task 3: Resolve the Module Graph in `yan-hir`

**Files:** `crates/yan-hir/src/lib.rs:1-780`, `crates/yanc/src/main.rs:68-326`

- [ ] **Step 1: Write a failing imported-symbol test**

```rust
#[test]
fn resolves_imported_public_function_without_cli_declaration_append() {
    let graph = ModuleGraph::new(vec![entry_module(), message_module()]);
    let program = resolve_modules(graph).expect("fixture must resolve");
    assert_eq!(find_main_call(&program).function, Some(DefId(1)));
}
```

Run: `cargo test -p yan-hir resolves_imported_public_function_without_cli_declaration_append`

Expected: FAIL because `yanc::append_public_symbol` flattens declarations.

- [ ] **Step 2: Define module graph input and resolve all semantic targets**

```rust
pub struct ModuleInput { pub id: ModuleId, pub program: Program }
pub struct ModuleGraph { pub modules: Vec<ModuleInput>, pub entry: ModuleId }
pub struct ResolvedProgram { pub modules: Vec<Program>, pub entry: ModuleId }
pub fn resolve_modules(graph: ModuleGraph) -> Result<ResolvedProgram, ResolveError>;
```

Assign global `DefId`/`FieldId`/`VariantId` and function-scoped `LocalId`, then rewrite reads, assignments, calls, construction, field access, patterns, loop bindings, and interpolation. Names remain only diagnostic metadata.

- [ ] **Step 3: Make `yanc` build the graph instead of appending declarations**

Keep file reads, source-root checks, private-import checks, and cycle diagnostics in `yanc`. Replace `append_public_symbol` with `ModuleInput` collection and `resolve_modules`, mapping `ResolveError { span, message }` to the existing renderer.

- [ ] **Step 4: Verify and commit**

Run: `cargo test -p yan-hir; cargo test -p yanc links_public_declarations_from_file_modules`

Expected: PASS; imports and local reads resolve by IDs.

Run: `git add crates/yan-hir/src/lib.rs crates/yanc/src/main.rs; git commit -m "feat(hir): 解析模块图语义引用"`

### Task 4: Complete Typed HIR

**Files:** `crates/yan-typeck/src/lib.rs:20-730`

- [ ] **Step 1: Write a failing interpolation-target test**

```rust
#[test]
fn typed_interpolation_uses_its_resolved_local_id() {
    let typed = type_check("fn main() -> string { let title = \"Yan\" \"{title}\" }");
    assert!(matches!(tail_expression(&typed.functions[0]).kind, TypedExpressionKind::String(ref parts) if matches!(parts[0], TypedStringPart::Local(LocalId(0)))));
}
```

Run: `cargo test -p yan-typeck typed_interpolation_uses_its_resolved_local_id`

Expected: FAIL while `local_for_name` exists.

- [ ] **Step 2: Build typed nodes from resolved IDs only**

Populate `TypedFunction`, `TypedStruct`, `TypedEnum`, `TypedNewtype`, `TypedCallTarget`, `TypedPattern`, `TypedStringPart`, and fields from resolved HIR targets. Each value node has exactly one Yan `Type` and `SourceLocation`.

- [ ] **Step 3: Delete compatibility paths**

Remove `local_for_name`, `local_in_statements`, `local_in_expression`, `field_id_for_type`, `RecordedExpression`, `ResolvedLocalId`, `ResolvedTarget`, `ResolvedReference`, and their collectors. Normal unresolved source remains a `TypeError`, never an `expect`.

- [ ] **Step 4: Verify and commit**

Run: `cargo test -p yan-typeck`

Expected: PASS; existing English diagnostics and spans are unchanged.

Run: `git add crates/yan-typeck/src/lib.rs; git commit -m "refactor(typeck): 消除类型化节点名称回退"`

### Task 5: Lower Sequential Semantics to MIR Instructions

**Files:** `crates/yan-mir/src/lib.rs:1-235`

- [ ] **Step 1: Write a failing instruction-lowering test**

```rust
#[test]
fn lowers_addition_into_typed_operands_and_destination() {
    let mir = lower_fixture("fn main() -> int { let value = 1 + 2 value }");
    assert!(matches!(mir.functions[0].blocks[0].instructions[2], Instruction::Binary { operator: BinaryOperator::Add, ty: Type::Int, .. }));
}
```

Run: `cargo test -p yan-mir lowers_addition_into_typed_operands_and_destination`

Expected: FAIL because MIR holds `TypedExpression`.

- [ ] **Step 2: Replace the sequential wrapper**

```rust
pub struct ValueId(pub u32);
pub enum Operand { Constant(Constant), Local(yan_hir::LocalId), Value(ValueId) }
pub enum Instruction {
    Assign { destination: ValueId, operand: Operand, ty: Type, location: SourceLocation },
    StoreLocal { local: yan_hir::LocalId, value: Operand, location: SourceLocation },
    Binary { destination: ValueId, operator: BinaryOperator, left: Operand, right: Operand, ty: Type, location: SourceLocation },
    Call { destination: Option<ValueId>, target: CallTarget, arguments: Vec<Operand>, ty: Type, location: SourceLocation },
}
```

Implement `FunctionLowerer` to lower literals, interpolation, aggregates, arithmetic, equality, local reads, `let`, assignment, struct/newtype/enum construction, fields, user calls, and existing built-ins in source evaluation order.

- [ ] **Step 3: Verify and commit**

Run: `cargo test -p yan-mir`

Expected: PASS; no MIR public node contains HIR or Typed HIR expressions.

Run: `git add crates/yan-mir/src/lib.rs; git commit -m "feat(mir): 降低顺序表达式为类型化指令"`

### Task 6: Lower Complete M2-M13 Control Flow

**Files:** `crates/yan-mir/src/lib.rs`

- [ ] **Step 1: Write failing CFG tests**

```rust
#[test]
fn lowers_if_to_branch_then_else_and_join_blocks() {
    let mir = lower_fixture("fn main() -> int { if true { 1 } else { 2 } }");
    assert!(mir.functions[0].blocks.iter().any(|block| matches!(block.terminator, Terminator::Branch { then_block: BasicBlockId(1), else_block: BasicBlockId(2), .. })));
}
```

Add one test each for enum/Option/Result match, loop back-edge, early return, Result propagation, and tuple-element stores.

- [ ] **Step 2: Implement typed terminators and lowering helpers**

```rust
pub enum Terminator {
    Goto { target: BasicBlockId, location: SourceLocation },
    Branch { condition: Operand, then_block: BasicBlockId, else_block: BasicBlockId, location: SourceLocation },
    Match { target: Operand, arms: Vec<MatchTarget>, otherwise: BasicBlockId, location: SourceLocation },
    Return { value: Option<Operand>, location: SourceLocation },
    PropagateErr { result: Operand, success: BasicBlockId, location: SourceLocation },
    Unreachable { location: SourceLocation },
}
```

Use `new_block`, `terminate_current`, and `join_value`. A second terminator returns a span-bearing lowering error. `for` uses only existing List behavior; no iterator API is added.

- [ ] **Step 3: Verify and commit**

Run: `cargo test -p yan-mir`

Expected: PASS; every existing M2-M13 control-flow case has complete CFG lowering.

Run: `git add crates/yan-mir/src/lib.rs; git commit -m "feat(mir): 降低既有控制流为基本块图"`

### Task 7: Verify MIR

**Files:** `crates/yan-mir/src/lib.rs`, `crates/yanc/src/main.rs:68-110`

- [ ] **Step 1: Write failing verifier tests**

```rust
#[test]
fn rejects_branch_to_missing_block() {
    let error = verify(Program::single_function_with(Terminator::Goto { target: BasicBlockId(9), location: fixture_location() })).expect_err("missing block must be rejected");
    assert_eq!(error.message, "invalid MIR jump target");
}
```

Add distinct tests for undefined values, immutable-local writes, operand type mismatch, missing terminator, invalid target ID, and incompatible call arguments/results.

- [ ] **Step 2: Implement opaque verified MIR**

```rust
pub fn verify(program: Program) -> Result<VerifiedProgram, VerifyError>;
pub struct VerifyError { pub location: SourceLocation, pub message: String }
pub struct VerifiedProgram(Program);
```

Verify block/jump IDs, one terminator per block, definitions before use, mutability, operand types, and call signatures. Inspect IDs and declaration metadata only, never names.

- [ ] **Step 3: Invoke verification in `yanc::compile`, verify, and commit**

Run: `cargo test -p yan-mir; cargo test -p yanc`

Expected: PASS; malformed in-memory MIR is rejected and normal CLI diagnostics are unchanged.

Run: `git add crates/yan-mir/src/lib.rs crates/yanc/src/main.rs; git commit -m "feat(mir): 验证控制流和类型不变量"`

### Task 8: Execute Verified MIR Only

**Files:** `crates/yan-eval/Cargo.toml`, `crates/yan-eval/src/lib.rs`, `crates/yanc/src/main.rs:47-65`

- [ ] **Step 1: Write a failing interpreter parity test**

```rust
#[test]
fn executes_verified_mir_with_result_propagation() {
    let program = verified_fixture("fn main() -> int { let value = Ok(3) value? }");
    assert_eq!(execute(&program).expect("execution must succeed"), vec!["3"]);
}
```

Run: `cargo test -p yan-eval executes_verified_mir_with_result_propagation`

Expected: FAIL because the evaluator reads Typed HIR through MIR.

- [ ] **Step 2: Interpret instructions and terminators**

Store source locals by `yan_hir::LocalId` and temporaries by `ValueId`; dispatch calls only through MIR `CallTarget`; follow all terminator kinds. Runtime errors use MIR spans. Remove all `TypedProgram`, `TypedExpression`, HIR `Expression`, and HIR `Statement` imports/APIs.

- [ ] **Step 3: Remove direct dependencies, verify, and commit**

Remove direct `yan-typeck` and `yan-hir` dependencies unless `yan-mir` re-exports an ID. Store `VerifiedProgram` in `CompiledProgram` and preserve exact `check`/`run` output behavior.

Run: `cargo test -p yan-eval; cargo test -p yanc`

Expected: PASS; interpreter output and runtime failures match the prior behavior.

Run: `git add crates/yan-eval/Cargo.toml crates/yan-eval/src/lib.rs crates/yanc/src/main.rs Cargo.lock; git commit -m "refactor(eval): 执行已验证 MIR"`

### Task 9: Fixture Matrix and M14 Closure

**Files:** `crates/yanc/src/main.rs:368-455`, `docs/milestones/m14-typed-hir-and-mir.md:3-64`

- [ ] **Step 1: Add table-driven executable-fixture regression**

```rust
for (path, expected) in [
    ("examples/hello.yan", &["Hello, Yan!"][..]),
    ("examples/language-design/02-functions/01_functions.yan", &["total: 597"][..]),
    ("examples/language-design/13-mutation-and-visibility/01_mut.yan", &["2"][..]),
] {
    assert_eq!(run_fixture(path), expected);
}
```

Include every executable M2-M13 example and exact line/column assertions for existing diagnostic fixtures.

- [ ] **Step 2: Run final repository verification**

Run: `cargo fmt --all -- --check; cargo test --workspace; git diff --check; git status --short; git diff`

Expected: all commands succeed; only M14 implementation/status changes remain, with no generated files.

- [ ] **Step 3: Mark M14 complete only after green evidence and commit**

Update the milestone status to resolved HIR, complete Typed HIR, verified executable MIR, MIR execution, and M2-M13 three-layer regression coverage. Do not claim M15 or code generation.

Run: `git add crates/yanc/src/main.rs docs/milestones/m14-typed-hir-and-mir.md; git commit -m "test(m14): 验收生产级执行中间表示"`
