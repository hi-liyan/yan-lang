# M14 Production IR Design

## Purpose

M14 turns the existing compiler frontend into a production-ready execution boundary. A successful Yan program must progress through resolved HIR, complete Typed HIR, verified MIR, and MIR interpretation without any later phase repeating name lookup or type checking.

The existing uncommitted Typed HIR and sequential MIR changes are the starting point. They remain subject to this design; they are not an alternative implementation contract.

## Scope

M14 supports exactly the semantics already accepted by M2 through M13. It does not add source syntax, standard-library APIs, commands, code generation, packages, capabilities, async behavior, mutable collections, or global mutable state.

M14 deliberately does not introduce SSA, optimization passes, a generic backend trait, or Rust/WASM output. Those mechanisms need real consumers, explicit performance objectives, or at least two implemented backends before their abstractions can be validated.

## Pipeline and Ownership

```text
Syntax AST -> resolved HIR -> Typed HIR -> verified MIR -> MIR interpreter
```

`yan-hir` owns compiler-session IDs and resolved targets. `yan-typeck` owns complete typed nodes and never exposes an execution path over untyped HIR. `yan-mir` owns CFG lowering and validation. `yan-eval` owns only MIR execution. `yanc` reads files, links modules, invokes each stage, and renders existing diagnostics.

`yan-source` owns `SourceId`, `SourceLocation`, and the immutable source-file table for one compilation session. Every diagnostic-bearing HIR, Typed HIR, MIR, verifier, and runtime location uses `SourceLocation { source, span }`; `Span` remains a lightweight file-local byte interval for lexer and parser operations.

## Resolved HIR

Every module, top-level definition, source local, field, and enum variant has a stable ID for one compilation session. Reads, assignments, calls, struct construction, field access, enum construction, and match patterns refer to that ID. Names may remain as diagnostic metadata, but no semantic consumer may resolve them again.

Module linking must give the frontend an explicit resolved module graph. The CLI must not synthesize declarations as a substitute for frontend resolution.

## Typed HIR

`TypedProgram` owns a typed equivalent of all executable HIR declarations and expressions. Every value expression records its canonical Yan `Type` and `Span`; every semantic operation records its resolved target. The typed tree includes local mutability and validated assignment, calls and built-ins, aggregates, fields, patterns, `if`, `match`, `for`, `return`, and Result propagation.

Compatibility-only tables that merely repeat type results by span are removed after all consumers have moved to typed nodes. Spans remain diagnostic locations, not stable identifiers.

## MIR

MIR represents each function as explicit basic blocks. Source bindings use `yan_hir::LocalId`; expression intermediates use a distinct MIR temporary ID, preventing source-local identity from being replaced by allocation order.

Instructions perform typed local writes, aggregate construction, field extraction, arithmetic, comparison, and resolved calls. Terminators express return, unconditional jump, boolean branch, enum/Option/Result match dispatch, Result error propagation, and unreachable control flow. Complex expressions are lowered in source evaluation order.

MIR contains no unlowered HIR expression, pending-control-flow marker, or string-based semantic target. Each local, value-producing instruction, and terminator carries the Yan type and source span needed for validation and diagnostics.

## Validation and Execution

`yan-mir::verify` is pure and reports a span-bearing internal compiler error when its input violates MIR invariants. It validates block and jump references, one terminator per block, local and temporary definitions before use, type-compatible writes and operands, and existing call targets with compatible argument/result types.

`yan-eval` accepts only verified MIR. It stores values by source-local and temporary ID, follows terminators, executes built-ins through explicit call targets, and reports runtime failures at MIR spans. It cannot inspect HIR or Typed HIR and cannot perform type or name lookup.

## Acceptance Evidence

Every M2-M13 semantic family receives three focused assertions:

1. Typed HIR has resolved IDs and expected Yan types.
2. MIR has the expected local, block, instruction, and terminator structure without unlowered semantic nodes.
3. The MIR interpreter produces the fixture's established output, while invalid source retains its existing stable Yan diagnostic.

The full workspace must pass formatting, tests, diff checks, and the established CLI fixture runs. `yanc check`, `yanc run`, `yanc --help`, their output, and exit codes remain unchanged.
