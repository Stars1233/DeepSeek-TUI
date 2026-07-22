//! Bounded, non-transcript presentation for delegated coordination receipts.
//!
//! Headless callers keep the machine-readable typed projection. The TUI uses
//! this single formatter for its Work inspector so compact rows and details do
//! not grow a second, string-parsed coordination model.

use std::fmt::Write as _;

use crate::tools::subagent::CoordinationDetailProjection;
use crate::tools::subagent::coord::{DecisionStatus, ReconciliationReceipt};

#[must_use]
pub(crate) fn summary(projection: &CoordinationDetailProjection) -> String {
    format!(
        "{} decisions · {} contentions · {} reconciled",
        projection.decisions.len(),
        projection.contentions.len(),
        projection.reconciliations.len()
    )
}

#[must_use]
pub(crate) fn needs_attention(projection: &CoordinationDetailProjection) -> bool {
    projection
        .decisions
        .iter()
        .any(|decision| decision.status == DecisionStatus::Proposed)
        || projection
            .reconciliations
            .iter()
            .any(|receipt| receipt.verification_outcome != "verified")
}

/// Format the durable coordination projection for the shared Work pager.
///
/// Deliberately omitted: decision constraints and general evidence handles.
/// Those fields inform delegated prompts and headless inspection, but they can
/// contain operator-authored detail that does not belong in ambient TUI chrome.
#[must_use]
pub(crate) fn format(projection: &CoordinationDetailProjection) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Schema {} · sequence {} · bounded to {} records",
        projection.schema_version, projection.sequence, projection.limit
    );

    section(&mut out, "Decisions");
    if projection.decisions.is_empty() {
        let _ = writeln!(out, "None");
    } else {
        for decision in &projection.decisions {
            let _ = writeln!(
                out,
                "{} · {}\n  status {} · owner {} · version {}",
                decision.decision_id,
                decision.subject,
                decision_status(decision.status),
                decision.owner,
                decision.version
            );
        }
    }

    section(&mut out, "Write claims");
    if projection.write_claims.is_empty() {
        let _ = writeln!(out, "None");
    } else {
        for receipt in &projection.write_claims {
            let claim = &receipt.claim;
            let _ = writeln!(
                out,
                "{} · {}\n  paths {}\n  contracts {}",
                claim.owner,
                if receipt.isolated_worktree {
                    "isolated"
                } else {
                    "shared workspace"
                },
                joined_paths(&claim.roots, &claim.exact_files),
                joined_or_none(&claim.contracts)
            );
        }
    }

    section(&mut out, "Contentions");
    if projection.contentions.is_empty() {
        let _ = writeln!(out, "None");
    } else {
        for receipt in &projection.contentions {
            let _ = writeln!(
                out,
                "claimant {} · owner {}\n  paths {}\n  contracts {}\n  disposition {}",
                receipt.claimant,
                receipt.conflicting_owner,
                joined_paths(&receipt.roots, &receipt.exact_files),
                joined_or_none(&receipt.contracts),
                receipt.disposition
            );
        }
    }

    section(&mut out, "Neutral reconciliation");
    if projection.reconciliations.is_empty() {
        let _ = writeln!(out, "None");
    } else {
        for receipt in &projection.reconciliations {
            format_reconciliation(&mut out, receipt);
        }
    }

    section(&mut out, "Context projections");
    if projection.context_projections.is_empty() {
        let _ = writeln!(out, "None");
    } else {
        for receipt in &projection.context_projections {
            let _ = writeln!(
                out,
                "{} · decisions {} · {} bytes · {} deduplicated · {} omitted",
                receipt.child_id,
                joined_or_none(&receipt.decision_ids),
                receipt.projected_bytes,
                receipt.deduplicated,
                receipt.omitted
            );
        }
    }

    section(&mut out, "Active hot paths");
    if projection.metrics.hottest_paths.is_empty() {
        let _ = writeln!(out, "None");
    } else {
        for path in &projection.metrics.hottest_paths {
            let _ = writeln!(out, "{} · {} active claims", path.path, path.active_claims);
        }
    }
    let _ = writeln!(out, "Metrics note\n{}", projection.metrics.note);

    out.trim_end().to_string()
}

fn section(out: &mut String, label: &str) {
    let _ = write!(out, "\n{label}\n");
}

fn format_reconciliation(out: &mut String, receipt: &ReconciliationReceipt) {
    let _ = writeln!(
        out,
        "{} · {} candidates · retry {}/{}\n  owner {}\n  reviewer {}\n  verifier {}\n  verification {}",
        receipt.subject,
        receipt.candidate_handles.len(),
        receipt.retry_count,
        receipt.retry_limit,
        receipt.owner,
        joined_or_none(&receipt.reviewer_evidence_handles),
        joined_or_none(&receipt.verifier_evidence_handles),
        receipt.verification_outcome
    );
}

const fn decision_status(status: DecisionStatus) -> &'static str {
    match status {
        DecisionStatus::Proposed => "proposed",
        DecisionStatus::Accepted => "accepted",
        DecisionStatus::Superseded => "superseded",
    }
}

fn joined_paths(roots: &[String], exact_files: &[String]) -> String {
    let values = roots
        .iter()
        .chain(exact_files)
        .map(String::as_str)
        .collect::<Vec<_>>();
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

fn joined_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::tools::subagent::coord::{
        ContextProjectionReceipt, CoordinationDetailMetrics, CoordinationHotPath, DecisionRecord,
        PersistedWriteClaim, WriteContentionReceipt, WriteScopeClaim,
    };

    fn projection() -> CoordinationDetailProjection {
        CoordinationDetailProjection {
            schema_version: 1,
            sequence: 9,
            decisions: vec![DecisionRecord {
                decision_id: "decision-ui".to_string(),
                subject: "composer edges".to_string(),
                status: DecisionStatus::Accepted,
                owner: "planner".to_string(),
                scope: vec!["path:crates/tui".to_string()],
                constraints: vec!["PRIVATE-TRANSCRIPT-MARKER".to_string()],
                evidence_handles: vec!["artifact:hidden-evidence".to_string()],
                version: 3,
                sequence: 1,
            }],
            write_claims: vec![PersistedWriteClaim {
                claim: WriteScopeClaim {
                    owner: "worker-a".to_string(),
                    roots: vec!["crates/tui".to_string()],
                    exact_files: vec!["Cargo.toml".to_string()],
                    contracts: vec!["ui-contract".to_string()],
                },
                sequence: 2,
                isolated_worktree: false,
            }],
            reconciliations: vec![ReconciliationReceipt {
                reconciliation_id: "reconcile-ui".to_string(),
                subject: "composer edges".to_string(),
                owner: "release-owner".to_string(),
                input_decisions: vec!["decision-a".to_string(), "decision-b".to_string()],
                outcome: "candidate-a".to_string(),
                evidence_handles: Vec::new(),
                candidate_handles: vec!["branch:a".to_string(), "branch:b".to_string()],
                retry_count: 1,
                retry_limit: 3,
                reviewer_evidence_handles: vec!["agent:reviewer".to_string()],
                verifier_evidence_handles: vec!["agent:verifier".to_string()],
                verification_outcome: "verified".to_string(),
                sequence: 3,
            }],
            context_projections: vec![ContextProjectionReceipt {
                child_id: "worker-a".to_string(),
                decision_ids: vec!["decision-ui".to_string()],
                projected_bytes: 128,
                deduplicated: 2,
                omitted: 1,
                sequence: 4,
            }],
            contentions: vec![WriteContentionReceipt {
                claimant: "worker-b".to_string(),
                conflicting_owner: "worker-a".to_string(),
                roots: vec!["crates/tui".to_string()],
                exact_files: vec!["Cargo.toml".to_string()],
                contracts: vec!["ui-contract".to_string()],
                disposition: "blocked_pending_isolation_or_serialization".to_string(),
                sequence: 5,
            }],
            metrics: CoordinationDetailMetrics {
                hottest_paths: vec![CoordinationHotPath {
                    path: "crates/tui".to_string(),
                    active_claims: 2,
                }],
                package_or_module_growth: Some(json!({"ignored": true})),
                route_or_cost: None,
                note: "Only active owners contribute to hot paths".to_string(),
            },
            bounded: true,
            limit: 24,
        }
    }

    #[test]
    fn formatter_uses_typed_receipts_without_transcript_shaped_fields() {
        let text = format(&projection());
        for required in [
            "decision-ui · composer edges",
            "status accepted · owner planner · version 3",
            "claimant worker-b · owner worker-a",
            "paths crates/tui, Cargo.toml",
            "contracts ui-contract",
            "disposition blocked_pending_isolation_or_serialization",
            "composer edges · 2 candidates · retry 1/3",
            "reviewer agent:reviewer",
            "verifier agent:verifier",
            "verification verified",
            "worker-a · decisions decision-ui · 128 bytes · 2 deduplicated · 1 omitted",
            "crates/tui · 2 active claims",
        ] {
            assert!(text.contains(required), "missing {required}:\n{text}");
        }
        assert!(!text.contains("PRIVATE-TRANSCRIPT-MARKER"), "{text}");
        assert!(!text.contains("hidden-evidence"), "{text}");
        assert!(!text.contains("ignored"), "{text}");
    }

    #[test]
    fn attention_comes_from_typed_status_and_verification_only() {
        let mut value = projection();
        assert!(!needs_attention(&value));
        value.decisions[0].status = DecisionStatus::Proposed;
        assert!(needs_attention(&value));
        value.decisions[0].status = DecisionStatus::Accepted;
        value.reconciliations[0].verification_outcome = "blocked".to_string();
        assert!(needs_attention(&value));
    }
}
