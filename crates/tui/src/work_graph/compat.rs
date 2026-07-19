//! Compat projections — signatures only in this slice.
//!
//! Invariant V10 is enforced at the type level: every projection is a pure
//! function taking `&WorkGraphSnapshot` and returning owned data. Nothing
//! hands a projection mutable graph access, so projections cannot write the
//! graph or each other — one graph writes every projection.
//!
//! The session-compat slice implements these bodies (plan/todo snapshot
//! derivation and legacy tool adapters). Until then they are declared but
//! unimplemented so the type-level story — and its compile-time test — is
//! already in force.

use super::model::WorkGraphSnapshot;

/// Placeholder for the derived plan snapshot (shaped in the session-compat
/// slice to match the existing plan store's wire format).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PlanProjection;

/// Placeholder for the derived todo snapshot (shaped in the session-compat
/// slice to match the existing todo store's wire format).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TodoProjection;

/// Derive the plan view. Pure function of the snapshot (V10).
pub fn project_plan(snapshot: &WorkGraphSnapshot) -> PlanProjection {
    let _ = snapshot;
    unimplemented!("implemented in the session-compat slice")
}

/// Derive the todo view. Pure function of the snapshot (V10).
pub fn project_todos(snapshot: &WorkGraphSnapshot) -> TodoProjection {
    let _ = snapshot;
    unimplemented!("implemented in the session-compat slice")
}
