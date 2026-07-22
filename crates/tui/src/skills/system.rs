//! System-skill installer: bundles first-party skills and auto-installs them
//! on first launch.

use std::fs;
use std::path::Path;

/// Bundled catalog generation for the default CodeWhale skill pack (#4691).
const BUNDLED_SKILL_VERSION: &str = "5";

// ── system & extension (meta) ───────────────────────────────────────────────
const SKILL_CREATOR_BODY: &str = include_str!("../../assets/skills/skill-creator/SKILL.md");
const DELEGATE_BODY: &str = include_str!("../../assets/skills/delegate/SKILL.md");
const PLUGIN_CREATOR_BODY: &str = include_str!("../../assets/skills/plugin-creator/SKILL.md");
const SKILL_INSTALLER_BODY: &str = include_str!("../../assets/skills/skill-installer/SKILL.md");
const MCP_BUILDER_BODY: &str = include_str!("../../assets/skills/mcp-builder/SKILL.md");
const FLEET_MANAGER_BODY: &str = include_str!("../../assets/skills/fleet-manager/SKILL.md");

// ── end-user workflows ──────────────────────────────────────────────────────
const INTERVIEW_BODY: &str = include_str!("../../assets/skills/interview/SKILL.md");
const PLAN_BODY: &str = include_str!("../../assets/skills/plan/SKILL.md");
const IMPLEMENT_BODY: &str = include_str!("../../assets/skills/implement/SKILL.md");
const DEBUG_BODY: &str = include_str!("../../assets/skills/debug/SKILL.md");
const TEST_BODY: &str = include_str!("../../assets/skills/test/SKILL.md");
const REVIEW_BODY: &str = include_str!("../../assets/skills/review/SKILL.md");
const SECURITY_REVIEW_BODY: &str = include_str!("../../assets/skills/security-review/SKILL.md");
const SIMPLIFY_BODY: &str = include_str!("../../assets/skills/simplify/SKILL.md");
const VERIFY_BODY: &str = include_str!("../../assets/skills/verify/SKILL.md");
const RESEARCH_BODY: &str = include_str!("../../assets/skills/research/SKILL.md");
const FRONTEND_DESIGN_BODY: &str = include_str!("../../assets/skills/frontend-design/SKILL.md");
const WEBAPP_TESTING_BODY: &str = include_str!("../../assets/skills/webapp-testing/SKILL.md");
const DOCUMENT_BODY: &str = include_str!("../../assets/skills/document/SKILL.md");
const DATAVIZ_BODY: &str = include_str!("../../assets/skills/dataviz/SKILL.md");
const DOCX_BODY: &str = include_str!("../../assets/skills/docx/SKILL.md");
const PDF_BODY: &str = include_str!("../../assets/skills/pdf/SKILL.md");
const PPTX_BODY: &str = include_str!("../../assets/skills/pptx/SKILL.md");
const XLSX_BODY: &str = include_str!("../../assets/skills/xlsx/SKILL.md");
const DOCUMENTS_ALIAS_BODY: &str = include_str!("../../assets/skills/documents/SKILL.md");
const PRESENTATIONS_ALIAS_BODY: &str = include_str!("../../assets/skills/presentations/SKILL.md");
const SPREADSHEETS_ALIAS_BODY: &str = include_str!("../../assets/skills/spreadsheets/SKILL.md");

// ── power / explicit-only ───────────────────────────────────────────────────
const BATCH_BODY: &str = include_str!("../../assets/skills/batch/SKILL.md");
const DEPENDENCY_UPDATE_BODY: &str = include_str!("../../assets/skills/dependency-update/SKILL.md");
const RELEASE_BODY: &str = include_str!("../../assets/skills/release/SKILL.md");

// Optional integration (not auto-installed for every user): Feishu body kept for
// digest/migration helpers only.
const FEISHU_BODY: &str = include_str!("../../assets/skills/feishu/SKILL.md");

// Legacy v4 body retained solely for digest-based safe retirement (#4691).
const V4_BEST_PRACTICES_BODY: &str = include_str!("../../assets/skills/v4-best-practices/SKILL.md");

struct BundledSkill {
    name: &'static str,
    body: &'static str,
    introduced_in: u32,
}

/// Skills auto-installed for every user on fresh install / upgrade.
const BUNDLED_SKILLS: &[BundledSkill] = &[
    // System & extension
    BundledSkill {
        name: "skill-creator",
        body: SKILL_CREATOR_BODY,
        introduced_in: 1,
    },
    BundledSkill {
        name: "delegate",
        body: DELEGATE_BODY,
        introduced_in: 2,
    },
    BundledSkill {
        name: "plugin-creator",
        body: PLUGIN_CREATOR_BODY,
        introduced_in: 3,
    },
    BundledSkill {
        name: "skill-installer",
        body: SKILL_INSTALLER_BODY,
        introduced_in: 3,
    },
    BundledSkill {
        name: "mcp-builder",
        body: MCP_BUILDER_BODY,
        introduced_in: 3,
    },
    BundledSkill {
        name: "fleet-manager",
        body: FLEET_MANAGER_BODY,
        introduced_in: 4,
    },
    // End-user workflows (v5)
    BundledSkill {
        name: "interview",
        body: INTERVIEW_BODY,
        introduced_in: 5,
    },
    BundledSkill {
        name: "plan",
        body: PLAN_BODY,
        introduced_in: 5,
    },
    BundledSkill {
        name: "implement",
        body: IMPLEMENT_BODY,
        introduced_in: 5,
    },
    BundledSkill {
        name: "debug",
        body: DEBUG_BODY,
        introduced_in: 5,
    },
    BundledSkill {
        name: "test",
        body: TEST_BODY,
        introduced_in: 5,
    },
    BundledSkill {
        name: "review",
        body: REVIEW_BODY,
        introduced_in: 5,
    },
    BundledSkill {
        name: "security-review",
        body: SECURITY_REVIEW_BODY,
        introduced_in: 5,
    },
    BundledSkill {
        name: "simplify",
        body: SIMPLIFY_BODY,
        introduced_in: 5,
    },
    BundledSkill {
        name: "verify",
        body: VERIFY_BODY,
        introduced_in: 5,
    },
    BundledSkill {
        name: "research",
        body: RESEARCH_BODY,
        introduced_in: 5,
    },
    BundledSkill {
        name: "frontend-design",
        body: FRONTEND_DESIGN_BODY,
        introduced_in: 5,
    },
    BundledSkill {
        name: "webapp-testing",
        body: WEBAPP_TESTING_BODY,
        introduced_in: 5,
    },
    BundledSkill {
        name: "document",
        body: DOCUMENT_BODY,
        introduced_in: 5,
    },
    BundledSkill {
        name: "dataviz",
        body: DATAVIZ_BODY,
        introduced_in: 5,
    },
    BundledSkill {
        name: "docx",
        body: DOCX_BODY,
        introduced_in: 5,
    },
    BundledSkill {
        name: "pdf",
        body: PDF_BODY,
        introduced_in: 3,
    },
    BundledSkill {
        name: "pptx",
        body: PPTX_BODY,
        introduced_in: 5,
    },
    BundledSkill {
        name: "xlsx",
        body: XLSX_BODY,
        introduced_in: 5,
    },
    // Compatibility aliases for pre-v5 artifact names
    BundledSkill {
        name: "documents",
        body: DOCUMENTS_ALIAS_BODY,
        introduced_in: 3,
    },
    BundledSkill {
        name: "presentations",
        body: PRESENTATIONS_ALIAS_BODY,
        introduced_in: 3,
    },
    BundledSkill {
        name: "spreadsheets",
        body: SPREADSHEETS_ALIAS_BODY,
        introduced_in: 3,
    },
    // Power / explicit-only
    BundledSkill {
        name: "batch",
        body: BATCH_BODY,
        introduced_in: 5,
    },
    BundledSkill {
        name: "dependency-update",
        body: DEPENDENCY_UPDATE_BODY,
        introduced_in: 5,
    },
    BundledSkill {
        name: "release",
        body: RELEASE_BODY,
        introduced_in: 5,
    },
];

/// Legacy v4-best-practices body digest helper (not in BUNDLED_SKILLS).
fn v4_best_practices_body() -> &'static str {
    V4_BEST_PRACTICES_BODY
}

fn feishu_body() -> &'static str {
    FEISHU_BODY
}

/// Whether a skill name matches one of the bundled first-party skills.
///
/// Used by `/skills` to distinguish user-created skills (which should be
/// surfaced prominently) from the always-installed bundle (which can be
/// rendered compactly when many skills are present).
///
/// Prefer [`is_exact_bundled_skill`] when classifying audit rows — name-only
/// matches can collide with user overrides of the same command name.
#[must_use]
pub fn is_bundled_skill_name(name: &str) -> bool {
    BUNDLED_SKILLS.iter().any(|s| s.name == name)
}

/// True when `name` is a bundled skill **and** `skill_md_content` exactly
/// matches the shipped asset body (byte-for-byte).
///
/// Used by the skill audit inventory so a user-edited copy of a bundled name
/// is not misclassified as built-in.
#[must_use]
pub fn is_exact_bundled_skill(name: &str, skill_md_content: &str) -> bool {
    BUNDLED_SKILLS
        .iter()
        .any(|s| s.name == name && s.body == skill_md_content)
}

/// SHA-256 (hex) of the shipped `SKILL.md` body for a bundled skill, if any.
#[must_use]
#[allow(dead_code)] // available for managers / docs that prefer digest over body compare
pub fn bundled_skill_body_sha256(name: &str) -> Option<String> {
    use sha2::{Digest, Sha256};
    BUNDLED_SKILLS.iter().find(|s| s.name == name).map(|s| {
        let digest = Sha256::digest(s.body.as_bytes());
        let mut out = String::with_capacity(digest.len() * 2);
        for byte in digest {
            use std::fmt::Write as _;
            let _ = write!(&mut out, "{byte:02x}");
        }
        out
    })
}

/// Attempt to install a single bundled skill into `skills_dir`.
///
/// Returns `true` if installation occurred (fresh install or version bump).
fn install_one(
    skills_dir: &Path,
    skill: &BundledSkill,
    installed_version: Option<&str>,
) -> std::io::Result<bool> {
    let target_dir = skills_dir.join(skill.name);
    let target_file = target_dir.join("SKILL.md");
    let dir_exists = target_dir.exists();
    let installed_number = installed_version.and_then(|value| value.parse::<u32>().ok());

    let should_install = match (installed_version, installed_number, dir_exists) {
        // Fresh install: neither marker nor directory.
        (None, _, false) => true,
        // Newly bundled skill: add it for older system-skill installs.
        (Some(_), Some(version), _) if version < skill.introduced_in => true,
        // Version bump for an existing skill: refresh only if the user has not
        // intentionally deleted that skill directory.
        (Some(version), _, true) if version != BUNDLED_SKILL_VERSION => true,
        // Every other case: current install, user-deleted dir, or pre-existing
        // user-owned skill without our marker.
        _ => false,
    };

    if should_install {
        // Never overwrite a user-modified copy that no longer matches a known
        // shipped body (#4691 non-destructive upgrade table).
        if target_file.exists() {
            let existing = fs::read_to_string(&target_file).unwrap_or_default();
            if !existing.is_empty() && existing != skill.body {
                // Preserve user/compatible-root content; skip replace-by-name.
                return Ok(false);
            }
        }
        fs::create_dir_all(&target_dir)?;
        fs::write(&target_file, skill.body)?;
    }
    Ok(should_install)
}

/// Install bundled system skills into `skills_dir`.
///
/// Behaviour:
/// - Fresh install (no marker, no dir): installs every bundled skill, then
///   writes the version marker.
/// - Version bump (marker present with older version): re-installs any existing
///   bundled skill and installs newly introduced bundled skills.
/// - User deleted a skill dir while marker still present at same version: leaves
///   it gone.
/// - Idempotent: calling twice with no changes is a no-op.
///
/// Errors are I/O errors from the filesystem; the caller should log them but not
/// abort startup.
pub fn install_system_skills(skills_dir: &Path) -> std::io::Result<()> {
    let marker = skills_dir.join(".system-installed-version");

    let installed_version = fs::read_to_string(&marker)
        .ok()
        .map(|s| s.trim().to_string());

    let mut changed = false;
    for skill in BUNDLED_SKILLS {
        changed |= install_one(skills_dir, skill, installed_version.as_deref())?;
    }

    // Safe retirement: remove only an unchanged CodeWhale-owned v4-best-practices.
    changed |= retire_unchanged_v4_best_practices(skills_dir)?;

    // Feishu is optional: do not install for every user. If an older bundle
    // installed an exact shipped copy, leave it; never delete by name alone.
    let _ = feishu_body();

    if changed {
        fs::create_dir_all(skills_dir)?;
        fs::write(&marker, BUNDLED_SKILL_VERSION)?;
    }
    Ok(())
}

/// Delete `v4-best-practices` only when the installed SKILL.md exactly matches
/// the last shipped bundled body (byte-for-byte). Modified or user-owned copies
/// are preserved.
fn retire_unchanged_v4_best_practices(skills_dir: &Path) -> std::io::Result<bool> {
    let dir = skills_dir.join("v4-best-practices");
    let file = dir.join("SKILL.md");
    if !file.exists() {
        return Ok(false);
    }
    let existing = fs::read_to_string(&file)?;
    if existing != v4_best_practices_body() {
        return Ok(false);
    }
    fs::remove_dir_all(&dir)?;
    Ok(true)
}

/// Remove all system skills and the version marker.
///
/// Intended for tests and `deepseek setup --clean`.  Ignores missing files.
#[allow(dead_code)]
pub fn uninstall_system_skills(skills_dir: &Path) -> std::io::Result<()> {
    let marker = skills_dir.join(".system-installed-version");

    for skill in BUNDLED_SKILLS {
        let dir = skills_dir.join(skill.name);
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
    }
    if marker.exists() {
        fs::remove_file(&marker)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn skill_file(tmp: &TempDir, name: &str) -> std::path::PathBuf {
        tmp.path().join(name).join("SKILL.md")
    }

    fn skill_dir(tmp: &TempDir, name: &str) -> std::path::PathBuf {
        tmp.path().join(name)
    }

    fn marker_file(tmp: &TempDir) -> std::path::PathBuf {
        tmp.path().join(".system-installed-version")
    }

    // ── fresh install ─────────────────────────────────────────────────────────

    #[test]
    fn fresh_install_creates_bundled_skills_and_marker() {
        let tmp = TempDir::new().unwrap();
        install_system_skills(tmp.path()).unwrap();

        for skill in BUNDLED_SKILLS {
            assert!(
                skill_file(&tmp, skill.name).exists(),
                "{} SKILL.md should be created",
                skill.name
            );
        }
        assert!(marker_file(&tmp).exists(), "marker should be created");

        let ver = fs::read_to_string(marker_file(&tmp)).unwrap();
        assert_eq!(ver.trim(), BUNDLED_SKILL_VERSION);
    }

    #[test]
    fn fresh_install_skills_parse_for_discovery() {
        let tmp = TempDir::new().unwrap();
        install_system_skills(tmp.path()).unwrap();

        let registry = crate::skills::SkillRegistry::discover(tmp.path());
        assert!(
            registry.warnings().is_empty(),
            "bundled skills should parse cleanly: {:?}",
            registry.warnings()
        );

        for skill in BUNDLED_SKILLS {
            let parsed = registry
                .get(skill.name)
                .unwrap_or_else(|| panic!("{} should be discoverable", skill.name));
            assert!(
                !parsed.description.is_empty(),
                "{} should include model-visible description",
                skill.name
            );
        }
    }

    // ── idempotence ───────────────────────────────────────────────────────────

    #[test]
    fn calling_twice_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        install_system_skills(tmp.path()).unwrap();

        for skill in BUNDLED_SKILLS {
            fs::write(
                skill_file(&tmp, skill.name),
                format!("{}-sentinel", skill.name),
            )
            .unwrap();
        }

        install_system_skills(tmp.path()).unwrap();

        for skill in BUNDLED_SKILLS {
            let body = fs::read_to_string(skill_file(&tmp, skill.name)).unwrap();
            assert_eq!(
                body,
                format!("{}-sentinel", skill.name),
                "second install should not overwrite {}",
                skill.name
            );
        }
    }

    // ── user deleted a directory ──────────────────────────────────────────────

    #[test]
    fn user_deleted_dir_is_not_recreated() {
        let tmp = TempDir::new().unwrap();
        install_system_skills(tmp.path()).unwrap();

        // Simulate user deliberately removing one skill directory.
        fs::remove_dir_all(skill_dir(&tmp, "delegate")).unwrap();

        // Re-launch must NOT recreate the deleted directory.
        install_system_skills(tmp.path()).unwrap();

        assert!(
            !skill_file(&tmp, "delegate").exists(),
            "delegate must not be recreated after user deleted it"
        );
        assert!(
            skill_file(&tmp, "skill-creator").exists(),
            "skill-creator should still be present (not deleted by user)"
        );
    }

    #[test]
    fn user_deleted_all_dirs_are_not_recreated() {
        let tmp = TempDir::new().unwrap();
        install_system_skills(tmp.path()).unwrap();

        for skill in BUNDLED_SKILLS {
            fs::remove_dir_all(skill_dir(&tmp, skill.name)).unwrap();
        }

        install_system_skills(tmp.path()).unwrap();

        for skill in BUNDLED_SKILLS {
            assert!(
                !skill_file(&tmp, skill.name).exists(),
                "{} must not be recreated after user deletion",
                skill.name
            );
        }
    }

    // ── version bump re-installs ──────────────────────────────────────────────

    #[test]
    fn outdated_marker_triggers_reinstall_of_existing_skills() {
        let tmp = TempDir::new().unwrap();
        // Exact shipped bodies present with old marker: refresh is allowed and
        // new v5 skills are added. Non-matching user content is preserved
        // elsewhere (see upgrade_preserves_user_modified_bundled_skill_body).
        for skill in BUNDLED_SKILLS.iter().filter(|s| s.introduced_in <= 4) {
            fs::create_dir_all(skill_dir(&tmp, skill.name)).unwrap();
            fs::write(skill_file(&tmp, skill.name), skill.body).unwrap();
        }
        fs::write(marker_file(&tmp), "0").unwrap();

        install_system_skills(tmp.path()).unwrap();

        for skill in BUNDLED_SKILLS {
            assert!(
                skill_file(&tmp, skill.name).exists(),
                "{} should be installed after marker upgrade",
                skill.name
            );
            let content = fs::read_to_string(skill_file(&tmp, skill.name)).unwrap();
            assert_eq!(
                content, skill.body,
                "{} body should match shipped",
                skill.name
            );
        }
        let ver = fs::read_to_string(marker_file(&tmp)).unwrap();
        assert_eq!(ver.trim(), BUNDLED_SKILL_VERSION);
    }

    // ── partial previous install ─────────────────────────────────────────────

    #[test]
    fn version_bump_adds_skills_introduced_after_marker() {
        let tmp = TempDir::new().unwrap();
        // Pre-v5 install: only skills introduced through v4, with exact bodies.
        for skill in BUNDLED_SKILLS.iter().filter(|s| s.introduced_in <= 4) {
            fs::create_dir_all(skill_dir(&tmp, skill.name)).unwrap();
            fs::write(skill_file(&tmp, skill.name), skill.body).unwrap();
        }
        fs::write(marker_file(&tmp), "4").unwrap();

        install_system_skills(tmp.path()).unwrap();

        for skill in BUNDLED_SKILLS.iter().filter(|s| s.introduced_in == 5) {
            assert!(
                skill_file(&tmp, skill.name).exists(),
                "v5 skill {} should be installed on upgrade",
                skill.name
            );
        }
        // Unchanged exact bodies remain current.
        for skill in BUNDLED_SKILLS.iter().filter(|s| s.introduced_in <= 4) {
            let content = fs::read_to_string(skill_file(&tmp, skill.name)).unwrap();
            assert_eq!(content, skill.body);
        }
        let ver = fs::read_to_string(marker_file(&tmp)).unwrap();
        assert_eq!(ver.trim(), BUNDLED_SKILL_VERSION);
    }

    #[test]
    fn version_bump_respects_deleted_existing_skill_while_adding_new_skill() {
        let tmp = TempDir::new().unwrap();

        // Simulate v2 where older bundled skills had been deliberately removed
        // before later versions introduced more system skills.
        fs::write(marker_file(&tmp), "2").unwrap();

        install_system_skills(tmp.path()).unwrap();

        assert!(
            !skill_file(&tmp, "skill-creator").exists(),
            "version bump should not recreate deleted skill-creator"
        );
        assert!(
            !skill_file(&tmp, "delegate").exists(),
            "version bump should not recreate deleted delegate"
        );
        for skill in BUNDLED_SKILLS
            .iter()
            .filter(|skill| skill.introduced_in > 2)
        {
            assert!(
                skill_file(&tmp, skill.name).exists(),
                "version bump should install newly introduced {}",
                skill.name
            );
        }
        let ver = fs::read_to_string(marker_file(&tmp)).unwrap();
        assert_eq!(ver.trim(), BUNDLED_SKILL_VERSION);
    }

    // ── uninstall ─────────────────────────────────────────────────────────────

    #[test]
    fn uninstall_removes_bundled_skills_and_marker() {
        let tmp = TempDir::new().unwrap();
        install_system_skills(tmp.path()).unwrap();
        uninstall_system_skills(tmp.path()).unwrap();

        for skill in BUNDLED_SKILLS {
            assert!(
                !skill_file(&tmp, skill.name).exists(),
                "{} should be removed",
                skill.name
            );
        }
        assert!(!marker_file(&tmp).exists(), "marker should be removed");
    }

    #[test]
    fn uninstall_on_clean_dir_is_a_noop() {
        let tmp = TempDir::new().unwrap();
        // Must not panic or error.
        uninstall_system_skills(tmp.path()).unwrap();
    }
    #[test]
    fn upgrade_from_v4_installs_pack_and_retires_unchanged_v4_best_practices() {
        let tmp = TempDir::new().unwrap();
        // Simulate a v4 install: marker + legacy skill bodies.
        fs::create_dir_all(skill_dir(&tmp, "v4-best-practices")).unwrap();
        fs::write(
            skill_file(&tmp, "v4-best-practices"),
            V4_BEST_PRACTICES_BODY,
        )
        .unwrap();
        fs::write(marker_file(&tmp), "4").unwrap();

        install_system_skills(tmp.path()).unwrap();

        assert!(
            !skill_dir(&tmp, "v4-best-practices").exists(),
            "unchanged v4-best-practices must be retired"
        );
        assert!(skill_file(&tmp, "debug").exists());
        assert!(skill_file(&tmp, "docx").exists());
        assert!(skill_file(&tmp, "release").exists());
        // Feishu is optional — not auto-installed by v5.
        assert!(
            !skill_dir(&tmp, "feishu").exists(),
            "feishu must not be universally installed"
        );
        let ver = fs::read_to_string(marker_file(&tmp)).unwrap();
        assert_eq!(ver.trim(), BUNDLED_SKILL_VERSION);
    }

    #[test]
    fn upgrade_preserves_modified_v4_best_practices() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(skill_dir(&tmp, "v4-best-practices")).unwrap();
        fs::write(
            skill_file(&tmp, "v4-best-practices"),
            "---\nname: v4-best-practices\ndescription: user-owned\n---\n\n# mine\n",
        )
        .unwrap();
        fs::write(marker_file(&tmp), "4").unwrap();

        install_system_skills(tmp.path()).unwrap();

        assert!(skill_dir(&tmp, "v4-best-practices").exists());
        let body = fs::read_to_string(skill_file(&tmp, "v4-best-practices")).unwrap();
        assert!(
            body.contains("user-owned"),
            "modified body must be preserved"
        );
    }

    #[test]
    fn upgrade_preserves_user_modified_bundled_skill_body() {
        let tmp = TempDir::new().unwrap();
        install_system_skills(tmp.path()).unwrap();
        let path = skill_file(&tmp, "debug");
        fs::write(
            &path,
            "---\nname: debug\ndescription: customized\n---\n\n# custom\n",
        )
        .unwrap();
        // Force version bump attempt
        fs::write(marker_file(&tmp), "4").unwrap();
        install_system_skills(tmp.path()).unwrap();
        let body = fs::read_to_string(path).unwrap();
        assert!(
            body.contains("customized"),
            "user edit must not be overwritten by name alone"
        );
    }

    #[test]
    fn end_user_pack_skills_parse_for_discovery() {
        let tmp = TempDir::new().unwrap();
        install_system_skills(tmp.path()).unwrap();
        let registry = crate::skills::SkillRegistry::discover(tmp.path());
        assert!(
            registry.warnings().is_empty(),
            "bundled skills should parse cleanly: {:?}",
            registry.warnings()
        );
        for name in [
            "debug", "test", "review", "document", "docx", "release", "plan", "verify",
        ] {
            assert!(registry.get(name).is_some(), "{name} must be discoverable");
        }
    }
}
