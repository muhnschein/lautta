// SPDX-License-Identifier: MIT
// Copy of github/codeql rust/ql/src/queries/security/CWE-825/AccessInvalidPointer.ql
// (codeql/rust-queries 0.1.45, Copyright (c) GitHub, Inc., MIT), minus sinks in
// code `#[derive(QObject)]` writes.
/**
 * @name Access of invalid pointer
 * @description Dereferencing an invalid or dangling pointer causes undefined behavior and may result in memory corruption.
 * @kind path-problem
 * @problem.severity error
 * @security-severity 7.5
 * @precision high
 * @id rust/access-invalid-pointer
 * @tags reliability
 *       security
 *       external/cwe/cwe-476
 *       external/cwe/cwe-825
 */

import rust
import codeql.rust.dataflow.DataFlow
import codeql.rust.dataflow.TaintTracking
import codeql.rust.security.AccessInvalidPointerExtensions
import AccessInvalidPointerFlow::PathGraph

/**
 * Holds if `n` is in code `#[derive(QObject)]` writes: its
 * `impl qmetaobject::QObject` takes signal offsets as `&(*null).field` (no
 * memory read), which stock query flags on derive line. Hand-written code never
 * matches.
 */
predicate inQObjectDerive(AstNode n) {
  exists(MacroItems m, Impl i |
    m = any(TypeItem t).getADeriveMacroExpansion() and
    i = m.getAnItem() and
    i.getTrait().getCanonicalPath() = "qmetaobject::QObject" and
    n.getParentNode+() = m
  )
}

module AccessInvalidPointerConfig implements DataFlow::ConfigSig {
  import AccessInvalidPointer

  predicate isSource(DataFlow::Node node) { node instanceof Source }

  predicate isSink(DataFlow::Node node) {
    node instanceof Sink and not inQObjectDerive(node.asExpr())
  }

  predicate isBarrier(DataFlow::Node barrier) { barrier instanceof Barrier }

  predicate isBarrierOut(DataFlow::Node node) { isSink(node) }

  predicate observeDiffInformedIncrementalMode() { any() }
}

module AccessInvalidPointerFlow = TaintTracking::Global<AccessInvalidPointerConfig>;

from AccessInvalidPointerFlow::PathNode sourceNode, AccessInvalidPointerFlow::PathNode sinkNode
where AccessInvalidPointerFlow::flowPath(sourceNode, sinkNode)
select sinkNode.getNode(), sourceNode, sinkNode,
  "This operation dereferences a pointer that may be $@.", sourceNode.getNode(), "invalid"
