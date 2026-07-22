//! Unique skill mutation controller for CodeWhale-owned roots.
//!
//! All install / import / update / remove / trust writes go through this
//! module. Compatible harness roots, built-ins, and plugin snapshots are
//! never mutated.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::network_policy::NetworkPolicy;

use super::audit::{
    AuditedSkill, AuditedSkillId, DigestState, SkillActionKind, SkillAuditMode, SkillSourceKind,
    scan_with_configured,
};
use super::install::{
    self, InstallOutcome, InstallSource, UpdateResult, write_installed_from_v2, write_trust_v2,
};
use super::normalize_skill_name_for_lookup;
use super::package_digest;
use super::roots::{SkillRootCatalog, SkillRootKind, SkillScope, safe_display_path};

/// Project vs global CodeWhale-owned install target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillTargetScope {
    Project,
    Global,
}

impl SkillTargetScope {
    #[must_use]
    pub fn as_skill_scope(self) -> SkillScope {
        match self {
            Self::Project => SkillScope::Project,
            Self::Global => SkillScope::Global,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictPolicy {
    Reject,
    ReplaceConfirmed,
}

#[derive(Debug, Clone)]
pub enum SkillMutationRequest {
    InstallRemote {
        source: InstallSource,
        target: SkillTargetScope,
    },
    ImportExternal {
        source_id: AuditedSkillId,
        expected_digest: String,
        target: SkillTargetScope,
        conflict_policy: ConflictPolicy,
    },
    Update {
        skill_id: AuditedSkillId,
        expected_digest: Option<String>,
    },
    /// Resolve by name inside owned roots (compatible `/skill update` path).
    UpdateByName {
        name: String,
        scope: Option<SkillTargetScope>,
        expected_digest: Option<String>,
    },
    Remove {
        skill_id: AuditedSkillId,
        expected_digest: Option<String>,
    },
    RemoveByName {
        name: String,
        scope: Option<SkillTargetScope>,
        expected_digest: Option<String>,
    },
    Trust {
        skill_id: AuditedSkillId,
        expected_digest: String,
    },
    TrustByName {
        name: String,
        scope: Option<SkillTargetScope>,
        expected_digest: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillMutationOutcome {
    Installed,
    Updated,
    NoChange,
    Removed,
    Trusted,
    Imported,
    AlreadyPresent,
    NeedsApproval(String),
    NetworkDenied(String),
}

#[derive(Debug, Clone)]
pub struct SkillMutationReceipt {
    #[allow(dead_code)] // surfaced by manager detail / future receipt toast
    pub action: SkillActionKind,
    pub name: String,
    #[allow(dead_code)] // surfaced by manager detail / future receipt toast
    pub scope: SkillScope,
    pub safe_target_path: String,
    #[allow(dead_code)] // reserved for digest-diff UI
    pub before_digest: Option<String>,
    #[allow(dead_code)] // reserved for digest-diff UI
    pub after_digest: Option<String>,
    pub outcome: SkillMutationOutcome,
}

/// Inputs shared by mutation operations.
pub struct MutationContext<'a> {
    pub workspace: &'a Path,
    pub home: Option<&'a Path>,
    pub configured_skills_dir: Option<&'a Path>,
    pub network: &'a NetworkPolicy,
    pub max_size: u64,
    pub registry_url: &'a str,
}

/// Resolve the on-disk CodeWhale-owned skills directory for a target scope.
pub fn resolve_owned_target(
    workspace: &Path,
    home: Option<&Path>,
    target: SkillTargetScope,
) -> Result<PathBuf> {
    let catalog = SkillRootCatalog::build(workspace, home, None);
    let kind = match target {
        SkillTargetScope::Project => SkillRootKind::CodeWhaleProject,
        SkillTargetScope::Global => SkillRootKind::CodeWhaleGlobal,
    };
    let root = catalog
        .owned_writable_roots()
        .into_iter()
        .find(|r| r.kind == kind)
        .with_context(|| format!("no owned root for {target:?}"))?;
    if !root.is_writable_owned() {
        bail!("refusing to mutate non-owned root {}", root.path.display());
    }
    Ok(root.path.clone())
}

/// Execute a mutation request against CodeWhale-owned roots only.
pub async fn execute(
    request: SkillMutationRequest,
    ctx: &MutationContext<'_>,
) -> Result<SkillMutationReceipt> {
    match request {
        SkillMutationRequest::InstallRemote { source, target } => {
            install_remote(source, target, ctx).await
        }
        SkillMutationRequest::Update {
            skill_id,
            expected_digest,
        } => update_skill(skill_id, expected_digest, ctx).await,
        SkillMutationRequest::UpdateByName {
            name,
            scope,
            expected_digest,
        } => {
            let resolved = resolve_owned_skill_by_name(ctx, &name, scope)?;
            update_skill(resolved.id, expected_digest.or(Some(resolved.digest)), ctx).await
        }
        sync => execute_sync(sync, ctx),
    }
}

/// Sync mutations (import / remove / trust) — safe to call without a Tokio runtime.
pub fn execute_sync(
    request: SkillMutationRequest,
    ctx: &MutationContext<'_>,
) -> Result<SkillMutationReceipt> {
    match request {
        SkillMutationRequest::ImportExternal {
            source_id,
            expected_digest,
            target,
            conflict_policy,
        } => import_external(source_id, expected_digest, target, conflict_policy, ctx),
        SkillMutationRequest::Remove {
            skill_id,
            expected_digest,
        } => remove_skill(skill_id, expected_digest, ctx),
        SkillMutationRequest::RemoveByName {
            name,
            scope,
            expected_digest,
        } => {
            let resolved = resolve_owned_skill_by_name(ctx, &name, scope)?;
            remove_skill(resolved.id, expected_digest.or(Some(resolved.digest)), ctx)
        }
        SkillMutationRequest::Trust {
            skill_id,
            expected_digest,
        } => trust_skill(skill_id, expected_digest, ctx),
        SkillMutationRequest::TrustByName {
            name,
            scope,
            expected_digest,
        } => {
            let resolved = resolve_owned_skill_by_name(ctx, &name, scope)?;
            let digest = expected_digest.unwrap_or(resolved.digest);
            trust_skill(resolved.id, digest, ctx)
        }
        SkillMutationRequest::InstallRemote { .. }
        | SkillMutationRequest::Update { .. }
        | SkillMutationRequest::UpdateByName { .. } => {
            bail!("this mutation requires async execute (network I/O)")
        }
    }
}

#[derive(Debug)]
struct ResolvedOwnedSkill {
    id: AuditedSkillId,
    digest: String,
}

fn resolve_owned_skill_by_name(
    ctx: &MutationContext<'_>,
    name: &str,
    scope: Option<SkillTargetScope>,
) -> Result<ResolvedOwnedSkill> {
    // Match SkillRegistry::get — users may pass frontmatter/dir forms
    // (Hello_World) while audit stores the folded canonical key (hello-world).
    let canonical = normalize_skill_name_for_lookup(name);
    let snap = scan_with_configured(
        ctx.workspace,
        ctx.home,
        ctx.configured_skills_dir,
        SkillAuditMode::OwnedOnly,
        None,
    );
    let mut matches: Vec<&AuditedSkill> = snap
        .skills
        .iter()
        .filter(|s| s.id.canonical_name == canonical)
        .filter(|s| s.root.is_writable_owned())
        .collect();

    if let Some(scope) = scope {
        let want = match scope {
            SkillTargetScope::Project => SkillRootKind::CodeWhaleProject,
            SkillTargetScope::Global => SkillRootKind::CodeWhaleGlobal,
        };
        matches.retain(|s| s.root.kind == want);
    }

    match matches.as_slice() {
        [] => {
            // If the name only exists externally, tell the user to import.
            let compatible = scan_with_configured(
                ctx.workspace,
                ctx.home,
                ctx.configured_skills_dir,
                SkillAuditMode::Compatible,
                None,
            );
            if compatible.skills.iter().any(|s| {
                s.id.canonical_name == canonical
                    && s.source_kind == SkillSourceKind::CompatibleExternal
            }) {
                bail!(
                    "skill '{name}' exists only in a compatible external root; \
                     import it with /skills (refusing to write external harness directories)"
                );
            }
            bail!("skill '{name}' not found in CodeWhale-owned project/global roots");
        }
        [only] => {
            let DigestState::Known(digest) = &only.digest else {
                bail!("skill '{name}' has unknown package digest; refusing mutation");
            };
            Ok(ResolvedOwnedSkill {
                id: only.id.clone(),
                digest: digest.clone(),
            })
        }
        _ => bail!(
            "skill '{name}' exists in both project and global CodeWhale roots; \
             specify --project or --global"
        ),
    }
}

fn find_audited_skill(
    ctx: &MutationContext<'_>,
    skill_id: &AuditedSkillId,
) -> Result<(AuditedSkill, PathBuf)> {
    let snap = scan_with_configured(
        ctx.workspace,
        ctx.home,
        ctx.configured_skills_dir,
        SkillAuditMode::Compatible,
        None,
    );
    let skill = snap
        .skills
        .into_iter()
        .find(|s| &s.id == skill_id)
        .with_context(|| format!("audited skill {} not found", skill_id.canonical_name))?;
    let path = skill.root.path.join(&skill.id.relative_dir);
    Ok((skill, path))
}

/// Directory segment under the skills root. Prefer this over `canonical_name`
/// when calling install helpers that join `skills_dir / name` — installs use
/// raw frontmatter names, while audit stores a normalized lookup key.
fn on_disk_package_name(skill_id: &AuditedSkillId) -> Result<&str> {
    let name = skill_id
        .relative_dir
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .ok_or_else(|| {
            anyhow::anyhow!(
                "invalid on-disk package directory for skill '{}'",
                skill_id.canonical_name
            )
        })?;
    Ok(name)
}

fn verify_expected_digest(path: &Path, expected: Option<&str>) -> Result<Option<String>> {
    let current = package_digest::compute_package_digest(path)
        .with_context(|| format!("cannot digest {}", path.display()))?;
    if let Some(expected) = expected
        && expected != current
    {
        bail!(
            "skill content changed since audit (expected {expected}, found {current}); \
             re-review before mutating"
        );
    }
    Ok(Some(current))
}

fn ensure_remote_updatable(skill_dir: &Path) -> Result<()> {
    let marker_path = skill_dir.join(install::INSTALLED_FROM_MARKER);
    let body = fs::read_to_string(&marker_path)
        .with_context(|| format!("failed to read {}", marker_path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&body)
        .with_context(|| format!("malformed {}", install::INSTALLED_FROM_MARKER))?;
    let spec = value
        .get("spec")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if !install::is_registry_updatable_spec(spec) {
        bail!(
            "skill was imported locally (spec '{spec}') and cannot be updated from a registry; \
             re-import or remove it first"
        );
    }
    Ok(())
}

async fn install_remote(
    source: InstallSource,
    target: SkillTargetScope,
    ctx: &MutationContext<'_>,
) -> Result<SkillMutationReceipt> {
    let skills_dir = resolve_owned_target(ctx.workspace, ctx.home, target)?;
    fs::create_dir_all(&skills_dir)
        .with_context(|| format!("failed to create {}", skills_dir.display()))?;

    let outcome = install::install_with_registry(
        source,
        &skills_dir,
        ctx.max_size,
        ctx.network,
        false,
        ctx.registry_url,
    )
    .await?;

    match outcome {
        InstallOutcome::Installed(installed) => {
            let after = package_digest::compute_package_digest(&installed.path).ok();
            Ok(SkillMutationReceipt {
                action: SkillActionKind::Install,
                name: installed.name,
                scope: target.as_skill_scope(),
                safe_target_path: safe_display_path(&installed.path, Some(ctx.workspace), ctx.home),
                before_digest: None,
                after_digest: after,
                outcome: SkillMutationOutcome::Installed,
            })
        }
        InstallOutcome::NeedsApproval(host) => Ok(SkillMutationReceipt {
            action: SkillActionKind::Install,
            name: String::new(),
            scope: target.as_skill_scope(),
            safe_target_path: safe_display_path(&skills_dir, Some(ctx.workspace), ctx.home),
            before_digest: None,
            after_digest: None,
            outcome: SkillMutationOutcome::NeedsApproval(host),
        }),
        InstallOutcome::NetworkDenied(host) => Ok(SkillMutationReceipt {
            action: SkillActionKind::Install,
            name: String::new(),
            scope: target.as_skill_scope(),
            safe_target_path: safe_display_path(&skills_dir, Some(ctx.workspace), ctx.home),
            before_digest: None,
            after_digest: None,
            outcome: SkillMutationOutcome::NetworkDenied(host),
        }),
    }
}

fn import_external(
    source_id: AuditedSkillId,
    expected_digest: String,
    target: SkillTargetScope,
    conflict_policy: ConflictPolicy,
    ctx: &MutationContext<'_>,
) -> Result<SkillMutationReceipt> {
    let (source_skill, source_path) = find_audited_skill(ctx, &source_id)?;
    if source_skill.source_kind != SkillSourceKind::CompatibleExternal {
        bail!("import source must be a compatible external skill");
    }
    if source_skill.path_unsafe {
        bail!("refusing to import unsafe external skill package");
    }

    let owned_snap = scan_with_configured(
        ctx.workspace,
        ctx.home,
        ctx.configured_skills_dir,
        SkillAuditMode::OwnedOnly,
        None,
    );
    let owned_has_name = owned_snap
        .skills
        .iter()
        .any(|s| s.id.canonical_name == source_id.canonical_name);

    if !owned_has_name && !source_skill.import_candidate {
        bail!("skill is not an import candidate");
    }

    let before = verify_expected_digest(&source_path, Some(&expected_digest))?;

    let skills_dir = resolve_owned_target(ctx.workspace, ctx.home, target)?;
    fs::create_dir_all(&skills_dir)
        .with_context(|| format!("failed to create {}", skills_dir.display()))?;

    let want_kind = match target {
        SkillTargetScope::Project => SkillRootKind::CodeWhaleProject,
        SkillTargetScope::Global => SkillRootKind::CodeWhaleGlobal,
    };
    let existing_in_target: Vec<&AuditedSkill> = owned_snap
        .skills
        .iter()
        .filter(|s| s.id.canonical_name == source_id.canonical_name)
        .filter(|s| s.root.kind == want_kind)
        .collect();

    // Prefer the on-disk directory of an existing same-scope package so we
    // replace/reject against Hello_World rather than creating a parallel
    // hello-world next to it.
    let (dest, dest_segment) = match existing_in_target.as_slice() {
        [] => {
            let segment = source_id.canonical_name.clone();
            (skills_dir.join(&segment), segment)
        }
        [only] => {
            let segment = on_disk_package_name(&only.id)?.to_string();
            (only.root.path.join(&only.id.relative_dir), segment)
        }
        _ => bail!(
            "skill '{}' has multiple owned packages in the target scope; \
             remove or consolidate them before importing",
            source_id.canonical_name
        ),
    };

    if dest.exists() {
        let existing = package_digest::compute_package_digest(&dest).ok();
        if existing.as_deref() == Some(expected_digest.as_str()) {
            return Ok(SkillMutationReceipt {
                action: SkillActionKind::Import,
                name: source_id.canonical_name.clone(),
                scope: target.as_skill_scope(),
                safe_target_path: safe_display_path(&dest, Some(ctx.workspace), ctx.home),
                before_digest: before,
                after_digest: existing,
                outcome: SkillMutationOutcome::AlreadyPresent,
            });
        }
        if conflict_policy == ConflictPolicy::Reject {
            bail!(
                "destination {} already exists with different content",
                dest.display()
            );
        }
    }

    // Re-verify source digest immediately before copy (TOCTOU).
    let _ = verify_expected_digest(&source_path, Some(&expected_digest))?;

    let staging = skills_dir.join(format!("{dest_segment}.tmp"));
    if staging.exists() {
        fs::remove_dir_all(&staging).ok();
    }
    copy_skill_package(&source_path, &staging)?;
    package_digest::compute_package_digest(&staging)
        .context("staged import package failed digest validation")?;

    let mut backup_path: Option<PathBuf> = None;
    if dest.exists() {
        let backup = skills_dir.join(format!("{dest_segment}.bak"));
        if backup.exists() {
            fs::remove_dir_all(&backup).ok();
        }
        fs::rename(&dest, &backup)
            .with_context(|| format!("failed to backup {}", dest.display()))?;
        if let Err(err) = fs::rename(&staging, &dest) {
            let _ = fs::rename(&backup, &dest);
            let _ = fs::remove_dir_all(&staging);
            return Err(err).context("failed to replace destination with imported skill");
        }
        backup_path = Some(backup);
    } else if let Err(err) = fs::rename(&staging, &dest) {
        let _ = fs::remove_dir_all(&staging);
        return Err(err).context("failed to install imported skill");
    }

    // Keep backup until digest + marker finalize succeed so a failed import
    // can restore the previous owned skill.
    let finalize = (|| -> Result<String> {
        let _ = verify_expected_digest(&source_path, Some(&expected_digest))?;
        let after = package_digest::compute_package_digest(&dest)?;
        if after != expected_digest {
            bail!("imported content digest mismatch after copy; aborted");
        }
        write_installed_from_v2(
            &dest,
            &format!("import:{}", source_skill.safe_display_path),
            None,
            &expected_digest,
            &after,
            &source_id.canonical_name,
        )?;
        Ok(after)
    })();

    let after = match finalize {
        Ok(after) => {
            if let Some(backup) = backup_path.take() {
                fs::remove_dir_all(&backup).ok();
            }
            after
        }
        Err(err) => {
            let _ = fs::remove_dir_all(&dest);
            if let Some(backup) = backup_path.take() {
                let _ = fs::rename(&backup, &dest);
            }
            return Err(err);
        }
    };

    Ok(SkillMutationReceipt {
        action: SkillActionKind::Import,
        name: source_id.canonical_name,
        scope: target.as_skill_scope(),
        safe_target_path: safe_display_path(&dest, Some(ctx.workspace), ctx.home),
        before_digest: before,
        after_digest: Some(after),
        outcome: SkillMutationOutcome::Imported,
    })
}

fn copy_skill_package(src: &Path, dest: &Path) -> Result<()> {
    // Fail closed: source must already pass package digest (no symlinks).
    package_digest::compute_package_digest(src).context("source package is not safe to copy")?;
    copy_dir_regular_files(src, dest, src)?;
    Ok(())
}

fn copy_dir_regular_files(src: &Path, dest: &Path, package_root: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() {
            bail!("refusing to copy symlink {}", path.display());
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str == install::INSTALLED_FROM_MARKER
            || name_str == install::TRUSTED_MARKER
            || name_str == ".system-installed-version"
        {
            continue;
        }
        let target = dest.join(&name);
        if meta.is_dir() {
            if name_str.starts_with('.') {
                continue;
            }
            copy_dir_regular_files(&path, &target, package_root)?;
        } else if meta.is_file() {
            if name_str.starts_with('.') {
                continue;
            }
            fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

async fn update_skill(
    skill_id: AuditedSkillId,
    expected_digest: Option<String>,
    ctx: &MutationContext<'_>,
) -> Result<SkillMutationReceipt> {
    let (skill, path) = find_audited_skill(ctx, &skill_id)?;
    if !skill.root.is_writable_owned() {
        bail!("refusing to update skill outside CodeWhale-owned roots");
    }
    if skill.source_kind != SkillSourceKind::CodeWhaleManaged {
        bail!("only CodeWhale managed skills can be updated");
    }
    // Imported skills carry `import:…` provenance and must not hit the registry.
    ensure_remote_updatable(&path)?;
    let before = verify_expected_digest(&path, expected_digest.as_deref())?;
    let skills_dir = skill.root.path.clone();
    let scope = match skill.root.kind {
        SkillRootKind::CodeWhaleProject => SkillScope::Project,
        SkillRootKind::CodeWhaleGlobal => SkillScope::Global,
        _ => SkillScope::Logical,
    };

    let package_name = on_disk_package_name(&skill_id)?;
    let outcome = install::update_with_registry(
        package_name,
        &skills_dir,
        ctx.max_size,
        ctx.network,
        ctx.registry_url,
    )
    .await?;

    match outcome {
        UpdateResult::NoChange => Ok(SkillMutationReceipt {
            action: SkillActionKind::Update,
            name: skill_id.canonical_name,
            scope,
            safe_target_path: safe_display_path(&path, Some(ctx.workspace), ctx.home),
            before_digest: before.clone(),
            after_digest: before,
            outcome: SkillMutationOutcome::NoChange,
        }),
        UpdateResult::Updated(installed) => {
            let after = package_digest::compute_package_digest(&installed.path).ok();
            Ok(SkillMutationReceipt {
                action: SkillActionKind::Update,
                name: installed.name,
                scope,
                safe_target_path: safe_display_path(&installed.path, Some(ctx.workspace), ctx.home),
                before_digest: before,
                after_digest: after,
                outcome: SkillMutationOutcome::Updated,
            })
        }
        UpdateResult::NeedsApproval(host) => Ok(SkillMutationReceipt {
            action: SkillActionKind::Update,
            name: skill_id.canonical_name,
            scope,
            safe_target_path: safe_display_path(&path, Some(ctx.workspace), ctx.home),
            before_digest: before,
            after_digest: None,
            outcome: SkillMutationOutcome::NeedsApproval(host),
        }),
        UpdateResult::NetworkDenied(host) => Ok(SkillMutationReceipt {
            action: SkillActionKind::Update,
            name: skill_id.canonical_name,
            scope,
            safe_target_path: safe_display_path(&path, Some(ctx.workspace), ctx.home),
            before_digest: before,
            after_digest: None,
            outcome: SkillMutationOutcome::NetworkDenied(host),
        }),
    }
}

fn remove_skill(
    skill_id: AuditedSkillId,
    expected_digest: Option<String>,
    ctx: &MutationContext<'_>,
) -> Result<SkillMutationReceipt> {
    let (skill, path) = find_audited_skill(ctx, &skill_id)?;
    if !skill.root.is_writable_owned() {
        bail!("refusing to remove skill outside CodeWhale-owned roots");
    }
    if skill.source_kind != SkillSourceKind::CodeWhaleManaged {
        bail!("only CodeWhale managed skills can be removed");
    }
    let before = verify_expected_digest(&path, expected_digest.as_deref())?;
    let scope = match skill.root.kind {
        SkillRootKind::CodeWhaleProject => SkillScope::Project,
        SkillRootKind::CodeWhaleGlobal => SkillScope::Global,
        _ => SkillScope::Logical,
    };
    let package_name = on_disk_package_name(&skill_id)?;
    install::uninstall(package_name, &skill.root.path)?;
    Ok(SkillMutationReceipt {
        action: SkillActionKind::Remove,
        name: skill_id.canonical_name,
        scope,
        safe_target_path: safe_display_path(&path, Some(ctx.workspace), ctx.home),
        before_digest: before,
        after_digest: None,
        outcome: SkillMutationOutcome::Removed,
    })
}

fn trust_skill(
    skill_id: AuditedSkillId,
    expected_digest: String,
    ctx: &MutationContext<'_>,
) -> Result<SkillMutationReceipt> {
    let (skill, path) = find_audited_skill(ctx, &skill_id)?;
    if !skill.root.is_writable_owned() {
        bail!("refusing to trust skill outside CodeWhale-owned roots");
    }
    if skill.source_kind != SkillSourceKind::CodeWhaleManaged {
        bail!("only CodeWhale managed skills can be trusted");
    }
    let before = verify_expected_digest(&path, Some(&expected_digest))?;
    write_trust_v2(&path, &expected_digest)?;
    let scope = match skill.root.kind {
        SkillRootKind::CodeWhaleProject => SkillScope::Project,
        SkillRootKind::CodeWhaleGlobal => SkillScope::Global,
        _ => SkillScope::Logical,
    };
    Ok(SkillMutationReceipt {
        action: SkillActionKind::Trust,
        name: skill_id.canonical_name,
        scope,
        safe_target_path: safe_display_path(&path, Some(ctx.workspace), ctx.home),
        before_digest: before.clone(),
        after_digest: before,
        outcome: SkillMutationOutcome::Trusted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network_policy::NetworkPolicy;
    use tempfile::TempDir;

    fn write_skill(dir: &Path, name: &str, body: &str) {
        let skill_dir = dir.join(name);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: d\n---\n{body}\n"),
        )
        .unwrap();
    }

    fn ctx<'a>(
        workspace: &'a Path,
        home: &'a Path,
        network: &'a NetworkPolicy,
    ) -> MutationContext<'a> {
        MutationContext {
            workspace,
            home: Some(home),
            configured_skills_dir: None,
            network,
            max_size: install::DEFAULT_MAX_SIZE_BYTES,
            registry_url: install::DEFAULT_REGISTRY_URL,
        }
    }

    #[test]
    fn resolve_owned_target_project_and_global() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        let home = tmp.path().join("home");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&home).unwrap();

        let project =
            resolve_owned_target(&workspace, Some(&home), SkillTargetScope::Project).unwrap();
        let global =
            resolve_owned_target(&workspace, Some(&home), SkillTargetScope::Global).unwrap();
        assert_eq!(project, workspace.join(".codewhale").join("skills"));
        assert_eq!(global, home.join(".codewhale").join("skills"));
    }

    #[test]
    fn remove_and_trust_require_managed_owned() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        let home = tmp.path().join("home");
        let network = NetworkPolicy::default();
        let c = ctx(&workspace, &home, &network);

        write_skill(
            &workspace.join(".codewhale").join("skills"),
            "manual",
            "body",
        );
        let err = resolve_owned_skill_by_name(&c, "manual", Some(SkillTargetScope::Project));
        // Manual skill resolves, but remove/trust should fail source_kind check.
        let resolved = err.unwrap();
        let remove = remove_skill(resolved.id.clone(), Some(resolved.digest.clone()), &c);
        assert!(remove.is_err());

        write_skill(&workspace.join(".claude").join("skills"), "ext", "body");
        // Place sentinel in external root.
        let sentinel = workspace.join(".claude").join("skills").join("SENTINEL");
        fs::write(&sentinel, "do-not-touch").unwrap();

        let err = resolve_owned_skill_by_name(&c, "ext", None);
        assert!(err.unwrap_err().to_string().contains("compatible external"));
        assert_eq!(fs::read_to_string(&sentinel).unwrap(), "do-not-touch");
    }

    #[test]
    fn import_external_copies_into_project_owned() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        let home = tmp.path().join("home");
        let network = NetworkPolicy::default();
        let c = ctx(&workspace, &home, &network);

        fs::create_dir_all(workspace.join(".codewhale").join("skills")).unwrap();
        write_skill(
            &workspace.join(".claude").join("skills"),
            "from-claude",
            "import-me",
        );
        let sentinel = workspace.join(".claude").join("skills").join("SENTINEL");
        fs::write(&sentinel, "keep").unwrap();

        let snap = scan_with_configured(
            &workspace,
            Some(&home),
            None,
            SkillAuditMode::Compatible,
            None,
        );
        let external = snap
            .skills
            .iter()
            .find(|s| s.name == "from-claude")
            .unwrap();
        let DigestState::Known(digest) = &external.digest else {
            panic!("digest");
        };

        let receipt = import_external(
            external.id.clone(),
            digest.clone(),
            SkillTargetScope::Project,
            ConflictPolicy::Reject,
            &c,
        )
        .unwrap();
        assert_eq!(receipt.outcome, SkillMutationOutcome::Imported);
        assert!(
            workspace
                .join(".codewhale")
                .join("skills")
                .join("from-claude")
                .join("SKILL.md")
                .is_file()
        );
        assert_eq!(fs::read_to_string(&sentinel).unwrap(), "keep");
        // Source untouched.
        assert!(
            workspace
                .join(".claude")
                .join("skills")
                .join("from-claude")
                .join("SKILL.md")
                .is_file()
        );
    }

    #[test]
    fn import_exact_duplicate_is_already_present() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        let home = tmp.path().join("home");
        let network = NetworkPolicy::default();
        let c = ctx(&workspace, &home, &network);

        let content = "---\nname: shared\ndescription: d\n---\nbody\n";
        let owned = workspace.join(".codewhale").join("skills").join("shared");
        let external = workspace.join(".claude").join("skills").join("shared");
        fs::create_dir_all(&owned).unwrap();
        fs::create_dir_all(&external).unwrap();
        fs::write(owned.join("SKILL.md"), content).unwrap();
        fs::write(external.join("SKILL.md"), content).unwrap();

        let snap = scan_with_configured(
            &workspace,
            Some(&home),
            None,
            SkillAuditMode::Compatible,
            None,
        );
        let external = snap
            .skills
            .iter()
            .find(|s| s.name == "shared" && s.source_kind == SkillSourceKind::CompatibleExternal)
            .unwrap();
        let DigestState::Known(digest) = &external.digest else {
            panic!("digest");
        };

        let receipt = import_external(
            external.id.clone(),
            digest.clone(),
            SkillTargetScope::Project,
            ConflictPolicy::Reject,
            &c,
        )
        .unwrap();
        assert_eq!(receipt.outcome, SkillMutationOutcome::AlreadyPresent);
    }

    #[test]
    fn trust_writes_v2_digest_binding() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        let home = tmp.path().join("home");
        let network = NetworkPolicy::default();
        let c = ctx(&workspace, &home, &network);
        let root = workspace.join(".codewhale").join("skills");
        write_skill(&root, "managed", "body");
        let digest = package_digest::compute_package_digest(&root.join("managed")).unwrap();
        write_installed_from_v2(
            &root.join("managed"),
            "github:o/r",
            None,
            "src",
            &digest,
            "managed",
        )
        .unwrap();

        let snap = scan_with_configured(
            &workspace,
            Some(&home),
            None,
            SkillAuditMode::OwnedOnly,
            None,
        );
        let skill = &snap.skills[0];
        let receipt = trust_skill(skill.id.clone(), digest.clone(), &c).unwrap();
        assert_eq!(receipt.outcome, SkillMutationOutcome::Trusted);
        let trust_body =
            fs::read_to_string(root.join("managed").join(install::TRUSTED_MARKER)).unwrap();
        assert!(trust_body.contains("schema_version"));
        assert!(trust_body.contains(&digest));
    }

    #[test]
    fn remove_managed_skill() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        let home = tmp.path().join("home");
        let network = NetworkPolicy::default();
        let c = ctx(&workspace, &home, &network);
        let root = workspace.join(".codewhale").join("skills");
        write_skill(&root, "managed", "body");
        let digest = package_digest::compute_package_digest(&root.join("managed")).unwrap();
        write_installed_from_v2(
            &root.join("managed"),
            "github:o/r",
            None,
            "src",
            &digest,
            "managed",
        )
        .unwrap();

        let snap = scan_with_configured(
            &workspace,
            Some(&home),
            None,
            SkillAuditMode::OwnedOnly,
            None,
        );
        let skill = &snap.skills[0];
        let receipt = remove_skill(skill.id.clone(), Some(digest), &c).unwrap();
        assert_eq!(receipt.outcome, SkillMutationOutcome::Removed);
        assert!(!root.join("managed").exists());
    }

    #[test]
    fn resolve_by_name_normalizes_like_registry() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        let home = tmp.path().join("home");
        let network = NetworkPolicy::default();
        let c = ctx(&workspace, &home, &network);
        let root = workspace.join(".codewhale").join("skills");
        write_skill(&root, "Hello_World", "body");
        let digest = package_digest::compute_package_digest(&root.join("Hello_World")).unwrap();
        write_installed_from_v2(
            &root.join("Hello_World"),
            "github:o/r",
            None,
            "src",
            &digest,
            "Hello_World",
        )
        .unwrap();

        let resolved =
            resolve_owned_skill_by_name(&c, "Hello_World", Some(SkillTargetScope::Project))
                .unwrap();
        assert_eq!(resolved.id.canonical_name, "hello-world");
        assert_eq!(
            resolve_owned_skill_by_name(&c, "hello-world", Some(SkillTargetScope::Project))
                .unwrap()
                .id,
            resolved.id
        );
    }

    #[test]
    fn remove_uses_on_disk_dir_not_canonical_name() {
        // Frontmatter/dir keep underscores+case; audit canonicalizes to dashes+lower.
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        let home = tmp.path().join("home");
        let network = NetworkPolicy::default();
        let c = ctx(&workspace, &home, &network);
        let root = workspace.join(".codewhale").join("skills");
        let dir_name = "Hello_World";
        write_skill(&root, dir_name, "body");
        let digest = package_digest::compute_package_digest(&root.join(dir_name)).unwrap();
        write_installed_from_v2(
            &root.join(dir_name),
            "github:o/r",
            None,
            "src",
            &digest,
            dir_name,
        )
        .unwrap();

        let snap = scan_with_configured(
            &workspace,
            Some(&home),
            None,
            SkillAuditMode::OwnedOnly,
            None,
        );
        let skill = snap
            .skills
            .iter()
            .find(|s| s.id.canonical_name == "hello-world")
            .expect("canonical name should fold Hello_World → hello-world");
        assert_ne!(skill.id.canonical_name, dir_name);
        assert_eq!(
            skill.id.relative_dir.file_name().unwrap().to_str().unwrap(),
            dir_name
        );

        let receipt = remove_skill(skill.id.clone(), Some(digest), &c).unwrap();
        assert_eq!(receipt.outcome, SkillMutationOutcome::Removed);
        assert!(!root.join(dir_name).exists());
    }

    #[test]
    fn imported_skill_rejects_registry_update() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        let home = tmp.path().join("home");
        let network = NetworkPolicy::default();
        let c = ctx(&workspace, &home, &network);

        fs::create_dir_all(workspace.join(".codewhale").join("skills")).unwrap();
        write_skill(
            &workspace.join(".claude").join("skills"),
            "from-claude",
            "import-me",
        );
        let snap = scan_with_configured(
            &workspace,
            Some(&home),
            None,
            SkillAuditMode::Compatible,
            None,
        );
        let external = snap
            .skills
            .iter()
            .find(|s| s.name == "from-claude")
            .unwrap();
        let DigestState::Known(digest) = &external.digest else {
            panic!("digest");
        };
        import_external(
            external.id.clone(),
            digest.clone(),
            SkillTargetScope::Project,
            ConflictPolicy::Reject,
            &c,
        )
        .unwrap();

        let owned = workspace
            .join(".codewhale")
            .join("skills")
            .join("from-claude");
        assert!(ensure_remote_updatable(&owned).is_err());
        assert!(!install::is_registry_updatable_spec(
            "import:<workspace>/.claude/skills/from-claude"
        ));
        assert!(install::is_registry_updatable_spec("github:owner/repo"));
    }

    #[test]
    fn import_rejects_when_owned_dir_differs_from_canonical() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        let home = tmp.path().join("home");
        let network = NetworkPolicy::default();
        let c = ctx(&workspace, &home, &network);

        let owned_root = workspace.join(".codewhale").join("skills");
        write_skill(&owned_root, "Hello_World", "original-owned");
        let original_digest =
            package_digest::compute_package_digest(&owned_root.join("Hello_World")).unwrap();
        write_installed_from_v2(
            &owned_root.join("Hello_World"),
            "github:o/r",
            None,
            "src",
            &original_digest,
            "Hello_World",
        )
        .unwrap();

        write_skill(
            &workspace.join(".claude").join("skills"),
            "Hello_World",
            "external-different",
        );
        let snap = scan_with_configured(
            &workspace,
            Some(&home),
            None,
            SkillAuditMode::Compatible,
            None,
        );
        let external = snap
            .skills
            .iter()
            .find(|s| {
                s.id.canonical_name == "hello-world"
                    && s.source_kind == SkillSourceKind::CompatibleExternal
            })
            .unwrap();
        let DigestState::Known(ext_digest) = &external.digest else {
            panic!("digest");
        };

        let err = import_external(
            external.id.clone(),
            ext_digest.clone(),
            SkillTargetScope::Project,
            ConflictPolicy::Reject,
            &c,
        );
        assert!(err.unwrap_err().to_string().contains("already exists"));
        assert!(owned_root.join("Hello_World").exists());
        assert!(!owned_root.join("hello-world").exists());
        let still =
            package_digest::compute_package_digest(&owned_root.join("Hello_World")).unwrap();
        assert_eq!(still, original_digest);

        let receipt = import_external(
            external.id.clone(),
            ext_digest.clone(),
            SkillTargetScope::Project,
            ConflictPolicy::ReplaceConfirmed,
            &c,
        )
        .unwrap();
        assert_eq!(receipt.outcome, SkillMutationOutcome::Imported);
        assert!(owned_root.join("Hello_World").exists());
        assert!(!owned_root.join("hello-world").exists());
        assert!(!owned_root.join("Hello_World.bak").exists());
        let after =
            package_digest::compute_package_digest(&owned_root.join("Hello_World")).unwrap();
        assert_eq!(after, *ext_digest);
    }

    #[test]
    fn import_conflict_reject_keeps_owned_and_replace_cleans_backup() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        let home = tmp.path().join("home");
        let network = NetworkPolicy::default();
        let c = ctx(&workspace, &home, &network);

        let owned_root = workspace.join(".codewhale").join("skills");
        write_skill(&owned_root, "shared", "original-owned");
        let original_digest =
            package_digest::compute_package_digest(&owned_root.join("shared")).unwrap();
        write_installed_from_v2(
            &owned_root.join("shared"),
            "github:o/r",
            None,
            "src",
            &original_digest,
            "shared",
        )
        .unwrap();

        write_skill(
            &workspace.join(".claude").join("skills"),
            "shared",
            "external-different",
        );
        let snap = scan_with_configured(
            &workspace,
            Some(&home),
            None,
            SkillAuditMode::Compatible,
            None,
        );
        let external = snap
            .skills
            .iter()
            .find(|s| s.name == "shared" && s.source_kind == SkillSourceKind::CompatibleExternal)
            .unwrap();
        let DigestState::Known(ext_digest) = &external.digest else {
            panic!("digest");
        };

        let err = import_external(
            external.id.clone(),
            ext_digest.clone(),
            SkillTargetScope::Project,
            ConflictPolicy::Reject,
            &c,
        );
        assert!(err.is_err());
        let still = package_digest::compute_package_digest(&owned_root.join("shared")).unwrap();
        assert_eq!(still, original_digest);

        let receipt = import_external(
            external.id.clone(),
            ext_digest.clone(),
            SkillTargetScope::Project,
            ConflictPolicy::ReplaceConfirmed,
            &c,
        )
        .unwrap();
        assert_eq!(receipt.outcome, SkillMutationOutcome::Imported);
        let after = package_digest::compute_package_digest(&owned_root.join("shared")).unwrap();
        assert_eq!(after, *ext_digest);
        assert!(!owned_root.join("shared.bak").exists());
    }

    #[test]
    fn digest_mismatch_rejects_remove() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        let home = tmp.path().join("home");
        let network = NetworkPolicy::default();
        let c = ctx(&workspace, &home, &network);
        let root = workspace.join(".codewhale").join("skills");
        write_skill(&root, "managed", "body");
        let digest = package_digest::compute_package_digest(&root.join("managed")).unwrap();
        write_installed_from_v2(
            &root.join("managed"),
            "github:o/r",
            None,
            "src",
            &digest,
            "managed",
        )
        .unwrap();

        let snap = scan_with_configured(
            &workspace,
            Some(&home),
            None,
            SkillAuditMode::OwnedOnly,
            None,
        );
        let err = remove_skill(snap.skills[0].id.clone(), Some("deadbeef".into()), &c);
        assert!(err.unwrap_err().to_string().contains("changed since audit"));
        assert!(root.join("managed").exists());
    }

    #[test]
    fn dual_scope_same_name_requires_explicit_scope() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        let home = tmp.path().join("home");
        let network = NetworkPolicy::default();
        let c = ctx(&workspace, &home, &network);

        write_skill(
            &workspace.join(".codewhale").join("skills"),
            "dup",
            "project",
        );
        write_skill(&home.join(".codewhale").join("skills"), "dup", "global");

        let err = resolve_owned_skill_by_name(&c, "dup", None).unwrap_err();
        assert!(
            err.to_string().contains("--project") && err.to_string().contains("--global"),
            "got: {err}"
        );

        let project =
            resolve_owned_skill_by_name(&c, "dup", Some(SkillTargetScope::Project)).unwrap();
        let global =
            resolve_owned_skill_by_name(&c, "dup", Some(SkillTargetScope::Global)).unwrap();
        assert_ne!(project.id.root_id, global.id.root_id);
        assert_eq!(project.id.canonical_name, "dup");
        assert_eq!(global.id.canonical_name, "dup");
    }

    #[test]
    fn uninstall_external_only_refuses_and_keeps_sentinel() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        let home = tmp.path().join("home");
        let network = NetworkPolicy::default();
        let c = ctx(&workspace, &home, &network);

        fs::create_dir_all(workspace.join(".codewhale").join("skills")).unwrap();
        write_skill(
            &workspace.join(".claude").join("skills"),
            "only-ext",
            "body",
        );
        let sentinel = workspace.join(".claude").join("skills").join("SENTINEL");
        fs::write(&sentinel, "untouched").unwrap();

        let err = resolve_owned_skill_by_name(&c, "only-ext", None).unwrap_err();
        assert!(
            err.to_string().contains("compatible external"),
            "got: {err}"
        );
        assert_eq!(fs::read_to_string(&sentinel).unwrap(), "untouched");
        assert!(
            workspace
                .join(".claude")
                .join("skills")
                .join("only-ext")
                .join("SKILL.md")
                .is_file()
        );
    }
}
