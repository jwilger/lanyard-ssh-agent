//! Regression tests for release and deployment configuration.

use std::{error::Error, fs, io, path::Path};

use serde_yaml::Value as Yaml;
use toml::Value as Toml;

const SHA_LENGTH: usize = 40;

fn read(path: &str) -> Result<String, Box<dyn Error>> {
    Ok(fs::read_to_string(path)?)
}

fn manifest() -> Result<Toml, Box<dyn Error>> {
    Ok(toml::from_str(&read("Cargo.toml")?)?)
}

fn release_plz_config() -> Result<Toml, Box<dyn Error>> {
    Ok(toml::from_str(&read("release-plz.toml")?)?)
}

fn workflow(path: &str) -> Result<Yaml, Box<dyn Error>> {
    Ok(serde_yaml::from_str(&read(path)?)?)
}

#[expect(
    clippy::pattern_type_mismatch,
    reason = "match ergonomics keep the recursive YAML traversal readable"
)]
fn values_for_key<'document>(
    document: &'document Yaml,
    wanted: &str,
    found: &mut Vec<&'document str>,
) {
    match document {
        Yaml::Mapping(mapping) => {
            for (key, nested) in mapping {
                if key.as_str() == Some(wanted)
                    && let Some(scalar) = nested.as_str()
                {
                    found.push(scalar);
                }
                values_for_key(nested, wanted, found);
            }
        }
        Yaml::Sequence(sequence) => {
            for nested in sequence {
                values_for_key(nested, wanted, found);
            }
        }
        Yaml::Null | Yaml::Bool(_) | Yaml::Number(_) | Yaml::String(_) | Yaml::Tagged(_) => {}
    }
}

fn assert_immutable_action_references(workflow: &Yaml) -> Result<(), Box<dyn Error>> {
    let mut references = Vec::new();
    values_for_key(workflow, "uses", &mut references);
    assert!(!references.is_empty(), "workflow must invoke an action");

    for reference in references {
        if reference.starts_with("./") {
            continue;
        }
        let (_, revision) = reference.rsplit_once('@').ok_or_else(|| {
            io::Error::other(format!("action reference has no revision: {reference}"))
        })?;
        assert_eq!(
            revision.len(),
            SHA_LENGTH,
            "action is not pinned: {reference}"
        );
        assert!(
            revision.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "action revision is not a commit SHA: {reference}"
        );
    }
    Ok(())
}

#[test]
fn dist_installer_verifies_pinned_archives_before_execution() -> Result<(), Box<dyn Error>> {
    let installer = workflow(".github/actions/install-dist/action.yml")?;
    let mut scripts = Vec::new();
    let mut arm64_digests = Vec::new();
    let mut x64_digests = Vec::new();
    values_for_key(&installer, "run", &mut scripts);
    values_for_key(&installer, "DIST_SHA256_ARM64", &mut arm64_digests);
    values_for_key(&installer, "DIST_SHA256_X64", &mut x64_digests);
    let script = scripts
        .first()
        .ok_or("dist installer must define an executable step")?;

    assert!(script.contains("sha256sum --check --status"));
    assert!(script.contains("DIST_SHA256_ARM64"));
    assert!(script.contains("DIST_SHA256_X64"));
    assert!(script.lines().all(|line| {
        let command = line.trim_end();
        !command.ends_with("| sh") && !command.ends_with("| bash")
    }));
    assert_eq!(arm64_digests.len(), 1);
    assert_eq!(x64_digests.len(), 1);
    for digest in arm64_digests.into_iter().chain(x64_digests) {
        assert_eq!(digest.len(), 64);
        assert!(digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }
    Ok(())
}

#[test]
fn dist_grants_repository_write_access_only_to_the_host_job() -> Result<(), Box<dyn Error>> {
    let release = workflow(".github/workflows/release.yml")?;
    let jobs = release
        .get("jobs")
        .ok_or("dist workflow must define jobs")?;

    assert_eq!(
        release
            .get("permissions")
            .and_then(|permissions| permissions.get("contents"))
            .and_then(Yaml::as_str),
        Some("read")
    );
    assert_eq!(
        jobs.get("host")
            .and_then(|host| host.get("permissions"))
            .and_then(|permissions| permissions.get("contents"))
            .and_then(Yaml::as_str),
        Some("write")
    );
    for job_name in [
        "plan",
        "build-local-artifacts",
        "build-global-artifacts",
        "announce",
    ] {
        assert!(
            jobs.get(job_name)
                .and_then(|job| job.get("env"))
                .and_then(|environment| environment.get("GH_TOKEN"))
                .is_none(),
            "{job_name} must not expose GH_TOKEN at job scope"
        );
    }
    Ok(())
}

#[test]
fn crate_manifest_is_ready_for_crates_io() -> Result<(), Box<dyn Error>> {
    let manifest = manifest()?;
    let package = manifest
        .get("package")
        .and_then(Toml::as_table)
        .ok_or("Cargo.toml must define a package")?;

    assert_eq!(
        package.get("name").and_then(Toml::as_str),
        Some("lanyard-ssh-agent")
    );
    assert_eq!(
        package.get("readme").and_then(Toml::as_str),
        Some("README.md")
    );
    assert_eq!(
        package.get("documentation").and_then(Toml::as_str),
        Some("https://docs.rs/lanyard-ssh-agent")
    );
    let included = package
        .get("include")
        .and_then(Toml::as_array)
        .ok_or("Cargo.toml must bound the published crate contents")?;
    assert!(
        included
            .iter()
            .all(|path| path.as_str() != Some("/site/**"))
    );
    assert!(Path::new("LICENSE-MIT").is_file());
    assert!(Path::new("LICENSE-APACHE").is_file());
    Ok(())
}

#[test]
fn release_plz_publishes_then_leaves_the_github_release_to_dist() -> Result<(), Box<dyn Error>> {
    let config = release_plz_config()?;
    let workspace = config
        .get("workspace")
        .and_then(Toml::as_table)
        .ok_or("release-plz must define workspace behavior")?;
    let package = config
        .get("package")
        .and_then(Toml::as_array)
        .and_then(|packages| packages.first())
        .and_then(Toml::as_table)
        .ok_or("release-plz must configure the distributable package")?;

    assert_eq!(workspace.get("publish").and_then(Toml::as_bool), Some(true));
    assert_eq!(
        workspace.get("git_tag_enable").and_then(Toml::as_bool),
        Some(true)
    );
    assert_eq!(
        workspace.get("git_release_enable").and_then(Toml::as_bool),
        Some(false)
    );
    assert_eq!(
        package.get("name").and_then(Toml::as_str),
        Some("lanyard-ssh-agent")
    );
    assert_eq!(
        package.get("git_tag_name").and_then(Toml::as_str),
        Some("v{{ version }}")
    );
    Ok(())
}

#[test]
fn dist_builds_checksummed_linux_archives() -> Result<(), Box<dyn Error>> {
    let manifest = manifest()?;
    let dist = manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("metadata"))
        .and_then(|metadata| metadata.get("dist"))
        .and_then(Toml::as_table)
        .ok_or("Cargo.toml must define dist metadata")?;
    let targets = dist
        .get("targets")
        .and_then(Toml::as_array)
        .ok_or("dist targets must be an array")?;

    assert_eq!(
        dist.get("cargo-dist-version").and_then(Toml::as_str),
        Some("0.32.0")
    );
    assert_eq!(dist.get("ci").and_then(Toml::as_str), Some("github"));
    assert_eq!(dist.get("checksum").and_then(Toml::as_str), Some("sha256"));
    assert!(
        targets
            .iter()
            .any(|target| target.as_str() == Some("x86_64-unknown-linux-gnu"))
    );
    assert!(
        targets
            .iter()
            .any(|target| target.as_str() == Some("aarch64-unknown-linux-gnu"))
    );
    Ok(())
}

#[test]
fn release_entrypoint_enables_pinned_shared_automation() -> Result<(), Box<dyn Error>> {
    let release_plz = workflow(".github/workflows/release-plz.yml")?;
    let dist = workflow(".github/workflows/release.yml")?;
    let mut conditions = Vec::new();
    let mut shared_workflows = Vec::new();
    values_for_key(&release_plz, "if", &mut conditions);
    values_for_key(&release_plz, "uses", &mut shared_workflows);

    assert_immutable_action_references(&release_plz)?;
    assert_immutable_action_references(&dist)?;
    assert!(conditions.is_empty());
    assert_eq!(
        shared_workflows,
        [
            "jwilger/gha-workflows/.github/workflows/rust-release-plz.yml@017ca09a54090c441a12a0234e7bd0f96d485b8f"
        ]
    );
    Ok(())
}

#[test]
fn release_guide_documents_the_enabled_shared_workflow() -> Result<(), Box<dyn Error>> {
    let guide = read("docs/releases.md")?;

    assert!(guide.contains("017ca09a54090c441a12a0234e7bd0f96d485b8f"));
    assert!(!guide.contains("RELEASE_WORKFLOW_NESTED_ACTIONS_PINNED"));
    Ok(())
}

#[test]
fn pages_builds_the_site_and_uses_official_pinned_deployment_actions() -> Result<(), Box<dyn Error>>
{
    let pages = workflow(".github/workflows/pages.yml")?;
    let mut working_directories = Vec::new();
    let mut artifact_paths = Vec::new();
    let mut pages_permissions = Vec::new();
    let mut identity_permissions = Vec::new();
    values_for_key(&pages, "working-directory", &mut working_directories);
    values_for_key(&pages, "path", &mut artifact_paths);
    values_for_key(&pages, "pages", &mut pages_permissions);
    values_for_key(&pages, "id-token", &mut identity_permissions);

    assert_immutable_action_references(&pages)?;
    assert_eq!(working_directories, ["site", "site"]);
    assert!(artifact_paths.contains(&"site/dist"));
    assert_eq!(pages_permissions, ["write"]);
    assert_eq!(identity_permissions, ["write"]);
    Ok(())
}
