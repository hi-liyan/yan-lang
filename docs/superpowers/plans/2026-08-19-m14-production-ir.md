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

## Execution Status

- [x] 2026-08-19: Commit `839a704` established the current Typed HIR/MIR baseline, the M14 design and this plan.
- [x] 2026-08-19: `yan-hir` resolves string interpolation to `LocalId`, and Typed HIR consumes that ID directly; `resolves_string_interpolation_to_its_local_id` is green.
- [x] 2026-08-19: `yan-source` provides `SourceId`, `SourceLocation`, and `SourceMap`; `resolves_identical_spans_to_their_own_session_source_files` is green.
- [x] 2026-08-21: 跨模块源位置已贯通 HIR、Typed HIR、MIR、解释器与 `yanc` 诊断；诊断按 `SourceLocation` 渲染到所属源文件。
- [x] 2026-08-19: Sequential expressions now lower into independent `MirExpression`/`MirStatement` nodes; `yan-mir::Program` no longer stores `TypedProgram`.
- [x] 2026-08-19: `verify(lower(...))` produces `VerifiedProgram`; `yanc` verifies before execution and `yan-eval::execute` accepts only `VerifiedProgram`.
- [x] 2026-08-19: Independent MIR execution covers existing sequential values, mutation, `if`, `for`, structs and fields; workspace tests are green.
- [x] 2026-08-21: 模块图 ID、完整 CFG lowering、MIR 验证器、Result/Option/enum 执行和 M2 至 M13 fixture 矩阵均已完成；`cargo fmt --all -- --check`、`cargo test --workspace` 与 `git diff --check` 已通过。

### Task 1: Lock the Existing Contract

**Files:** `crates/yan-hir/src/lib.rs:1170-1410`, `crates/yan-typeck/src/lib.rs:2730-2965`, `crates/yan-mir/src/lib.rs:203-235`

- [x] **Step 1: 添加 HIR 和 MIR 结构测试**

```rust
#[test]
fn resolves_semantic_references_to_session_ids() {
    let program = lower_program("struct User { name: string } fn label(user: User) -> string { user.name } fn main() -> unit { let user = User { name: \"Yan\" } label(user) }");
    assert!(matches!(program.functions[0].statements.last(), Some(Statement::Expression(Expression::FieldAccess { field_id: Some(_), .. }))));
    assert!(matches!(program.functions[1].statements.last(), Some(Statement::Expression(Expression::Call { function: Some(_), .. }))));
}
```

Add identical focused coverage for interpolation, enum/match binding, mutation, tuple destructuring, `if`, `for`, `return`, and `?`. Each typed assertion checks IDs and `Type`; each MIR assertion rejects `TypedExpression`, `TypedStatement`, and HIR `Expression` payloads.

- [x] **Step 2: 执行结构契约测试**

Run: `cargo test -p yan-hir resolves_semantic_references_to_session_ids; cargo test -p yan-mir`

结果：MIR 结构测试通过，公开 MIR 节点不再嵌入 Typed HIR 或 HIR 表达式。

- [x] **Step 3: 将结构契约纳入 M14 回归范围**

### Task 2: Carry Source Locations Across Module Boundaries

**Files:** `crates/yan-source/src/lib.rs`, `crates/yan-hir/src/lib.rs`, `crates/yan-typeck/src/lib.rs`, `crates/yan-mir/src/lib.rs`, `crates/yan-eval/src/lib.rs`, `crates/yanc/src/main.rs`

- [x] **Step 1: Extend the source-table test into an API contract**

```rust
let location = SourceLocation::new(second, Span::new(4, 9));
assert_eq!(sources.get(location.source).map(SourceFile::path), Some(Path::new("second.yan")));
```

Run: `cargo test -p yan-source resolves_identical_spans_to_their_own_session_source_files`

Expected: PASS after `SourceMap::insert` and `SourceMap::get` are introduced; `Span` remains unchanged.

- [x] **Step 2: Introduce source-bearing HIR module and diagnostic boundaries**

Add `source: SourceId` to `yan_hir::Program` and use `SourceLocation` in `LowerError`, `TypeError`, MIR instructions/terminators, verifier errors, and evaluator errors. Do not add `SourceId` to `Span` or to lexer/parser tokens. A module's `SourceId` is attached once after syntax lowering, and each diagnostic location is built from its owning module source plus the existing span.

- [x] **Step 3: Build one source map per `yanc` compilation**

Make `read_module` insert each read `SourceFile` into the compilation `SourceMap`; retain the assigned ID in `ModuleFile`. `render_diagnostic` must look up `diagnostic.location.source`, then compute line/column from `diagnostic.location.span`. Unknown IDs yield the stable English internal diagnostic `invalid source location`.

- [x] **Step 4: 添加跨模块错误回归**

Create an existing-style temporary module fixture whose imported public function has an undefined variable. Assert `yanc check` reports the imported file path and its original line/column, not the entry file path.

Run: `cargo test -p yan-source; cargo test -p yanc cross_module_diagnostic_uses_imported_source_file`

Expected: PASS; two files with the same byte offset render distinct paths.

- [x] **Step 5: 验证跨模块源位置传播**

### Task 3: Resolve the Module Graph in `yan-hir`

**Files:** `crates/yan-hir/src/lib.rs:1-780`, `crates/yanc/src/main.rs:68-326`

- [x] **Step 1: Write a failing imported-symbol test**

```rust
#[test]
fn resolves_imported_public_function_without_cli_declaration_append() {
    let graph = ModuleGraph::new(vec![entry_module(), message_module()]);
    let program = resolve_modules(graph).expect("fixture must resolve");
    assert_eq!(find_main_call(&program).function, Some(DefId(1)));
}
```

Run: `cargo test -p yan-hir resolves_imported_public_function_without_cli_declaration_append`

结果：已由 `yan-hir` 模块图解析导入的公开符号，CLI 不再拼接声明。

- [x] **Step 2: Define module graph input and resolve all semantic targets**

```rust
pub struct ModuleInput { pub id: ModuleId, pub program: Program }
pub struct ModuleGraph { pub modules: Vec<ModuleInput>, pub entry: ModuleId }
pub struct ResolvedProgram { pub modules: Vec<Program>, pub entry: ModuleId }
pub fn resolve_modules(graph: ModuleGraph) -> Result<ResolvedProgram, ResolveError>;
```

Assign global `DefId`/`FieldId`/`VariantId` and function-scoped `LocalId`, then rewrite reads, assignments, calls, construction, field access, patterns, loop bindings, and interpolation. Names remain only diagnostic metadata.

- [x] **Step 3: Make `yanc` build the graph instead of appending declarations**

Keep file reads, source-root checks, private-import checks, and cycle diagnostics in `yanc`. Replace `append_public_symbol` with `ModuleInput` collection and `resolve_modules`, mapping `ResolveError { span, message }` to the existing renderer.

- [x] **Step 4: 验证模块图解析**

Run: `cargo test -p yan-hir; cargo test -p yanc links_public_declarations_from_file_modules`

Expected: PASS; imports and local reads resolve by IDs.


### Task 4: Complete Typed HIR

**Files:** `crates/yan-typeck/src/lib.rs:20-730`

- [x] **Step 1: 添加插值目标测试**

```rust
#[test]
fn typed_interpolation_uses_its_resolved_local_id() {
    let typed = type_check("fn main() -> string { let title = \"Yan\" \"{title}\" }");
    assert!(matches!(tail_expression(&typed.functions[0]).kind, TypedExpressionKind::String(ref parts) if matches!(parts[0], TypedStringPart::Local(LocalId(0)))));
}
```

Run: `cargo test -p yan-typeck typed_interpolation_uses_its_resolved_local_id`

结果：Typed HIR 直接使用已解析的局部 ID，不保留名称回退。

- [x] **Step 2: 仅由已解析 ID 构建类型化节点**

Populate `TypedFunction`, `TypedStruct`, `TypedEnum`, `TypedNewtype`, `TypedCallTarget`, `TypedPattern`, `TypedStringPart`, and fields from resolved HIR targets. Each value node has exactly one Yan `Type` and `SourceLocation`.

- [x] **Step 3: 移除兼容路径**

Remove `local_for_name`, `local_in_statements`, `local_in_expression`, `field_id_for_type`, `RecordedExpression`, `ResolvedLocalId`, `ResolvedTarget`, `ResolvedReference`, and their collectors. Normal unresolved source remains a `TypeError`, never an `expect`.

- [x] **Step 4: 验证完整 Typed HIR**

Run: `cargo test -p yan-typeck`

Expected: PASS; existing English diagnostics and spans are unchanged.


### Task 5: Lower Sequential Semantics to MIR Instructions

**Files:** `crates/yan-mir/src/lib.rs:1-235`

- [x] **Step 1: Write a failing instruction-lowering test**

```rust
#[test]
fn lowers_addition_into_typed_operands_and_destination() {
    let mir = lower_fixture("fn main() -> int { let value = 1 + 2 value }");
    assert!(matches!(mir.functions[0].blocks[0].instructions[2], Instruction::Binary { operator: BinaryOperator::Add, ty: Type::Int, .. }));
}
```

Run: `cargo test -p yan-mir lowers_addition_into_typed_operands_and_destination`

结果：MIR 以类型化操作数、指令和目标位置表达顺序语义，不再持有 `TypedExpression`。

- [x] **Step 2: Replace the sequential wrapper**

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

- [x] **Step 3: 验证顺序语义 lowering**

Run: `cargo test -p yan-mir`

Expected: PASS; no MIR public node contains HIR or Typed HIR expressions.


### Task 6: Lower Complete M2-M13 Control Flow

**Files:** `crates/yan-mir/src/lib.rs`

- [x] **Step 1: 添加 CFG 测试**

```rust
#[test]
fn lowers_if_to_branch_then_else_and_join_blocks() {
    let mir = lower_fixture("fn main() -> int { if true { 1 } else { 2 } }");
    assert!(mir.functions[0].blocks.iter().any(|block| matches!(block.terminator, Terminator::Branch { then_block: BasicBlockId(1), else_block: BasicBlockId(2), .. })));
}
```

Add one test each for enum/Option/Result match, loop back-edge, early return, Result propagation, and tuple-element stores.

- [x] **Step 2: 实现类型化终结指令和 lowering 辅助逻辑**

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

- [x] **Step 3: 验证完整 M2 至 M13 控制流 lowering**

Run: `cargo test -p yan-mir`

Expected: PASS; every existing M2-M13 control-flow case has complete CFG lowering.


### Task 7: Verify MIR

**Files:** `crates/yan-mir/src/lib.rs`, `crates/yanc/src/main.rs:68-110`

- [x] **Step 1: 添加 MIR 验证器测试**

```rust
#[test]
fn rejects_branch_to_missing_block() {
    let error = verify(Program::single_function_with(Terminator::Goto { target: BasicBlockId(9), location: fixture_location() })).expect_err("missing block must be rejected");
    assert_eq!(error.message, "invalid MIR jump target");
}
```

Add distinct tests for undefined values, immutable-local writes, operand type mismatch, missing terminator, invalid target ID, and incompatible call arguments/results.

- [x] **Step 2: 实现不透明的 Verified MIR**

```rust
pub fn verify(program: Program) -> Result<VerifiedProgram, VerifyError>;
pub struct VerifyError { pub location: SourceLocation, pub message: String }
pub struct VerifiedProgram(Program);
```

Verify block/jump IDs, one terminator per block, definitions before use, mutability, operand types, and call signatures. Inspect IDs and declaration metadata only, never names.

- [x] **Step 3: 在 `yanc::compile` 中调用验证并验证行为**

Run: `cargo test -p yan-mir; cargo test -p yanc`

Expected: PASS; malformed in-memory MIR is rejected and normal CLI diagnostics are unchanged.


### Task 8: Execute Verified MIR Only

**Files:** `crates/yan-eval/Cargo.toml`, `crates/yan-eval/src/lib.rs`, `crates/yanc/src/main.rs:47-65`

- [x] **Step 1: 添加解释器一致性测试**

```rust
#[test]
fn executes_verified_mir_with_result_propagation() {
    let program = verified_fixture("fn main() -> int { let value = Ok(3) value? }");
    assert_eq!(execute(&program).expect("execution must succeed"), vec!["3"]);
}
```

Run: `cargo test -p yan-eval executes_verified_mir_with_result_propagation`

结果：解释器只读取 `VerifiedProgram`，不再通过 MIR 回读 Typed HIR。

- [x] **Step 2: 解释指令和终结指令**

Store source locals by `yan_hir::LocalId` and temporaries by `ValueId`; dispatch calls only through MIR `CallTarget`; follow all terminator kinds. Runtime errors use MIR spans. Remove all `TypedProgram`, `TypedExpression`, HIR `Expression`, and HIR `Statement` imports/APIs.

- [x] **Step 3: 移除直接依赖并验证解释器**

Remove direct `yan-typeck` and `yan-hir` dependencies unless `yan-mir` re-exports an ID. Store `VerifiedProgram` in `CompiledProgram` and preserve exact `check`/`run` output behavior.

Run: `cargo test -p yan-eval; cargo test -p yanc`

Expected: PASS; interpreter output and runtime failures match the prior behavior.


### Task 9: Fixture Matrix and M14 Closure

**Files:** `crates/yanc/src/main.rs:368-455`, `docs/milestones/m14-typed-hir-and-mir.md:3-64`

- [x] **Step 1: Add table-driven executable-fixture regression**

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

- [x] **Step 2: Run final repository verification**

Run: `cargo fmt --all -- --check; cargo test --workspace; git diff --check; git status --short; git diff`

Expected: all commands succeed; only M14 implementation/status changes remain, with no generated files.

- [x] **Step 3: 在绿色验证证据后标记 M14 完成**

已更新里程碑状态为已解析 HIR、完整 Typed HIR、经验证的可执行 MIR、Verified MIR 执行和 M2 至 M13 三层回归覆盖；不包含 M15 或代码生成。
