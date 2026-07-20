//! Regression tests for release and deployment configuration.

use std::{
    env,
    error::Error,
    fs, io,
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    process::{Command, Output},
};

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

fn write_executable(path: &Path, source: &str) -> Result<(), Box<dyn Error>> {
    let bash = executable_on_path("bash")?;
    let rendered_source =
        source.replacen("#!/usr/bin/env bash", &format!("#!{}", bash.display()), 1);
    fs::write(path, rendered_source)?;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

fn executable_on_path(name: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = env::var_os("PATH").ok_or("PATH is unavailable")?;
    env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| format!("{name} is unavailable on PATH").into())
}

#[test]
fn generated_test_executables_do_not_require_usr_bin_env() -> Result<(), Box<dyn Error>> {
    let sandbox = tempfile::tempdir()?;
    let executable = sandbox.path().join("probe");
    write_executable(
        &executable,
        "#!/usr/bin/env bash\nprintf '%s\\n' portable\n",
    )?;

    let source = fs::read_to_string(&executable)?;
    assert!(!source.starts_with("#!/usr/bin/env"));
    let output = Command::new(executable).env_clear().output()?;
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout)?, "portable\n");
    Ok(())
}

#[expect(
    clippy::literal_string_with_formatting_args,
    reason = "the literal contains Bash parameter expansion, not Rust formatting"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "the arguments name independent release-state dimensions used by the scenario matrix"
)]
fn run_release_state(
    dirty: bool,
    crate_status: &str,
    github_release_status: &str,
    github_release_draft: &str,
    tag_exists: bool,
    tag_sha: &str,
    verify_fails: bool,
    recovery_tag: Option<&str>,
) -> Result<(Output, String, String), Box<dyn Error>> {
    let sandbox = tempfile::tempdir()?;
    let bin = sandbox.path().join("bin");
    fs::create_dir_all(&bin)?;
    let command_log = sandbox.path().join("commands.log");
    let github_output = sandbox.path().join("github-output");
    write_executable(
        &bin.join("git"),
        r#"#!/usr/bin/env bash
set -euo pipefail
printf 'git %s\n' "$*" >> "$COMMAND_LOG"
case "$*" in
  "diff --cached --quiet") test "${DIRTY:-0}" != 1 ;;
  "rev-parse --verify --quiet refs/tags/"*"^{commit}")
    test "${TAG_EXISTS:-0}" = 1 && printf '%s\n' "${TAG_SHA:-release-sha}"
    ;;
  "rev-parse refs/tags/"*"^{commit}") printf '%s\n' "${TAG_SHA:-release-sha}" ;;
  "rev-parse HEAD") printf '%s\n' release-sha ;;
  "verify-tag "*) test "${VERIFY_FAIL:-0}" != 1 ;;
esac
"#,
    )?;
    write_executable(
        &bin.join("cargo"),
        r#"#!/usr/bin/env bash
printf '%s\n' '{"packages":[{"name":"lanyard-ssh-agent","version":"1.2.3"}]}'
"#,
    )?;
    write_executable(
        &bin.join("curl"),
        r#"#!/usr/bin/env bash
printf 'curl %s\n' "$*" >> "$COMMAND_LOG"
if [[ "$*" == *api.github.com* ]]; then
  output=""
  while (($#)); do
    if [[ "$1" == --output ]]; then
      output="$2"
      break
    fi
    shift
  done
  printf '{"draft":%s}\n' "$GITHUB_RELEASE_DRAFT" > "$output"
  printf '%s' "$GITHUB_RELEASE_STATUS"
else
  printf '%s' "$CRATE_STATUS"
fi
"#,
    )?;
    write_executable(
        &bin.join("ssh-keygen"),
        "#!/usr/bin/env bash\nprintf '%s\\n' 'ssh-ed25519 AAAATEST'\n",
    )?;

    let path = format!(
        "{}:{}",
        bin.display(),
        env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_owned())
    );
    let output = Command::new("bash")
        .arg("scripts/prepare-release.sh")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("PATH", path)
        .env("COMMAND_LOG", &command_log)
        .env("GITHUB_OUTPUT", &github_output)
        .env("RUNNER_TEMP", sandbox.path())
        .env("DIRTY", if dirty { "1" } else { "0" })
        .env("CRATE_STATUS", crate_status)
        .env("GITHUB_RELEASE_STATUS", github_release_status)
        .env("GITHUB_RELEASE_DRAFT", github_release_draft)
        .env("GITHUB_REPOSITORY", "jwilger/lanyard-ssh-agent")
        .env("TAG_EXISTS", if tag_exists { "1" } else { "0" })
        .env("TAG_SHA", tag_sha)
        .env("VERIFY_FAIL", if verify_fails { "1" } else { "0" })
        .env("RECOVERY_TAG", recovery_tag.unwrap_or_default())
        .env("GH_RELEASE_AUTOMATION_TOKEN", "test-token")
        .env("RELEASE_SIGNING_KEY", "test-private-key")
        .env("RELEASE_SIGNING_NAME", "Release Bot")
        .env("RELEASE_SIGNING_EMAIL", "release@example.com")
        .output()?;
    Ok((
        output,
        fs::read_to_string(command_log).unwrap_or_default(),
        fs::read_to_string(github_output).unwrap_or_default(),
    ))
}

fn run_stage_release(existing: &str) -> Result<(Output, String), Box<dyn Error>> {
    let sandbox = tempfile::tempdir()?;
    let bin = sandbox.path().join("bin");
    let artifacts = sandbox.path().join("artifacts");
    fs::create_dir_all(&bin)?;
    fs::create_dir_all(&artifacts)?;
    fs::write(artifacts.join("lanyard.tar.xz"), "artifact")?;
    let command_log = sandbox.path().join("commands.log");
    write_executable(
        &bin.join("gh"),
        r#"#!/usr/bin/env bash
set -euo pipefail
printf 'gh %s\n' "$*" >> "$COMMAND_LOG"
if [[ "$*" == "release view "* ]]; then
  case "$EXISTING_RELEASE" in
    missing | error) exit 1 ;;
    draft) printf '%s\n' true ;;
    public) printf '%s\n' false ;;
  esac
fi
"#,
    )?;
    write_executable(
        &bin.join("curl"),
        r#"#!/usr/bin/env bash
set -euo pipefail
printf 'curl %s\n' "$*" >> "$COMMAND_LOG"
output=""
while (($#)); do
  if [[ "$1" == --output ]]; then
    output="$2"
    break
  fi
  shift
done
case "$EXISTING_RELEASE" in
  missing) printf '%s' 404 ;;
  draft) printf '%s\n' '{"draft":true}' > "$output"; printf '%s' 200 ;;
  public) printf '%s\n' '{"draft":false}' > "$output"; printf '%s' 200 ;;
  error) printf '%s' 503 ;;
esac
"#,
    )?;
    let path = format!(
        "{}:{}",
        bin.display(),
        env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_owned())
    );
    let output = Command::new("bash")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/stage-release.sh"))
        .current_dir(sandbox.path())
        .env("PATH", path)
        .env("COMMAND_LOG", &command_log)
        .env("EXISTING_RELEASE", existing)
        .env("GITHUB_REPOSITORY", "jwilger/lanyard-ssh-agent")
        .env("GH_TOKEN", "test-token")
        .env("RELEASE_TAG", "v1.2.3")
        .env("RELEASE_COMMIT", "release-sha")
        .env("ARTIFACT_DIR", &artifacts)
        .output()?;
    Ok((output, fs::read_to_string(command_log).unwrap_or_default()))
}

fn run_recovery_detection(
    crate_status: &str,
    github_status: &str,
    github_draft: &str,
) -> Result<(Output, String, String), Box<dyn Error>> {
    let sandbox = tempfile::tempdir()?;
    let bin = sandbox.path().join("bin");
    fs::create_dir_all(&bin)?;
    let command_log = sandbox.path().join("commands.log");
    let github_output = sandbox.path().join("github-output");
    write_executable(
        &bin.join("git"),
        r#"#!/usr/bin/env bash
printf 'git %s\n' "$*" >> "$COMMAND_LOG"
case "$1" in
  tag) printf '%s\n' v1.2.3 ;;
  rev-parse) printf '%s\n' old-release-sha ;;
  config | verify-tag) ;;
  *) exit 1 ;;
esac
"#,
    )?;
    write_executable(
        &bin.join("curl"),
        r#"#!/usr/bin/env bash
printf 'curl %s\n' "$*" >> "$COMMAND_LOG"
output=""
while (($#)); do
  if [[ "$1" == --output ]]; then
    output="$2"
    break
  fi
  shift
done
if [[ "$*" == *api.github.com* ]]; then
  printf '{"draft":%s}\n' "$GITHUB_RELEASE_DRAFT" > "$output"
  printf '%s' "$GITHUB_RELEASE_STATUS"
else
  printf '%s' "$CRATE_STATUS"
fi
"#,
    )?;
    write_executable(
        &bin.join("ssh-keygen"),
        "#!/usr/bin/env bash\nprintf '%s\\n' 'ssh-ed25519 AAAATEST'\n",
    )?;
    let path = format!(
        "{}:{}",
        bin.display(),
        env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_owned())
    );
    let output = Command::new("bash")
        .arg("scripts/detect-release-recovery.sh")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("PATH", path)
        .env("COMMAND_LOG", &command_log)
        .env("GITHUB_OUTPUT", &github_output)
        .env("RUNNER_TEMP", sandbox.path())
        .env("CRATE_STATUS", crate_status)
        .env("GITHUB_RELEASE_STATUS", github_status)
        .env("GITHUB_RELEASE_DRAFT", github_draft)
        .env("GITHUB_REPOSITORY", "jwilger/lanyard-ssh-agent")
        .env("GH_RELEASE_AUTOMATION_TOKEN", "test-token")
        .env("RELEASE_SIGNING_KEY", "test-private-key")
        .env("RELEASE_SIGNING_EMAIL", "release@example.com")
        .output()?;
    Ok((
        output,
        fs::read_to_string(command_log).unwrap_or_default(),
        fs::read_to_string(github_output).unwrap_or_default(),
    ))
}

#[expect(
    clippy::literal_string_with_formatting_args,
    reason = "the literal contains Bash parameter expansion, not Rust formatting"
)]
fn run_publish_crate(
    statuses: &str,
    manifest_version: &str,
    publish_fails: bool,
) -> Result<(Output, String), Box<dyn Error>> {
    let sandbox = tempfile::tempdir()?;
    let bin = sandbox.path().join("bin");
    fs::create_dir_all(&bin)?;
    let command_log = sandbox.path().join("commands.log");
    let status_file = sandbox.path().join("statuses");
    fs::write(&status_file, statuses)?;
    write_executable(
        &bin.join("curl"),
        r#"#!/usr/bin/env bash
set -euo pipefail
printf 'curl %s\n' "$*" >> "$COMMAND_LOG"
status="$(head -n 1 "$STATUS_FILE")"
tail -n +2 "$STATUS_FILE" > "$STATUS_FILE.next"
mv "$STATUS_FILE.next" "$STATUS_FILE"
printf '%s' "$status"
"#,
    )?;
    write_executable(
        &bin.join("cargo"),
        r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == metadata ]]; then
  printf '{"packages":[{"name":"lanyard-ssh-agent","version":"%s"}]}\n' "$MANIFEST_VERSION"
  exit 0
fi
printf 'cargo %s token=%s\n' "$*" "${CARGO_REGISTRY_TOKEN:+set}" >> "$COMMAND_LOG"
test "${PUBLISH_FAIL:-0}" != 1
"#,
    )?;
    let path = format!(
        "{}:{}",
        bin.display(),
        env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_owned())
    );
    let output = Command::new("bash")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/publish-crate.sh"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("PATH", path)
        .env("COMMAND_LOG", &command_log)
        .env("STATUS_FILE", status_file)
        .env("RELEASE_TAG", "v1.2.3")
        .env("MANIFEST_VERSION", manifest_version)
        .env("PUBLISH_FAIL", if publish_fails { "1" } else { "0" })
        .env("CARGO_REGISTRY_TOKEN", "test-token")
        .env("PUBLISH_POLL_ATTEMPTS", "3")
        .env("PUBLISH_POLL_DELAY_SECONDS", "0")
        .output()?;
    Ok((output, fs::read_to_string(command_log).unwrap_or_default()))
}

fn run_publish_release(existing: &str) -> Result<(Output, String), Box<dyn Error>> {
    let sandbox = tempfile::tempdir()?;
    let bin = sandbox.path().join("bin");
    fs::create_dir_all(&bin)?;
    let command_log = sandbox.path().join("commands.log");
    write_executable(
        &bin.join("gh"),
        r#"#!/usr/bin/env bash
set -euo pipefail
printf 'gh %s\n' "$*" >> "$COMMAND_LOG"
if [[ "$*" == "release view "* ]]; then
  case "$EXISTING_RELEASE" in
    missing) exit 1 ;;
    draft) printf '%s\n' true ;;
    public) printf '%s\n' false ;;
  esac
fi
"#,
    )?;
    let path = format!(
        "{}:{}",
        bin.display(),
        env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_owned())
    );
    let output = Command::new("bash")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/publish-release.sh"))
        .current_dir(sandbox.path())
        .env("PATH", path)
        .env("COMMAND_LOG", &command_log)
        .env("EXISTING_RELEASE", existing)
        .env("RELEASE_TAG", "v1.2.3")
        .output()?;
    Ok((output, fs::read_to_string(command_log).unwrap_or_default()))
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
fn release_write_access_is_limited_to_staging_and_final_publication() -> Result<(), Box<dyn Error>>
{
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
    assert_eq!(
        jobs.get("announce")
            .and_then(|announce| announce.get("permissions"))
            .and_then(|permissions| permissions.get("contents"))
            .and_then(Yaml::as_str),
        Some("write")
    );
    for job_name in [
        "host",
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
fn release_plz_only_prepares_release_metadata() -> Result<(), Box<dyn Error>> {
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

    assert_eq!(
        workspace.get("publish").and_then(Toml::as_bool),
        Some(false)
    );
    assert_eq!(
        workspace.get("git_tag_enable").and_then(Toml::as_bool),
        Some(false)
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
fn releases_have_one_main_branch_entrypoint() -> Result<(), Box<dyn Error>> {
    assert!(
        !Path::new(".github/workflows/release-plz.yml").exists(),
        "the former release-plz entrypoint must stay consolidated"
    );
    let release_workflows = fs::read_dir(".github/workflows")?
        .filter_map(Result::ok)
        .filter(|entry| {
            let path = entry.path();
            matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("yml" | "yaml")
            ) && fs::read_to_string(&path)
                .ok()
                .and_then(|source| serde_yaml::from_str::<Yaml>(&source).ok())
                .and_then(|document| {
                    document
                        .get("name")
                        .and_then(Yaml::as_str)
                        .map(str::to_owned)
                })
                .as_deref()
                == Some("Release")
        })
        .count();
    assert_eq!(release_workflows, 1);

    let release = workflow(".github/workflows/release.yml")?;
    let source = read(".github/workflows/release.yml")?;
    assert_immutable_action_references(&release)?;
    assert!(source.contains("branches: [main]"));
    assert!(source.contains("workflow_dispatch:"));
    assert!(!source.contains("pull_request:"));
    assert!(!source.contains("tags:"));
    assert!(source.contains("draft GitHub Release"));
    assert!(!source.contains("If you push multiple tags at once"));
    Ok(())
}

#[test]
fn release_preparation_updates_main_without_a_release_pr() -> Result<(), Box<dyn Error>> {
    let release = workflow(".github/workflows/release.yml")?;
    let concurrency = release
        .get("concurrency")
        .ok_or("release workflow must serialize preparation and publication")?;
    assert_eq!(
        concurrency.get("group").and_then(Yaml::as_str),
        Some("release-${{ github.ref }}")
    );
    assert_eq!(
        concurrency
            .get("cancel-in-progress")
            .and_then(Yaml::as_bool),
        Some(false)
    );
    let prepare = release
        .get("jobs")
        .and_then(|jobs| jobs.get("prepare-release"))
        .ok_or("release workflow must define a prepare-release job")?;
    assert_eq!(
        prepare
            .get("permissions")
            .and_then(|permissions| permissions.get("contents"))
            .and_then(Yaml::as_str),
        Some("read")
    );
    assert_eq!(
        prepare.get("if").and_then(Yaml::as_str),
        Some("${{ github.ref == 'refs/heads/main' }}")
    );
    let checkout = prepare
        .get("steps")
        .and_then(Yaml::as_sequence)
        .and_then(|steps| {
            steps
                .iter()
                .find(|step| step.get("name").and_then(Yaml::as_str) == Some("Checkout repository"))
        })
        .ok_or("release preparation must check out the repository")?;
    assert_eq!(
        checkout
            .get("with")
            .and_then(|inputs| inputs.get("ref"))
            .and_then(Yaml::as_str),
        Some("main")
    );
    assert_eq!(
        checkout
            .get("with")
            .and_then(|inputs| inputs.get("persist-credentials"))
            .and_then(Yaml::as_bool),
        Some(false)
    );
    let mut commands = Vec::new();
    let mut scripts = Vec::new();
    values_for_key(prepare, "command", &mut commands);
    values_for_key(prepare, "run", &mut scripts);
    let state_script = read("scripts/prepare-release.sh")?;

    assert_eq!(commands, ["update"]);
    let step_names = prepare
        .get("steps")
        .and_then(Yaml::as_sequence)
        .ok_or("release preparation steps must be a sequence")?
        .iter()
        .filter_map(|step| step.get("name").and_then(Yaml::as_str))
        .collect::<Vec<_>>();
    let update_index = step_names
        .iter()
        .position(|name| *name == "Update versions and changelog")
        .ok_or("release-plz update step is missing")?;
    let signing_index = step_names
        .iter()
        .position(|name| *name == "Configure signing and push release preparation commit")
        .ok_or("scoped signing step is missing")?;
    assert!(update_index < signing_index);
    assert!(state_script.contains("git commit -S"));
    assert!(state_script.contains("push origin HEAD:main"));
    assert!(!state_script.contains("--force"));
    assert!(!read(".github/workflows/release.yml")?.contains("release-pr"));
    Ok(())
}

#[test]
fn only_an_unpublished_version_enters_the_artifact_pipeline() -> Result<(), Box<dyn Error>> {
    let release = workflow(".github/workflows/release.yml")?;
    let jobs = release
        .get("jobs")
        .ok_or("release workflow must define jobs")?;
    let prepare = jobs
        .get("prepare-release")
        .ok_or("release workflow must prepare releases")?;
    let plan = jobs
        .get("plan")
        .ok_or("release workflow must plan artifacts")?;
    let mut scripts = Vec::new();
    values_for_key(prepare, "run", &mut scripts);
    assert!(scripts.contains(&"scripts/prepare-release.sh"));
    let state_script = read("scripts/prepare-release.sh")?;

    assert!(state_script.contains("--retry-all-errors"));
    assert!(state_script.contains("--connect-timeout"));
    assert!(state_script.contains("--max-time"));
    assert!(state_script.contains("git tag -s"));
    assert!(state_script.contains("push origin refs/tags/"));
    assert_eq!(
        prepare
            .get("outputs")
            .and_then(|outputs| outputs.get("publishing"))
            .and_then(Yaml::as_str),
        Some("${{ steps.release-state.outputs.publishing }}")
    );
    assert_eq!(
        prepare
            .get("outputs")
            .and_then(|outputs| outputs.get("release-commit"))
            .and_then(Yaml::as_str),
        Some("${{ steps.release-state.outputs.release-commit }}")
    );
    assert_eq!(
        plan.get("if").and_then(Yaml::as_str),
        Some("${{ needs.prepare-release.outputs.publishing == 'true' }}")
    );
    Ok(())
}

#[test]
fn preparation_commit_continues_in_the_same_release_run() -> Result<(), Box<dyn Error>> {
    let (dirty, dirty_log, dirty_outputs) =
        run_release_state(true, "404", "404", "false", false, "", false, None)?;
    assert!(
        dirty.status.success(),
        "{}",
        String::from_utf8_lossy(&dirty.stderr)
    );
    assert!(dirty_log.contains("commit -S -m chore(release): prepare v1.2.3"));
    assert!(dirty_log.contains("push origin HEAD:main"));
    assert!(dirty_log.contains("push origin refs/tags/v1.2.3"));
    assert!(dirty_outputs.contains("publishing=true"));
    assert!(dirty_outputs.contains("release-commit=release-sha"));
    let main_push = dirty_log
        .find("push origin HEAD:main")
        .ok_or("release preparation must push main")?;
    let registry_check = dirty_log
        .find("curl ")
        .ok_or("the same run must continue into publication")?;
    assert!(main_push < registry_check);
    Ok(())
}

#[test]
fn release_state_transitions_are_fail_closed_and_idempotent() -> Result<(), Box<dyn Error>> {
    let (unpublished, unpublished_log, unpublished_outputs) =
        run_release_state(false, "404", "404", "false", false, "", false, None)?;
    assert!(unpublished.status.success());
    assert!(unpublished_log.contains("tag -s -a v1.2.3"));
    assert!(unpublished_log.contains("push origin refs/tags/v1.2.3"));
    assert!(unpublished_outputs.contains("publishing=true"));
    assert!(unpublished_outputs.contains("tag=v1.2.3"));

    let (retry, retry_log, retry_outputs) = run_release_state(
        false,
        "404",
        "404",
        "false",
        true,
        "release-sha",
        false,
        None,
    )?;
    assert!(retry.status.success());
    assert!(retry_log.contains("verify-tag v1.2.3"));
    assert!(!retry_log.contains("tag -s -a"));
    assert!(retry_outputs.contains("publishing=true"));

    let (older_tag, older_tag_log, older_tag_outputs) =
        run_release_state(false, "404", "404", "false", true, "other-sha", false, None)?;
    assert!(older_tag.status.success());
    assert!(older_tag_log.contains("verify-tag v1.2.3"));
    assert!(older_tag_outputs.contains("publishing=true"));
    assert!(older_tag_outputs.contains("release-commit=other-sha"));

    let (invalid_signature, invalid_log, invalid_outputs) = run_release_state(
        false,
        "404",
        "404",
        "false",
        true,
        "release-sha",
        true,
        None,
    )?;
    assert!(!invalid_signature.status.success());
    assert!(!invalid_log.contains("push origin refs/tags/"));
    assert!(invalid_outputs.contains("publishing=false"));

    let (registry_error, _, registry_error_outputs) =
        run_release_state(false, "503", "404", "false", false, "", false, None)?;
    assert!(!registry_error.status.success());
    assert!(registry_error_outputs.contains("publishing=false"));
    Ok(())
}

#[test]
fn published_crate_resumes_incomplete_github_release() -> Result<(), Box<dyn Error>> {
    let (published, published_log, published_outputs) = run_release_state(
        false,
        "200",
        "200",
        "true",
        true,
        "old-release-sha",
        false,
        None,
    )?;
    assert!(published.status.success());
    assert!(!published_log.contains("tag -s"));
    assert!(published_log.contains("verify-tag v1.2.3"));
    let verification_config = published_log
        .find("config gpg.ssh.allowedSignersFile")
        .ok_or("SSH verification must be configured")?;
    let verification = published_log
        .find("verify-tag v1.2.3")
        .ok_or("tag signature must be verified")?;
    assert!(verification_config < verification);
    assert!(published_outputs.contains("publishing=true"));

    for (tag_exists, verify_fails) in [(false, false), (true, true)] {
        let (invalid_public, _, invalid_public_outputs) = run_release_state(
            false,
            "200",
            "200",
            "true",
            tag_exists,
            "old-release-sha",
            verify_fails,
            None,
        )?;
        assert!(!invalid_public.status.success());
        assert!(invalid_public_outputs.contains("publishing=false"));
    }

    let (missing_release, _, missing_release_outputs) = run_release_state(
        false,
        "200",
        "404",
        "false",
        true,
        "old-release-sha",
        false,
        None,
    )?;
    assert!(missing_release.status.success());
    assert!(missing_release_outputs.contains("publishing=true"));
    assert!(missing_release_outputs.contains("release-commit=old-release-sha"));

    let (missing_tag, missing_tag_log, missing_tag_outputs) =
        run_release_state(false, "200", "404", "false", false, "", false, None)?;
    assert!(!missing_tag.status.success());
    assert!(!missing_tag_log.contains("tag -s -a"));
    assert!(missing_tag_outputs.contains("publishing=false"));

    for (status, draft) in [("503", "false"), ("200", "null")] {
        let (invalid_release, _, invalid_release_outputs) = run_release_state(
            false,
            "200",
            status,
            draft,
            true,
            "release-sha",
            false,
            None,
        )?;
        assert!(!invalid_release.status.success());
        assert!(invalid_release_outputs.contains("publishing=false"));
    }
    Ok(())
}

#[test]
fn unfinished_release_is_selected_before_preparing_a_new_version() -> Result<(), Box<dyn Error>> {
    let (detected, commands, outputs) = run_recovery_detection("200", "200", "true")?;
    assert!(detected.status.success());
    assert!(commands.contains("verify-tag v1.2.3"));
    assert!(outputs.contains("recovering=true"));
    assert!(outputs.contains("tag=v1.2.3"));
    assert!(outputs.contains("release-commit=old-release-sha"));

    let release = workflow(".github/workflows/release.yml")?;
    let steps = release
        .get("jobs")
        .and_then(|jobs| jobs.get("prepare-release"))
        .and_then(|job| job.get("steps"))
        .and_then(Yaml::as_sequence)
        .ok_or("prepare-release steps must be a sequence")?;
    let recovery_index = steps
        .iter()
        .position(|step| step.get("id").and_then(Yaml::as_str) == Some("recovery-state"))
        .ok_or("release recovery must be detected")?;
    let update_index = steps
        .iter()
        .position(|step| {
            step.get("name").and_then(Yaml::as_str) == Some("Update versions and changelog")
        })
        .ok_or("release-plz update step is missing")?;
    assert!(recovery_index < update_index);
    assert_eq!(
        steps
            .get(update_index)
            .and_then(|step| step.get("if"))
            .and_then(Yaml::as_str),
        Some("${{ steps.recovery-state.outputs.recovering != 'true' }}")
    );
    Ok(())
}

#[test]
fn detected_recovery_tag_is_reverified_and_propagated() -> Result<(), Box<dyn Error>> {
    let (recovery, commands, outputs) = run_release_state(
        false,
        "200",
        "200",
        "true",
        true,
        "old-release-sha",
        false,
        Some("v1.2.3"),
    )?;
    assert!(recovery.status.success());
    assert!(commands.contains("verify-tag v1.2.3"));
    assert!(!commands.contains("add Cargo.toml"));
    assert!(!commands.contains("git commit "));
    assert!(!commands.contains("tag -s -a"));
    assert!(!commands.contains("push origin"));
    assert_eq!(
        outputs,
        "publishing=true\ntag=v1.2.3\ntag-flag=--tag=v1.2.3\nrelease-commit=old-release-sha\n"
    );

    let (invalid, invalid_commands, invalid_outputs) = run_release_state(
        false,
        "200",
        "200",
        "true",
        true,
        "old-release-sha",
        true,
        Some("v1.2.3"),
    )?;
    assert!(!invalid.status.success());
    assert!(!invalid_commands.contains("add Cargo.toml"));
    assert!(!invalid_commands.contains("git commit "));
    assert!(!invalid_commands.contains("tag -s -a"));
    assert!(!invalid_commands.contains("push origin"));
    assert_eq!(
        invalid_outputs,
        "publishing=false\ntag=v1.2.3\ntag-flag=--tag=v1.2.3\n"
    );

    let release = workflow(".github/workflows/release.yml")?;
    let steps = release
        .get("jobs")
        .and_then(|jobs| jobs.get("prepare-release"))
        .and_then(|job| job.get("steps"))
        .and_then(Yaml::as_sequence)
        .ok_or("prepare-release steps must be a sequence")?;
    let preparation = steps
        .iter()
        .find(|step| step.get("id").and_then(Yaml::as_str) == Some("release-state"))
        .ok_or("release-state step is missing")?;
    assert_eq!(
        preparation
            .get("env")
            .and_then(|environment| environment.get("RECOVERY_TAG"))
            .and_then(Yaml::as_str),
        Some("${{ steps.recovery-state.outputs.tag }}")
    );
    Ok(())
}

#[test]
fn release_recovery_detection_is_fail_closed_for_external_states() -> Result<(), Box<dyn Error>> {
    let cases = [
        ("404", "404", "false", true, Some(true)),
        ("404", "200", "true", true, Some(true)),
        ("200", "404", "false", true, Some(true)),
        ("200", "200", "false", true, Some(false)),
        ("404", "200", "false", false, None),
        ("503", "404", "false", false, None),
        ("200", "503", "false", false, None),
        ("200", "200", "null", false, None),
    ];
    for (crate_status, github_status, draft, succeeds, recovering) in cases {
        let (output, _, outputs) = run_recovery_detection(crate_status, github_status, draft)?;
        assert_eq!(
            output.status.success(),
            succeeds,
            "crate={crate_status} github={github_status} draft={draft}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        match recovering {
            Some(expected) => {
                assert!(outputs.contains(&format!("recovering={expected}")));
            }
            None => assert!(outputs.is_empty()),
        }
    }
    Ok(())
}

#[test]
fn completed_release_does_not_require_the_historical_signing_identity() -> Result<(), Box<dyn Error>>
{
    let (output, commands, outputs) = run_recovery_detection("200", "200", "false")?;
    assert!(output.status.success());
    assert_eq!(outputs, "recovering=false\n");
    assert!(!commands.contains("verify-tag"));
    Ok(())
}

#[test]
fn completed_release_preparation_does_not_require_the_historical_signing_identity()
-> Result<(), Box<dyn Error>> {
    let (output, commands, outputs) = run_release_state(
        false,
        "200",
        "200",
        "false",
        true,
        "old-release-sha",
        true,
        None,
    )?;
    assert!(output.status.success());
    assert_eq!(outputs, "publishing=false\n");
    assert!(!commands.contains("verify-tag"));
    Ok(())
}

#[test]
fn release_jobs_checkout_the_authoritative_tag_commit() -> Result<(), Box<dyn Error>> {
    let release = workflow(".github/workflows/release.yml")?;
    let jobs = release
        .get("jobs")
        .ok_or("release workflow must define jobs")?;
    let expected = [
        (
            "plan",
            "${{ needs.prepare-release.outputs.release-commit }}",
        ),
        (
            "build-local-artifacts",
            "${{ needs.plan.outputs.release-commit }}",
        ),
        (
            "build-global-artifacts",
            "${{ needs.plan.outputs.release-commit }}",
        ),
        ("host", "${{ needs.plan.outputs.release-commit }}"),
        ("publish-crate", "${{ needs.plan.outputs.release-commit }}"),
        ("announce", "${{ needs.plan.outputs.release-commit }}"),
    ];
    for (job_name, expected_ref) in expected {
        let job = jobs.get(job_name).ok_or("release job is missing")?;
        let mut refs = Vec::new();
        values_for_key(job, "ref", &mut refs);
        assert_eq!(refs, [expected_ref], "{job_name} checkout ref");
    }
    Ok(())
}

#[test]
fn verified_artifacts_are_staged_in_a_draft_release() -> Result<(), Box<dyn Error>> {
    let release = workflow(".github/workflows/release.yml")?;
    let host = release
        .get("jobs")
        .and_then(|jobs| jobs.get("host"))
        .ok_or("release workflow must define an artifact host job")?;
    let mut scripts = Vec::new();
    values_for_key(host, "run", &mut scripts);
    assert!(scripts.contains(&"scripts/stage-release.sh"));
    let host_script = read("scripts/stage-release.sh")?;

    assert!(host_script.contains("--draft"));
    assert!(host_script.contains("gh release upload"));
    assert!(host_script.contains("--clobber"));
    assert!(!host_script.contains("dist host"));
    Ok(())
}

#[test]
fn draft_release_staging_is_retryable_but_never_accepts_a_public_release()
-> Result<(), Box<dyn Error>> {
    let (missing, missing_log) = run_stage_release("missing")?;
    assert!(
        missing.status.success(),
        "{}",
        String::from_utf8_lossy(&missing.stderr)
    );
    assert!(missing_log.contains("release create v1.2.3 --draft"));
    assert!(missing_log.contains("release upload v1.2.3"));

    let (draft, draft_log) = run_stage_release("draft")?;
    assert!(draft.status.success());
    assert!(!draft_log.contains("release create"));
    assert!(draft_log.contains("release upload v1.2.3"));

    let (public, public_log) = run_stage_release("public")?;
    assert!(!public.status.success());
    assert!(!public_log.contains("release upload"));

    let (lookup_error, lookup_error_log) = run_stage_release("error")?;
    assert!(!lookup_error.status.success());
    assert!(!lookup_error_log.contains("release create"));
    assert!(!lookup_error_log.contains("release upload"));
    Ok(())
}

#[test]
fn draft_release_precedes_idempotent_crates_io_publication() -> Result<(), Box<dyn Error>> {
    let release = workflow(".github/workflows/release.yml")?;
    let jobs = release
        .get("jobs")
        .ok_or("release workflow must define jobs")?;
    let publish = jobs
        .get("publish-crate")
        .ok_or("release workflow must publish the crate")?;
    let dependencies = publish
        .get("needs")
        .and_then(Yaml::as_sequence)
        .ok_or("crate publication must declare dependencies")?;
    assert!(dependencies.contains(&Yaml::String("host".to_owned())));

    let mut scripts = Vec::new();
    values_for_key(publish, "run", &mut scripts);
    assert!(scripts.contains(&"scripts/publish-crate.sh"));
    let mut secrets = Vec::new();
    values_for_key(publish, "CARGO_REGISTRY_TOKEN", &mut secrets);
    assert_eq!(
        secrets
            .iter()
            .filter(|value| value.starts_with("op://"))
            .copied()
            .collect::<Vec<_>>(),
        ["op://Github Secrets/CARGO_REGISTRY_TOKEN/credential"]
    );

    let (version_mismatch, version_mismatch_log) = run_publish_crate("200\n", "9.9.9", false)?;
    assert!(!version_mismatch.status.success());
    assert!(version_mismatch_log.is_empty());

    let (already_published, already_published_log) = run_publish_crate("200\n", "1.2.3", false)?;
    assert!(already_published.status.success());
    assert!(!already_published_log.contains("cargo publish"));

    let (published, published_log) = run_publish_crate("404\n404\n200\n", "1.2.3", false)?;
    assert!(
        published.status.success(),
        "{}",
        String::from_utf8_lossy(&published.stderr)
    );
    assert!(published_log.contains("cargo publish --locked token=set"));
    assert!(!published_log.contains("test-token"));

    let (ambiguous_publish, ambiguous_publish_log) =
        run_publish_crate("404\n200\n", "1.2.3", true)?;
    assert!(ambiguous_publish.status.success());
    assert_eq!(ambiguous_publish_log.matches("cargo publish").count(), 1);

    let (registry_error, registry_error_log) = run_publish_crate("503\n", "1.2.3", false)?;
    assert!(!registry_error.status.success());
    assert!(!registry_error_log.contains("cargo publish"));

    let (never_visible, never_visible_log) =
        run_publish_crate("404\n404\n404\n404\n", "1.2.3", false)?;
    assert!(!never_visible.status.success());
    assert_eq!(never_visible_log.matches("cargo publish").count(), 1);
    Ok(())
}

#[test]
fn github_release_becomes_public_only_after_crates_io() -> Result<(), Box<dyn Error>> {
    let release = workflow(".github/workflows/release.yml")?;
    let announce = release
        .get("jobs")
        .and_then(|jobs| jobs.get("announce"))
        .ok_or("release workflow must announce the release")?;
    let dependencies = announce
        .get("needs")
        .and_then(Yaml::as_sequence)
        .ok_or("announcement must declare dependencies")?;
    assert!(dependencies.contains(&Yaml::String("host".to_owned())));
    assert!(dependencies.contains(&Yaml::String("publish-crate".to_owned())));

    let mut scripts = Vec::new();
    values_for_key(announce, "run", &mut scripts);
    assert!(scripts.contains(&"scripts/publish-release.sh"));

    let (draft, draft_log) = run_publish_release("draft")?;
    assert!(draft.status.success());
    assert!(draft_log.contains("release edit v1.2.3 --draft=false"));

    let (public, public_log) = run_publish_release("public")?;
    assert!(public.status.success());
    assert!(!public_log.contains("release edit"));

    let (missing, missing_log) = run_publish_release("missing")?;
    assert!(!missing.status.success());
    assert!(!missing_log.contains("release edit"));

    let (invalid, invalid_log) = run_publish_release("invalid")?;
    assert!(!invalid.status.success());
    assert!(!invalid_log.contains("release edit"));
    Ok(())
}

#[test]
fn release_docs_describe_the_single_staged_pipeline() -> Result<(), Box<dyn Error>> {
    let guide = read("docs/releases.md")?;
    let public_guide = read("site/src/pages/docs/releases.astro")?;
    let superseded = read("docs/adr/0004-separate-publishing-from-artifact-hosting.md")?;
    let decision = read("docs/adr/0006-stage-releases-before-publication.md")?;

    for document in [&guide, &public_guide, &decision] {
        let normalized = document.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(normalized.contains("draft GitHub Release"));
        assert!(normalized.contains("crates.io"));
        assert!(normalized.contains("GitHub Release public"));
    }
    assert!(guide.contains("Every push to `main`"));
    assert!(guide.contains("workflow_dispatch"));
    assert!(guide.contains("No release pull request"));
    assert!(!guide.contains("shared workflow"));
    assert!(!guide.contains("release-plz pull request"));
    assert!(superseded.contains("Superseded by ADR 0006"));
    assert!(decision.contains("## Status\n\nAccepted"));
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
