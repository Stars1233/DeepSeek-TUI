//! Work Graph — the single authoritative work ledger for a session.
//!
//! One graph carries objectives, plan steps, operations, evidence, blockers,
//! and approvals. **Invariant: one graph writes every projection; projections
//! never write each other.** Plan and todo views, work-surface rows, and the
//! inspector all derive from [`WorkGraphSnapshot`] through pure functions.
//!
//! Why a graph instead of parallel trackers: flat status lists let an agent
//! mark work "done" by assertion, lose dependency structure, and cannot say
//! what evidence backed a completion. Here completion and verification are
//! distinct states, `Verified` is unreachable without a satisfying evidence
//! path (V4, fail-closed), dependencies are first-class edges (acyclic, V1),
//! and liveness truth stays with the owning subsystems — the graph records
//! observations, it never invents them.
//!
//! This slice is the core only: model, changes, pure reducer, validation.
//! Session persistence, legacy import, UI projections, and liveness adapters
//! land in later slices; nothing in the app or engine calls this yet.
// Staged cutover: later slices wire persistence, UI, and liveness; until
// then the public surface (including re-exports) has no external callers.
#![allow(dead_code)]
#![allow(unused_imports)]

mod compat;
mod events;
mod ids;
mod model;
mod reducer;
mod validate;

#[cfg(test)]
mod tests;

pub use events::{
    ApprovalRef, CancelOutcome, ChangeCtx, ChangeReceipt, ObservationSummary, OperationObservation,
    OwnerState, ProposedNodeUpdate, WorkGraphChange, WorkGraphProposal, WorkNodePatch,
};
pub use ids::{BindingId, ChangeId, ProposalId, WorkEdgeId, WorkNodeId};
pub use model::{
    AcceptanceRequirement, BoundedSet, BoundedVec, EdgeKind, EvidenceKind, EvidenceKindTag,
    EvidenceRef, EvidenceRefError, HISTORY_CAP, IdempotencyKey, NodeKind, NodeState,
    OperationBinding, Provenance, SCHEMA_VERSION, SEEN_KEYS_CAP, Ts, WorkEdge, WorkGraphSnapshot,
    WorkNode, external_identity_is_well_formed,
};
pub use reducer::apply;
pub use validate::{ValidationCode, ValidationReport, Violation, validate};

/// Convenience wrapper owning the current snapshot. [`WorkGraph::apply`]
/// commits the reducer's result on success and leaves the snapshot untouched
/// on rejection — the fail-closed contract, packaged.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkGraph {
    snapshot: WorkGraphSnapshot,
}

impl WorkGraph {
    #[must_use]
    pub fn new() -> Self {
        Self {
            snapshot: WorkGraphSnapshot::new(),
        }
    }

    #[must_use]
    pub fn from_snapshot(snapshot: WorkGraphSnapshot) -> Self {
        Self { snapshot }
    }

    #[must_use]
    pub fn snapshot(&self) -> &WorkGraphSnapshot {
        &self.snapshot
    }

    #[must_use]
    pub fn into_snapshot(self) -> WorkGraphSnapshot {
        self.snapshot
    }

    /// Apply one change through the reducer. On `Ok` the new snapshot is
    /// committed; on `Err` the held snapshot is unchanged.
    pub fn apply(
        &mut self,
        change: WorkGraphChange,
        ctx: ChangeCtx,
    ) -> Result<ChangeReceipt, ValidationReport> {
        let (next, receipt) = reducer::apply(&self.snapshot, change, ctx)?;
        self.snapshot = next;
        Ok(receipt)
    }
}

impl Default for WorkGraph {
    fn default() -> Self {
        Self::new()
    }
}
