//! Installable feature modules.
//!
//! A module packages code that a generated project does not start with — a PDF
//! renderer, a notification channel, an import pipeline. Each module lives in
//! its own directory under the repository's top-level `modules/` folder and is
//! embedded at compile time, so installing one never depends on the module
//! sources being present next to the running binary.
//!
//! An install is *declarative*: `module.json` lists the directories to copy, the
//! Composer and npm packages to add and the Symfony bundles to register. There is
//! deliberately no install script — a shell script could not be made idempotent,
//! could not tell whether a file on disk was written by the module or by the
//! agent, and would not run on Windows without an extra shell.
//!
//! Everything an install records is kept in `<project>/.eggshell/modules.json`.
//! That file is what makes a second install a no-op: a same-version re-install
//! verifies the files it recorded and only touches the ones that are missing or
//! modified. When it does write, it always overwrites — a module owns every path
//! it declares, so a stale or broken copy is repaired rather than fought over.

use include_dir::{include_dir, Dir};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::llm::LlmResult;

/// The complete `modules/` tree, embedded recursively at compile time.
static MODULES_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../modules");

/// The manifest every module directory must contain.
const MANIFEST_FILE: &str = "module.json";

/// Where an install records what it did, relative to the project root.
const STATE_DIR: &str = ".eggshell";
const STATE_FILE: &str = "modules.json";

/// Installed module skills live here, so the skill loader can find them without
/// reading the state file.
const SKILL_DIR: &str = "modules";

/// Composer packages to install for a module.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ComposerSpec {
    /// `package` => version constraint, e.g. `"mpdf/mpdf": "^8.3"`.
    pub require: BTreeMap<String, String>,
}

/// npm packages to install for a module.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct NpmSpec {
    /// `package` => version range, e.g. `"jspdf": "^2.5.1"`.
    pub dependencies: BTreeMap<String, String>,
}

/// One directory copied from the module into the project.
///
/// Both paths are relative: `from` to the module directory, `to` to the project
/// root. A module therefore mirrors the project layout it targets, which keeps
/// the manifest readable: `backend` -> `backend`, `frontend` -> `frontend`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CopyMapping {
    pub from: String,
    pub to: String,
    /// When true a differing destination file is left alone instead of being
    /// replaced. Used for configuration defaults that a project or a Flex recipe
    /// may already provide; every non-optional mapping is overwritten on install.
    #[serde(default)]
    pub optional: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct BackendSpec {
    pub composer: ComposerSpec,
    /// Fully-qualified bundle classes appended to `config/bundles.php`.
    pub bundles: Vec<String>,
    #[serde(default)]
    pub copy: Vec<CopyMapping>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct FrontendSpec {
    pub npm: NpmSpec,
    #[serde(default)]
    pub copy: Vec<CopyMapping>,
}

/// A module's `module.json`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModuleManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    /// Path of the instructions file inside the module directory.
    #[serde(default = "default_skill_file")]
    pub skill: String,
    #[serde(default)]
    pub backend: BackendSpec,
    #[serde(default)]
    pub frontend: FrontendSpec,
}

fn default_skill_file() -> String {
    "SKILL.md".to_string()
}

/// A module as it is installed in one project.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct InstalledModule {
    pub version: String,
    pub installed_at: u64,
    /// Every file the module wrote, relative to the project root. Kept as a
    /// record for diagnostics and for reporting what a repair touched; a file
    /// outside it is still overwritten on install, but the overwrite is called
    /// out as a warning.
    pub files: Vec<String>,
    /// The module skill's path inside the project, relative to the project root.
    pub skill_path: Option<String>,
    /// False when the entry was written after a failed install, to record the
    /// files that were already copied. Such an entry exists so the retry owns its
    /// own files, but it must not be reported as a finished install. Entries
    /// written before this field existed are complete, hence the `true` default.
    #[serde(default = "default_true")]
    pub complete: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ModuleState {
    pub modules: BTreeMap<String, InstalledModule>,
}

/// What one install call did, so the agent can report it accurately.
#[derive(Debug, Clone, Default, Serialize)]
pub struct InstallReport {
    pub module: String,
    pub already_installed: bool,
    pub files_written: Vec<String>,
    /// Files already present with identical content; nothing was written.
    pub files_unchanged: Vec<String>,
    /// Files that already existed with different content and were replaced. A
    /// path is listed here when it was not in the module's ownership record, so
    /// a repair can say which pre-existing files it clobbered.
    pub files_overwritten: Vec<String>,
    pub packages_installed: Vec<String>,
    pub warnings: Vec<String>,
    pub skill_path: Option<String>,
    pub skill_content: Option<String>,
}

/// Returns every module found under `modules/`, sorted by id.
pub fn catalogue() -> Vec<ModuleManifest> {
    let mut modules: BTreeMap<String, ModuleManifest> = BTreeMap::new();

    for dir in MODULES_DIR.dirs() {
        let Some(manifest) = read_manifest(dir) else {
            continue;
        };
        modules.insert(manifest.id.clone(), manifest);
    }

    modules.into_values().collect()
}

fn read_manifest(dir: &'static Dir<'static>) -> Option<ModuleManifest> {
    let name = dir.path().file_name()?.to_str()?;
    // `include_dir` resolves paths from the embedded root, not from `dir`.
    let raw = MODULES_DIR.get_file(format!("{name}/{MANIFEST_FILE}"))?;
    let text = raw.contents_utf8()?;
    let manifest: ModuleManifest = serde_json::from_str(text).ok()?;
    (!manifest.id.trim().is_empty()).then_some(manifest)
}

fn module_dir(id: &str) -> Option<&'static Dir<'static>> {
    MODULES_DIR
        .dirs()
        .find(|dir| dir.path().file_name().and_then(|name| name.to_str()) == Some(id))
}

/// Finds a module by id.
pub fn find(id: &str) -> Option<ModuleManifest> {
    catalogue().into_iter().find(|module| module.id == id)
}

pub fn available_ids() -> Vec<String> {
    catalogue().into_iter().map(|module| module.id).collect()
}

/// The block appended to the builder agent's system prompt.
///
/// Mirrors the shape of `agent::skills_summary` on purpose: the model already
/// knows how to read that section, and the two appear together in the prompt.
pub fn catalogue_prompt() -> String {
    let modules = catalogue();
    if modules.is_empty() {
        return String::new();
    }

    let installed_note = "Call `enable_module` with the module id to install one. \
Installing copies its backend and frontend files into the project, adds its \
Composer/npm packages and registers its Symfony bundles. It overwrites the \
module's own files, so calling it again repairs a broken or partial install. \
Call it before writing code that depends on the module, and follow the \
module's skill for wiring.";

    let mut summary = String::from(
        "\n\n=== AVAILABLE MODULES ===\n\
The project can be extended with optional modules. They are not installed by default.\n\n",
    );
    for module in &modules {
        summary.push_str(&format!("**{}** ({})\n", module.id, module.name));
        if !module.description.trim().is_empty() {
            summary.push_str(&format!("Description: {}\n", module.description.trim()));
        }
        let composer = module
            .backend
            .composer
            .require
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        if !composer.is_empty() {
            summary.push_str(&format!("PHP packages: {}\n", composer.join(", ")));
        }
        let npm = module
            .frontend
            .npm
            .dependencies
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        if !npm.is_empty() {
            summary.push_str(&format!("npm packages: {}\n", npm.join(", ")));
        }
        summary.push('\n');
    }
    summary.push_str(installed_note);
    summary.push_str("\n=== END MODULES ===\n");
    summary
}

fn state_path(project: &Path) -> PathBuf {
    project.join(STATE_DIR).join(STATE_FILE)
}

pub fn read_state(project: &Path) -> ModuleState {
    let Ok(text) = std::fs::read_to_string(state_path(project)) else {
        return ModuleState::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn write_state(project: &Path, state: &ModuleState) -> LlmResult<()> {
    let path = state_path(project);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(state)?)?;
    Ok(())
}

/// Installs `id` into `project`.
///
/// Installation is idempotent and self-repairing: a module recorded at the same
/// version whose files are all intact is reported as already installed and
/// nothing is touched, while missing or modified files are re-written. A module
/// recorded at a different version is re-written over the files it owns. Any
/// pre-existing file a mapping targets is replaced, and the replacement is
/// reported in `InstallReport::files_overwritten`.
///
/// `module.json`, `SKILL.md` and the `overrides/` tree live at the module root
/// and are never copied by the `backend`/`frontend` mappings, so the skill is the
/// only thing that reaches `.eggshell/`.
pub async fn install(project: &Path, id: &str) -> LlmResult<InstallReport> {
    install_inner(project, id, true).await
}

/// Shared implementation. `run_packages` is false in tests, which must exercise
/// the copy/verify/repair path without invoking Composer or npm.
async fn install_inner(project: &Path, id: &str, run_packages: bool) -> LlmResult<InstallReport> {
    let id = id.trim();
    let Some(manifest) = find(id) else {
        return Err(format!(
            "Unknown module '{id}'. Available modules: {}.",
            available_ids().join(", ")
        )
        .into());
    };
    let Some(dir) = module_dir(&manifest.id) else {
        return Err(format!("Module '{id}' has no directory on disk.").into());
    };

    let mut state = read_state(project);
    let mut report = InstallReport {
        module: manifest.id.clone(),
        ..Default::default()
    };

    // A same-version install is only a no-op if every file it recorded is still
    // on disk unchanged. Otherwise fall through and re-copy, which repairs a
    // deleted or edited module file instead of leaving the project broken.
    let drifted = if let Some(installed) = state.modules.get(&manifest.id) {
        if installed.complete && installed.version == manifest.version {
            let drifted = verify_installed(project, dir, &manifest);
            if drifted.is_empty() {
                report.already_installed = true;
                report.skill_path = installed.skill_path.clone();
                report.skill_content = installed
                    .skill_path
                    .as_ref()
                    .and_then(|path| std::fs::read_to_string(project.join(path)).ok());
                return Ok(report);
            }
            drifted
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    if !drifted.is_empty() {
        report.warnings.push(format!(
            "{} file(s) were missing or modified since the module was installed; reinstalling to repair them.",
            drifted.len()
        ));
    }

    // The ownership record from a previous install. It no longer gates writes —
    // a module owns every path it declares — but it distinguishes a normal
    // re-install from a replacement of a file the module never wrote, which is
    // surfaced as a warning.
    let owned: Vec<String> = state
        .modules
        .get(&manifest.id)
        .map(|installed| installed.files.clone())
        .unwrap_or_default();

    let mut written: Vec<String> = Vec::new();

    for mapping in manifest
        .backend
        .copy
        .iter()
        .chain(manifest.frontend.copy.iter())
    {
        if let Err(error) = copy_mapping(project, dir, mapping, &owned, &mut written, &mut report) {
            // Record what was written before the failure so a retry knows those
            // files belong to the module and reports them as unchanged rather
            // than as replacements.
            record_partial_install(project, &manifest, &owned, &written);
            return Err(error);
        }
    }

    if !report.files_overwritten.is_empty() {
        report.warnings.push(format!(
            "Replaced {} pre-existing file(s) the module owns: {}.",
            report.files_overwritten.len(),
            report.files_overwritten.join(", ")
        ));
    }

    // Composer and npm run last, so a copy failure aborts before minutes are
    // spent downloading packages.

    // Ownership is recorded *before* the package-manager steps. Composer and npm
    // can fail after the copies succeeded; recording now keeps the retry able to
    // report those files as the module's own.
    record_partial_install(project, &manifest, &owned, &written);

    // Set once something has rebuilt the service container; `composer require`
    // does that through `post-update-cmd`.
    let mut container_rebuilt = false;

    // Bundles are registered before Composer runs. The module's `config/packages`
    // files are already on disk, and `composer require` triggers the project's
    // `post-update-cmd` (`cache:clear`), which compiles the container — a `twig:`
    // block with no TwigBundle registered would fail that compile. Flex's recipe
    // also registers the bundle, but a recipe can be skipped while the package
    // still installs, so the install must not depend on it. `register_bundles`
    // is idempotent, so a Flex-registered bundle is left alone.
    if !manifest.backend.bundles.is_empty() {
        let bundles = project.join("backend/config/bundles.php");
        if bundles.is_file() {
            match register_bundles(&bundles, &manifest.backend.bundles) {
                Ok(added) => {
                    if added > 0 {
                        report.warnings.push(format!(
                            "Registered {added} Symfony bundle(s) in config/bundles.php."
                        ));
                    }
                }
                Err(error) => report.warnings.push(format!(
                    "Could not register Symfony bundles in config/bundles.php: {error}"
                )),
            }
        } else {
            report
                .warnings
                .push("config/bundles.php not found; bundles were not registered.".to_string());
        }
    }

    if run_packages && !manifest.backend.composer.require.is_empty() {
        let backend = project.join("backend");
        if !backend.is_dir() {
            report
                .warnings
                .push("backend directory not found; skipped Composer packages.".to_string());
        } else {
            let packages: Vec<String> = manifest
                .backend
                .composer
                .require
                .iter()
                .map(|(name, constraint)| format!("{name}:{constraint}"))
                .collect();
            let command = format!("composer require {} --no-interaction --no-progress", packages.join(" "));
            match crate::llm::symfony::run_command_capture(&command, &backend).await {
                Ok(_) => {
                    report.packages_installed.extend(packages);
                    // `post-update-cmd` already ran `cache:clear`, so the
                    // trailing rebuild can be skipped.
                    container_rebuilt = true;
                }
                // `composer require` also runs the project's `post-update-cmd`
                // scripts, which include `cache:clear`. That compiles the whole
                // container, so an error in the project's own code — including a
                // file the module just copied — exits non-zero although the
                // packages downloaded fine. Reporting that as "Composer could not
                // install the packages" sends the agent chasing the wrong bug, so
                // the two cases are told apart: if every declared package is on
                // disk, only the script failed.
                Err(error) => {
                    let missing = missing_composer_packages(&backend, &manifest.backend.composer.require);
                    if missing.is_empty() {
                        report.packages_installed.extend(packages);
                        report.warnings.push(format!(
                            "The Composer packages were installed, but Composer's post-install \
                             scripts failed. This is a problem in the project's own code, not in the \
                             module's dependencies: {error}"
                        ));
                    } else {
                        return Err(format!(
                            "Composer could not install the packages the '{id}' module needs \
                             (missing: {}): {error}",
                            missing.join(", ")
                        )
                        .into());
                    }
                }
            }
        }
    }

    if run_packages && !manifest.frontend.npm.dependencies.is_empty() {
        let frontend = project.join("frontend");
        if !frontend.is_dir() {
            report
                .warnings
                .push("frontend directory not found; skipped npm packages.".to_string());
        } else {
            let packages: Vec<String> = manifest
                .frontend
                .npm
                .dependencies
                .iter()
                .map(|(name, range)| format!("{name}@{range}"))
                .collect();
            match crate::llm::react::run_npm_install_capture(&packages, &frontend).await {
                Ok(_) => report.packages_installed.extend(packages),
                Err(error) => {
                    return Err(format!(
                        "npm could not install the packages the '{id}' module needs: {error}"
                    )
                    .into())
                }
            }
        }
    }

    // A copied `config/packages/*.yaml` or a new bundle is invisible until the
    // container is rebuilt. Best effort: a failure here must not undo a good
    // install — the project's own code may be broken, and the agent needs the
    // real error from `lint_code`, not a failed install.
    if run_packages && !container_rebuilt && project.join("backend/bin/console").is_file() {
        let console_path = project.join("backend");
        if let Err(error) = crate::llm::symfony::run_command_capture(
            "php bin/console cache:clear --no-warmup",
            &console_path,
        )
        .await
        {
            report
                .warnings
                .push(format!("cache:clear failed: {error}"));
        }
    }

    let skill_path = install_skill(project, dir, &manifest, &mut report)?;
    report.skill_path = skill_path.clone();
    report.skill_content = skill_path
        .as_ref()
        .and_then(|path| std::fs::read_to_string(project.join(path)).ok());

    written.sort();
    written.dedup();

    let mut files = written.clone();
    for file in &owned {
        if !files.contains(file) {
            files.push(file.clone());
        }
    }
    files.sort();

    state.modules.insert(
        manifest.id.clone(),
        InstalledModule {
            version: manifest.version.clone(),
            installed_at: now_secs(),
            files,
            skill_path,
            complete: true,
        },
    );
    write_state(project, &state)?;

    report.files_written = written;
    Ok(report)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

/// Persists the files copied before a failure, so a retry recognises them as the
/// module's own and can finish the install.
///
/// The state entry deliberately records no skill and the version already on disk,
/// and is flagged `complete: false`: the install did not finish, so a later run
/// must not treat it as a no-op. It exists only to establish ownership of the
/// files that are already there.
fn record_partial_install(
    project: &Path,
    manifest: &ModuleManifest,
    previously_owned: &[String],
    written: &[String],
) {
    if written.is_empty() {
        return;
    }
    let mut state = read_state(project);
    let mut files = written.to_vec();
    for file in previously_owned {
        if !files.contains(file) {
            files.push(file.clone());
        }
    }
    files.sort();

    let entry = state.modules.entry(manifest.id.clone()).or_default();
    entry.files = files;
    if entry.version.is_empty() {
        entry.version = manifest.version.clone();
    }
    entry.installed_at = now_secs();
    entry.complete = false;
    let _ = write_state(project, &state);
}

/// Returns the declared Composer packages that are not present in the project.
///
/// Used to tell "the download failed" (a real install error) from "the download
/// worked and the project's `post-update-cmd` scripts failed" (a problem in the
/// project's own code). A package is considered present when its `vendor/`
/// directory exists; that is exactly what autoloading needs.
fn missing_composer_packages(
    backend: &Path,
    require: &BTreeMap<String, String>,
) -> Vec<String> {
    require
        .keys()
        .filter(|package| !backend.join("vendor").join(package).is_dir())
        .cloned()
        .collect()
}

/// Copies one `from` -> `to` mapping out of the embedded module directory.
fn copy_mapping(
    project: &Path,
    module_root: &'static Dir<'static>,
    mapping: &CopyMapping,
    owned: &[String],
    written: &mut Vec<String>,
    report: &mut InstallReport,
) -> LlmResult<()> {
    let from = mapping.from.trim_matches('/').replace('\\', "/");
    let to = mapping.to.trim_matches('/').replace('\\', "/");

    // `include_dir` resolves paths from the embedded root, not from `module_root`.
    let full = format!(
        "{}/{}",
        module_root
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default(),
        from
    );
    let Some(dir) = MODULES_DIR.get_dir(&full) else {
        return Err(format!(
            "Module directory '{from}' is missing from the package (mapping to '{to}')."
        )
        .into());
    };

    copy_dir(project, dir, &to, owned, written, report, mapping)
}

#[allow(clippy::too_many_arguments)]
fn copy_dir(
    project: &Path,
    dir: &'static Dir<'static>,
    project_prefix: &str,
    owned: &[String],
    written: &mut Vec<String>,
    report: &mut InstallReport,
    mapping: &CopyMapping,
) -> LlmResult<()> {
    for file in dir.files() {
        let Some(name) = file.path().file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let project_relative = join_relative(project_prefix, name);
        let destination = project.join(&project_relative);

        let contents = file.contents();
        match std::fs::read(&destination) {
            Ok(existing) if existing == contents => {
                report.files_unchanged.push(project_relative);
                continue;
            }
            // An `optional` mapping is a default that must not fight the project:
            // if the file already exists, whatever the agent or a Flex recipe put
            // there wins, even when it differs.
            Ok(_) if mapping.optional => {
                report.files_unchanged.push(project_relative);
                continue;
            }
            Ok(_) => {
                // The module owns this path, so a stale, broken or agent-written
                // file is replaced rather than treated as a conflict. Files the
                // module did not record are called out so a repair is visible.
                if !owned.iter().any(|path| path == &project_relative) {
                    report.files_overwritten.push(project_relative.clone());
                }
            }
            _ => {}
        }

        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&destination, contents)?;
        written.push(project_relative);
    }

    for child in dir.dirs() {
        let Some(name) = child.path().file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        copy_dir(
            project,
            child,
            &join_relative(project_prefix, name),
            owned,
            written,
            report,
            mapping,
        )?;
    }

    Ok(())
}

/// Returns the project-relative paths an earlier install recorded but that are
/// now missing or differ from the module's embedded copy.
///
/// Used to decide whether a same-version re-enable can short-circuit or must
/// re-copy to repair the project. `optional` mappings are skipped on purpose:
/// the project's own file is expected to win there.
fn verify_installed(
    project: &Path,
    module_root: &'static Dir<'static>,
    manifest: &ModuleManifest,
) -> Vec<String> {
    let dir_name = module_root
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let mut drifted = Vec::new();

    for mapping in manifest
        .backend
        .copy
        .iter()
        .chain(manifest.frontend.copy.iter())
    {
        if mapping.optional {
            continue;
        }
        let from = mapping.from.trim_matches('/').replace('\\', "/");
        let to = mapping.to.trim_matches('/').replace('\\', "/");
        // `include_dir` resolves paths from the embedded root, not from `module_root`.
        let full = format!("{dir_name}/{from}");
        let Some(source) = MODULES_DIR.get_dir(&full) else {
            continue;
        };
        collect_drifted(project, source, &to, &mut drifted);
    }

    // The skill is installed outside any copy mapping, so it is checked directly.
    let skill_relative = format!("{STATE_DIR}/{SKILL_DIR}/{}/SKILL.md", manifest.id);
    let skill = manifest.skill.trim_matches('/').replace('\\', "/");
    if let Some(file) = MODULES_DIR.get_file(format!("{dir_name}/{skill}")) {
        let destination = project.join(&skill_relative);
        match std::fs::read(&destination) {
            Ok(existing) if existing == file.contents() => {}
            _ => drifted.push(skill_relative),
        }
    }

    drifted
}

fn collect_drifted(
    project: &Path,
    dir: &'static Dir<'static>,
    project_prefix: &str,
    into: &mut Vec<String>,
) {
    for file in dir.files() {
        let Some(name) = file.path().file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let project_relative = join_relative(project_prefix, name);
        let destination = project.join(&project_relative);
        match std::fs::read(&destination) {
            Ok(existing) if existing == file.contents() => {}
            _ => into.push(project_relative),
        }
    }

    for child in dir.dirs() {
        let Some(name) = child.path().file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        collect_drifted(
            project,
            child,
            &join_relative(project_prefix, name),
            into,
        );
    }
}

fn join_relative(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}

/// Copies the module's skill into `.eggshell/modules/<id>/` and returns its
/// project-relative path.
fn install_skill(
    project: &Path,
    module_root: &'static Dir<'static>,
    manifest: &ModuleManifest,
    report: &mut InstallReport,
) -> LlmResult<Option<String>> {
    let skill = manifest.skill.trim_matches('/').replace('\\', "/");
    let dir_name = module_root
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&manifest.id);
    // `include_dir` resolves paths from the embedded root, not from `module_root`.
    let Some(file) = MODULES_DIR.get_file(format!("{dir_name}/{skill}")) else {
        report.warnings.push(format!(
            "Module '{0}' declares skill '{skill}' but the file is missing; its instructions were not installed.",
            manifest.id
        ));
        report.skill_content = None;
        return Ok(None);
    };

    let relative = format!("{STATE_DIR}/{SKILL_DIR}/{}/SKILL.md", manifest.id);
    let destination = project.join(&relative);
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&destination, file.contents())?;
    Ok(Some(relative))
}

/// Adds `bundles` to a Symfony `config/bundles.php` that are not already there.
///
/// The file is a plain PHP array literal. Rather than parsing PHP, the closing
/// `];` is located and entries are inserted before it, which preserves whatever
/// Flex wrote and stays valid as long as the array literal does — which is the
/// one thing every Symfony skeleton guarantees.
fn register_bundles(path: &Path, bundles: &[String]) -> std::io::Result<usize> {
    let source = std::fs::read_to_string(path)?;
    let mut additions = Vec::new();

    for bundle in bundles {
        // `::class` is compared, not the raw string, so both `Foo\Bar::class` and
        // a Flex-written `Foo\Bar::class` match the same entry.
        let fqcn = bundle.trim().trim_matches('\\');
        if fqcn.is_empty() {
            continue;
        }
        let already = source.contains(&format!("{fqcn}::class"))
            || source.contains(&format!("\\{fqcn}::class"));
        if !already {
            additions.push(format!("    \\{fqcn}::class => ['all' => true],"));
        }
    }

    if additions.is_empty() {
        return Ok(0);
    }

    let Some(close) = source.rfind("];") else {
        return Err(std::io::Error::other(
            "config/bundles.php has no closing `];` to insert into",
        ));
    };

    let mut updated = String::with_capacity(source.len() + additions.join("\n").len());
    updated.push_str(&source[..close]);
    if !source[..close].ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&additions.join("\n"));
    updated.push('\n');
    updated.push_str(&source[close..]);

    std::fs::write(path, updated)?;
    Ok(additions.len())
}

/// `enable_module` result, kept here so the tool stays a thin wrapper.
pub fn report_to_value(report: &InstallReport) -> Value {
    let message = if report.already_installed {
        format!(
            "Module '{}' is already installed; nothing was changed.",
            report.module
        )
    } else {
        let mut message = format!(
            "Module '{}' installed: {} file(s) written, {} package(s) added.",
            report.module,
            report.files_written.len(),
            report.packages_installed.len()
        );
        if !report.files_overwritten.is_empty() {
            message.push_str(&format!(
                " {} pre-existing file(s) were replaced.",
                report.files_overwritten.len()
            ));
        }
        message
    };

    json!({
        "success": true,
        "message": message,
        "details": {
            "module": report.module,
            "alreadyInstalled": report.already_installed,
            "filesWritten": report.files_written,
            "filesUnchanged": report.files_unchanged,
            "filesOverwritten": report.files_overwritten,
            "packagesInstalled": report.packages_installed,
            "warnings": report.warnings,
            "skillPath": report.skill_path,
            "skillContent": report.skill_content,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs a future on a fresh current-thread runtime, matching the pattern
    /// used by the other tool tests.
    fn run<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime builds")
            .block_on(future)
    }

    /// Public methods a controller inherits from `AbstractController`. A module
    /// controller that declares one of these with its own signature is a PHP
    /// fatal error at class-compile time: PHP never autoloads the file on its own,
    /// so the failure only appears when something compiles the container (for
    /// example `cache:clear` inside Composer's `post-update-cmd`), which turns a
    /// broken module file into an inexplicable install failure.
    ///
    /// Verified against Symfony 8.1 `AbstractController`. Re-check this list when
    /// upgrading Symfony: a method added there becomes a reserved name here.
    const ABSTRACT_CONTROLLER_PUBLIC_API: [&str; 18] = [
        "addFlash",
        "createAccessDeniedException",
        "createForm",
        "createFormBuilder",
        "createNotFoundException",
        "denyAccessUnlessGranted",
        "forward",
        "generateUrl",
        "getParameter",
        "getSubscribedServices",
        "getUser",
        "isGranted",
        "json",
        "redirect",
        "redirectToRoute",
        "render",
        "renderForm",
        "renderView",
    ];

    /// Every public method name declared in `$contents` (`public function name(`).
    fn declared_public_methods(contents: &str) -> Vec<String> {
        const MARKER: &str = "public function ";
        const STATIC_MARKER: &str = "public static function ";
        let mut names = Vec::new();
        for line in contents.lines() {
            // `public static function getSubscribedServices()` collides with the
            // parent just the same; PHP does not care about the modifier here.
            let rest = if let Some(index) = line.find(STATIC_MARKER) {
                &line[index + STATIC_MARKER.len()..]
            } else if let Some(index) = line.find(MARKER) {
                &line[index + MARKER.len()..]
            } else {
                continue;
            };
            let name: String = rest
                .chars()
                .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
                .collect();
            if !name.is_empty() {
                names.push(name);
            }
        }
        names
    }

    /// Collects every `*.php` file below `dir`, as (embedded path, source).
    fn collect_php(dir: &'static Dir<'static>, into: &mut Vec<(String, String)>) {
        for file in dir.files() {
            let path = file.path().to_string_lossy().replace('\\', "/");
            if !path.ends_with(".php") {
                continue;
            }
            if let Some(contents) = file.contents_utf8() {
                into.push((path, contents.to_string()));
            }
        }
        for child in dir.dirs() {
            collect_php(child, into);
        }
    }

    /// The module package is copied verbatim into a project, so a defect in it is
    /// a defect in every project that installs it. This is the check that would
    /// have caught `PdfController::render(string $template, string $id)` turning
    /// `composer require` into exit code 255.
    #[test]
    fn module_controllers_do_not_override_abstract_controller_methods() {
        let mut violations = Vec::new();

        for module_dir in MODULES_DIR.dirs() {
            let Some(name) = module_dir.path().file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some(controllers) = MODULES_DIR.get_dir(format!("{name}/backend/src/Controller"))
            else {
                continue;
            };

            let mut files = Vec::new();
            collect_php(controllers, &mut files);

            for (path, contents) in files {
                // Only classes that actually extend the base controller inherit
                // (and therefore must not redeclare) its methods.
                if !contents.contains("extends AbstractController")
                    && !contents.contains("extends \\Symfony\\Bundle\\FrameworkBundle\\Controller\\AbstractController")
                {
                    continue;
                }
                for method in declared_public_methods(&contents) {
                    if ABSTRACT_CONTROLLER_PUBLIC_API.contains(&method.as_str()) {
                        violations.push(format!(
                            "{path}: public function {method}() overrides AbstractController::{method}()"
                        ));
                    }
                }
            }
        }

        assert!(
            violations.is_empty(),
            "module controllers must not redeclare AbstractController methods \
             (PHP fatal at class-compile time, breaks cache:clear during install):\n{}",
            violations.join("\n")
        );
    }

    /// Guards the helper itself, so a silent regex regression cannot make the
    /// check above pass vacuously.
    #[test]
    fn declared_public_methods_reads_signatures() {
        let contents = "\
    public function render(string $template, string $id): Response
    public function templates(): JsonResponse
    public static function getSubscribedServices(): array
    private function helper(): void
";
        let mut methods = declared_public_methods(contents);
        methods.sort();
        assert_eq!(methods, ["getSubscribedServices", "render", "templates"]);
    }

    #[test]
    fn catalogue_contains_pdf_export() {
        let ids = available_ids();
        assert!(ids.contains(&"pdf-export".to_string()), "ids: {ids:?}");
    }

    #[test]
    fn manifest_declares_backend_packages() {
        let module = find("pdf-export").expect("pdf-export module exists");
        assert!(module.backend.composer.require.contains_key("mpdf/mpdf"));
        assert!(module.backend.composer.require.contains_key("symfony/twig-bundle"));
        assert!(!module.backend.bundles.is_empty());
        assert!(!module.backend.copy.is_empty());
        // The frontend client only uses the shared axios instance, so the module
        // deliberately pulls in no npm packages.
        assert!(module.frontend.npm.dependencies.is_empty());
        assert!(!module.frontend.copy.is_empty());
    }

    #[test]
    fn catalogue_prompt_lists_every_module() {
        let prompt = catalogue_prompt();
        for id in available_ids() {
            assert!(prompt.contains(&id), "prompt missing {id}");
        }
        assert!(prompt.contains("enable_module"));
    }

    #[test]
    fn registers_missing_bundles_once() {
        let dir = std::env::temp_dir().join(format!("eggshell-bundles-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("bundles.php");
        std::fs::write(
            &path,
            "<?php\n\nreturn [\n    Symfony\\Bundle\\FrameworkBundle\\FrameworkBundle::class => ['all' => true],\n];\n",
        )
        .expect("seed bundles");

        let added = register_bundles(&path, &["Symfony\\Bundle\\TwigBundle\\TwigBundle".to_string()])
            .expect("register");
        assert_eq!(added, 1);
        let again = register_bundles(&path, &["Symfony\\Bundle\\TwigBundle\\TwigBundle".to_string()])
            .expect("register twice");
        assert_eq!(again, 0);

        let source = std::fs::read_to_string(&path).expect("read");
        assert_eq!(source.matches("TwigBundle::class").count(), 1);
        assert!(source.trim_end().ends_with("];"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tail_output_keeps_the_end() {
        let text = (0..10).map(|n| n.to_string()).collect::<Vec<_>>().join("\n");
        let tail = crate::llm::symfony::tail_output(&text, 3);
        assert!(tail.contains("earlier lines omitted"));
        assert!(tail.ends_with('9'));
    }

    /// The module content itself: every declared copy source must exist in the
    /// package, or an install would fail halfway through.
    #[test]
    fn every_declared_copy_source_exists() {
        let module = find("pdf-export").expect("pdf-export module exists");
        for mapping in module.backend.copy.iter().chain(module.frontend.copy.iter()) {
            let path = format!("pdf-export/{}", mapping.from.trim_matches('/'));
            assert!(
                MODULES_DIR.get_dir(&path).is_some(),
                "missing module directory '{path}'"
            );
        }
        assert!(
            MODULES_DIR
                .get_file(format!("pdf-export/{}", module.skill))
                .is_some(),
            "missing SKILL.md"
        );
    }

    /// Copies into a temp project without running Composer/npm: those steps are
    /// guarded by the presence of `backend/` and `frontend/`, which this fixture
    /// deliberately does not create beyond the files the module writes.
    #[test]
    fn install_writes_files_and_is_idempotent() {
        let project = std::env::temp_dir().join(format!(
            "eggshell-install-{}-{}",
            std::process::id(),
            now_secs()
        ));
        std::fs::create_dir_all(&project).expect("project dir");

        let dir = module_dir("pdf-export").expect("module directory");
        let module = find("pdf-export").expect("manifest");
        let mut written = Vec::new();
        let mut report = InstallReport {
            module: module.id.clone(),
            ..Default::default()
        };
        for mapping in module.backend.copy.iter().chain(module.frontend.copy.iter()) {
            copy_mapping(&project, dir, mapping, &[], &mut written, &mut report)
                .expect("copy succeeds");
        }

        assert!(project
            .join("backend/src/Service/PdfTemplate/TemplatesPdf.php")
            .is_file());
        assert!(project.join("frontend/src/api/pdfClient.ts").is_file());
        assert!(project
            .join("backend/src/Service/PdfTemplate/Templates/invoice/template.html.twig")
            .is_file());

        // A second pass over the same files changes nothing.
        let mut again = Vec::new();
        let mut second = InstallReport {
            module: module.id.clone(),
            ..Default::default()
        };
        for mapping in module.backend.copy.iter().chain(module.frontend.copy.iter()) {
            copy_mapping(&project, dir, mapping, &written, &mut again, &mut second)
                .expect("second copy succeeds");
        }
        assert!(again.is_empty(), "second pass wrote: {again:?}");
        assert!(!second.files_unchanged.is_empty());

        let _ = std::fs::remove_dir_all(&project);
    }

    #[test]
    fn optional_mapping_keeps_the_existing_file() {
        let project = std::env::temp_dir().join(format!(
            "eggshell-optional-{}-{}",
            std::process::id(),
            now_secs()
        ));
        let target = project.join("backend/config/packages/twig.yaml");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("dirs");
        std::fs::write(&target, "# project's own twig config\n").expect("seed file");

        let dir = module_dir("pdf-export").expect("module directory");
        let module = find("pdf-export").expect("manifest");
        let mapping = module
            .backend
            .copy
            .iter()
            .find(|mapping| mapping.optional)
            .expect("module declares an optional mapping");

        let mut written = Vec::new();
        let mut report = InstallReport::default();
        copy_mapping(&project, dir, mapping, &[], &mut written, &mut report)
            .expect("optional mapping must not fail");

        assert!(written.is_empty());
        assert_eq!(
            std::fs::read_to_string(&target).expect("read"),
            "# project's own twig config\n"
        );

        let _ = std::fs::remove_dir_all(&project);
    }

    #[test]
    fn install_overwrites_unowned_files() {
        let project = std::env::temp_dir().join(format!(
            "eggshell-overwrite-{}-{}",
            std::process::id(),
            now_secs()
        ));
        let target = project.join("frontend/src/api/pdfClient.ts");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("dirs");
        std::fs::write(&target, "// the agent wrote this\n").expect("seed file");

        let dir = module_dir("pdf-export").expect("module directory");
        let module = find("pdf-export").expect("manifest");
        let mut written = Vec::new();
        let mut report = InstallReport::default();
        copy_mapping(
            &project,
            dir,
            &module.frontend.copy[0],
            &[],
            &mut written,
            &mut report,
        )
        .expect("an unowned existing file is overwritten, not refused");

        // The module's content replaced the agent's file.
        assert!(!std::fs::read_to_string(&target)
            .expect("read")
            .contains("the agent wrote this"));
        assert!(report
            .files_overwritten
            .iter()
            .any(|file| file == "frontend/src/api/pdfClient.ts"));

        let _ = std::fs::remove_dir_all(&project);
    }

    /// End-to-end: install into a temp project (no Composer/npm), then load the
    /// skill back through the same path the agent uses.
    #[test]
    fn installed_module_skill_is_loadable() {
        let project = std::env::temp_dir().join(format!(
            "eggshell-skill-{}-{}",
            std::process::id(),
            now_secs()
        ));
        std::fs::create_dir_all(&project).expect("project dir");

        // Copy only, mirroring `install` without the package-manager steps.
        let dir = module_dir("pdf-export").expect("module directory");
        let manifest = find("pdf-export").expect("manifest");
        let mut written = Vec::new();
        let mut report = InstallReport {
            module: manifest.id.clone(),
            ..Default::default()
        };
        for mapping in manifest.backend.copy.iter().chain(manifest.frontend.copy.iter()) {
            copy_mapping(&project, dir, mapping, &[], &mut written, &mut report)
                .expect("copy succeeds");
        }
        let skill_path = install_skill(&project, dir, &manifest, &mut report)
            .expect("skill installs")
            .expect("skill path");

        assert!(project.join(&skill_path).is_file());

        let skills = crate::llm::project_skills(Some(project.to_str().expect("utf8 path")));
        let skill = skills
            .iter()
            .find(|skill| skill.name() == "pdf_export")
            .expect("module skill is discovered");
        assert!(skill.content().contains("PDF Export Skill"));
        assert!(skill.description().contains("PDF"));

        let _ = std::fs::remove_dir_all(&project);
    }

    #[test]
    fn detects_composer_packages_that_did_not_land() {
        let project =
            std::env::temp_dir().join(format!("eggshell-composer-{}-{}", std::process::id(), now_secs()));
        let backend = project.join("backend");
        std::fs::create_dir_all(backend.join("vendor/mpdf/mpdf")).expect("vendor dir");

        let mut require = BTreeMap::new();
        require.insert("mpdf/mpdf".to_string(), "^8.3".to_string());
        require.insert("symfony/twig-bundle".to_string(), "^8.0".to_string());

        // Only mpdf was downloaded; twig-bundle is missing.
        assert_eq!(
            missing_composer_packages(&backend, &require),
            ["symfony/twig-bundle"]
        );

        std::fs::create_dir_all(backend.join("vendor/symfony/twig-bundle")).expect("vendor dir");
        assert!(missing_composer_packages(&backend, &require).is_empty());

        let _ = std::fs::remove_dir_all(&project);
    }

    /// A re-enable at the same version must repair files that were deleted or
    /// edited since the first install, instead of short-circuiting to a no-op.
    #[test]
    fn reinstall_repairs_deleted_and_modified_files() {
        let project = std::env::temp_dir().join(format!(
            "eggshell-repair-{}-{}",
            std::process::id(),
            now_secs()
        ));
        std::fs::create_dir_all(&project).expect("project dir");

        // First install: package steps are skipped, so no composer/npm needed.
        run(install_inner(&project, "pdf-export", false)).expect("first install");
        let template = project.join("backend/src/Service/PdfTemplate/TemplatesPdf.php");
        let client = project.join("frontend/src/api/pdfClient.ts");
        assert!(template.is_file());
        assert!(client.is_file());

        // Break the install: delete one module file, edit another.
        std::fs::remove_file(&template).expect("delete module file");
        std::fs::write(&client, "// modified by the agent\n").expect("modify module file");

        let report = run(install_inner(&project, "pdf-export", false)).expect("repair install");
        assert!(!report.already_installed, "drift must trigger a re-install");
        assert!(template.is_file(), "deleted file is restored");
        assert!(!std::fs::read_to_string(&client)
            .expect("read")
            .contains("modified by the agent"));
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("missing or modified")));

        let _ = std::fs::remove_dir_all(&project);
    }

    /// A same-version re-enable with every file intact must stay a no-op.
    #[test]
    fn reinstall_is_a_noop_when_files_match() {
        let project = std::env::temp_dir().join(format!(
            "eggshell-noop-{}-{}",
            std::process::id(),
            now_secs()
        ));
        std::fs::create_dir_all(&project).expect("project dir");

        run(install_inner(&project, "pdf-export", false)).expect("first install");
        let report = run(install_inner(&project, "pdf-export", false)).expect("second install");

        assert!(report.already_installed);
        assert!(report.files_written.is_empty());
        assert!(report.files_overwritten.is_empty());

        let _ = std::fs::remove_dir_all(&project);
    }
}
