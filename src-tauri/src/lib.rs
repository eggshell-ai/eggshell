mod db;
pub mod config;
pub mod llm;
mod tools;

use db::{NewProject, Project, ProjectsRepository, Session, SessionsRepository};
use sqlx::SqlitePool;
use serde::Serialize;
use std::net::{SocketAddr, TcpStream};
use std::process::Command;
use std::sync::Arc;
use tauri::Emitter;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[derive(Debug, Serialize)]
struct DependencyStatus { node: bool, php: bool, symfony: bool, mysql: bool }

#[derive(Debug, Serialize)]
struct InstallOutcome { installed: bool, already_present: bool, command: String, restart_required: bool }

/// Commands that install one dependency: `preparation` runs first and may fail
/// harmlessly (index refreshes), then the first `attempts` entry that leaves the
/// executable on PATH wins, and `follow_up` runs once something is installed —
/// service registration, which is not part of "is it installed".
struct InstallPlan {
    preparation: Vec<Vec<String>>,
    attempts: Vec<Vec<String>>,
    follow_up: Vec<Vec<String>>,
    hint: String,
}

/// Symfony ships no Windows installer we can drive silently, so setup unpacks a
/// pinned release archive itself instead of tracking whatever is newest.
#[cfg(windows)]
const SYMFONY_CLI_VERSION: &str = "5.17.1";

/// The directory Eggshell owns for binaries it installs by hand. `symfony.rs`
/// puts it on PATH before running any Symfony command.
#[cfg(windows)]
pub(crate) fn managed_bin_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("APPDATA").map(|appdata| std::path::PathBuf::from(appdata).join("eggshell"))
}

#[cfg(windows)]
fn managed_symfony_present() -> bool {
    managed_bin_dir().is_some_and(|directory| directory.join("symfony.exe").is_file())
}

#[cfg(not(windows))]
fn managed_symfony_present() -> bool {
    false
}

fn executable_in_process_path(executable: &str) -> bool {
    let locator = if cfg!(windows) { "where" } else { "which" };
    match Command::new(locator).arg(executable).output() {
        Ok(output) => output.status.success(),
        Err(error) => {
            println!("dependencies: could not run {locator} for {executable}: {error}");
            false
        }
    }
}

/// Installers only extend PATH for *new* processes, so a tool installed while
/// Eggshell is running stays invisible to `where`/`which` until a restart. Look
/// at the freshly written environment too before calling a dependency missing.
#[cfg(windows)]
fn executable_in_installed_path(executable: &str) -> bool {
    let script = format!(
        "$env:PATH = [Environment]::GetEnvironmentVariable('PATH', 'Machine') + ';' + \
         [Environment]::GetEnvironmentVariable('PATH', 'User'); \
         if (Get-Command '{executable}' -ErrorAction SilentlyContinue) {{ exit 0 }} else {{ exit 1 }}"
    );
    match Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
    {
        Ok(output) => output.status.success(),
        Err(error) => {
            println!("dependencies: could not refresh PATH for {executable}: {error}");
            false
        }
    }
}

/// Package managers install into prefixes a windowed process often does not
/// inherit: Finder hands apps a bare PATH and Linuxbrew lives under /home.
#[cfg(not(windows))]
fn executable_in_installed_path(executable: &str) -> bool {
    // get.symfony.com installs into $HOME/.symfony5/bin and leaves adding that to
    // PATH up to the user, so look there as well as in the system prefixes.
    let symfony_installer_prefix = std::env::var_os("HOME")
        .map(|home| std::path::PathBuf::from(home).join(".symfony5").join("bin"));

    [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/home/linuxbrew/.linuxbrew/bin",
        "/usr/bin",
        // Debian keeps daemons such as mysqld here, off the PATH of a plain user.
        "/usr/sbin",
        "/snap/bin",
    ]
    .into_iter()
    .map(std::path::PathBuf::from)
    .chain(symfony_installer_prefix)
    .any(|directory| directory.join(executable).is_file())
}

fn executable_in_path(executable: &str) -> bool {
    executable_in_process_path(executable) || executable_in_installed_path(executable)
}

/// MySQL's Windows installer unpacks the server into a versioned directory under
/// Program Files and leaves PATH untouched, so a finished install is invisible to
/// both `where mysql` and the environment the installer wrote.
#[cfg(windows)]
fn mysql_in_program_files() -> bool {
    ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"]
        .into_iter()
        .filter_map(std::env::var_os)
        .filter_map(|root| std::fs::read_dir(std::path::PathBuf::from(root).join("MySQL")).ok())
        .flatten()
        .filter_map(Result::ok)
        .any(|entry| entry.path().join("bin").join("mysql.exe").is_file())
}

#[cfg(not(windows))]
fn mysql_in_program_files() -> bool {
    false
}

/// Setup installs the MySQL *server*, and packagers disagree on whether the
/// client (`mysql`) lands on PATH beside the daemon (`mysqld`), so either one
/// counts as MySQL being here.
fn mysql_present() -> bool {
    executable_in_path("mysql") || executable_in_path("mysqld") || mysql_in_program_files()
}

#[tauri::command]
fn detect_dependencies() -> DependencyStatus {
    DependencyStatus {
        node: executable_in_path("node"),
        php: executable_in_path("php"),
        symfony: executable_in_path("symfony") || managed_symfony_present(),
        mysql: mysql_present(),
    }
}

fn unsupported_dependency(dependency: &str) -> String {
    format!("Automated installation is not available for \"{dependency}\" yet.")
}

fn executable_for(dependency: &str) -> Result<&'static str, String> {
    match dependency {
        "node" => Ok("node"),
        "php" => Ok("php"),
        "symfony" => Ok("symfony"),
        "mysql" => Ok("mysql"),
        other => Err(unsupported_dependency(other)),
    }
}

/// PATH is not the only place a dependency can legitimately live: the Symfony CLI
/// is unpacked into a directory Eggshell owns and adds to PATH on its own, and
/// MySQL announces itself through the daemon or its install directory.
fn dependency_present(dependency: &str, executable: &str) -> bool {
    match dependency {
        "symfony" => managed_symfony_present() || executable_in_path(executable),
        "mysql" => mysql_present(),
        _ => executable_in_path(executable),
    }
}

/// A dependency Eggshell manages itself is usable immediately, because Eggshell
/// prepends its directory to PATH every time it runs a command. Projects reach
/// MySQL over TCP rather than by running its client, so PATH never matters there.
fn requires_restart(dependency: &str, executable: &str) -> bool {
    if dependency == "symfony" && managed_symfony_present() { return false; }
    if dependency == "mysql" { return false; }
    !executable_in_process_path(executable)
}

/// Symfony publishes its Windows CLI only as a release archive, so setup fetches
/// the pinned build and unpacks it into the directory Eggshell manages.
#[cfg(windows)]
fn symfony_download_plan() -> Result<InstallPlan, String> {
    let directory = managed_bin_dir().ok_or_else(|| {
        "APPDATA is not set, so Eggshell has nowhere to keep the Symfony CLI.".to_string()
    })?;
    let url = format!(
        "https://github.com/symfony-cli/symfony-cli/releases/download/v{SYMFONY_CLI_VERSION}/symfony-cli_windows_386.zip"
    );
    // The path is interpolated into a single-quoted PowerShell literal, and user
    // profiles such as C:\Users\O'Brien\AppData\Roaming would close it early.
    let quoted_directory = directory.display().to_string().replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         $directory = '{quoted_directory}'; \
         New-Item -ItemType Directory -Force -Path $directory | Out-Null; \
         $archive = Join-Path $directory 'symfony-cli.zip'; \
         [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; \
         Invoke-WebRequest -Uri '{url}' -OutFile $archive -UseBasicParsing; \
         Expand-Archive -LiteralPath $archive -DestinationPath $directory -Force; \
         Remove-Item -LiteralPath $archive -Force"
    );

    Ok(InstallPlan {
        preparation: Vec::new(),
        attempts: vec![vec![
            "powershell".to_string(),
            "-NoProfile".to_string(),
            "-NonInteractive".to_string(),
            "-Command".to_string(),
            script,
        ]],
        follow_up: Vec::new(),
        hint: format!(
            "Eggshell downloads Symfony CLI {SYMFONY_CLI_VERSION} into {}, which needs access to github.com.",
            directory.display()
        ),
    })
}

/// MySQL registers a service only when its installer also configures the server,
/// so Eggshell asks for automatic start-up and gives up quietly when the winget
/// package left nothing to start.
#[cfg(windows)]
fn mysql_service_commands() -> Vec<Vec<String>> {
    let script = "$service = Get-Service -Name 'MySQL*' -ErrorAction SilentlyContinue | Select-Object -First 1; \
         if (-not $service) { Write-Error 'no MySQL service is registered'; exit 1 }; \
         Set-Service -Name $service.Name -StartupType Automatic; \
         if ($service.Status -ne 'Running') { Start-Service -Name $service.Name }";

    vec![vec![
        "powershell".to_string(),
        "-NoProfile".to_string(),
        "-NonInteractive".to_string(),
        "-Command".to_string(),
        script.to_string(),
    ]]
}

#[cfg(windows)]
fn install_plan(dependency: &str) -> Result<InstallPlan, String> {
    if dependency == "symfony" { return symfony_download_plan(); }

    if !executable_in_path("winget") {
        return Err("winget was not found. Install \"App Installer\" from the Microsoft Store, then run setup again.".to_string());
    }

    // Node LTS is 24.x and Symfony needs PHP 8.2+, so both defaults clear the
    // versions the setup screen asks for; PHP falls back a minor release in case
    // the newest manifest is unavailable.
    let packages: &[&str] = match dependency {
        "node" => &["OpenJS.NodeJS.LTS"],
        "php" => &["PHP.PHP.8.4", "PHP.PHP.8.3"],
        "mysql" => &["Oracle.MySQL"],
        other => return Err(unsupported_dependency(other)),
    };

    let attempts = packages
        .iter()
        .map(|package| {
            [
                "winget",
                "install",
                "--id",
                package,
                "--exact",
                "--source",
                "winget",
                "--silent",
                "--accept-package-agreements",
                "--accept-source-agreements",
                "--disable-interactivity",
            ]
            .map(String::from)
            .to_vec()
        })
        .collect();

    Ok(InstallPlan {
        preparation: Vec::new(),
        attempts,
        follow_up: if dependency == "mysql" { mysql_service_commands() } else { Vec::new() },
        hint: "winget may ask for administrator approval; accept the prompt when it appears.".to_string(),
    })
}

#[cfg(target_os = "macos")]
fn install_plan(dependency: &str) -> Result<InstallPlan, String> {
    // Symfony's own installer script is the only supported route on macOS, so it
    // runs before the Homebrew lookup that the other formulas depend on.
    if dependency == "symfony" {
        return Ok(InstallPlan {
            preparation: Vec::new(),
            attempts: vec![vec![
                "bash".to_string(),
                "-c".to_string(),
                "set -o pipefail; curl -sS https://get.symfony.com/cli/installer | bash".to_string(),
            ]],
            follow_up: Vec::new(),
            hint: "The Symfony installer downloads from get.symfony.com and writes to $HOME/.symfony5.".to_string(),
        });
    }

    let brew = ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
        .into_iter()
        .find(|path| std::path::Path::new(path).is_file())
        .map(String::from)
        .or_else(|| executable_in_process_path("brew").then(|| "brew".to_string()))
        .ok_or_else(|| "Homebrew was not found. Install it from https://brew.sh, then run setup again.".to_string())?;

    let formula = match dependency {
        "node" => "node",
        "php" => "php",
        "mysql" => "mysql",
        other => return Err(unsupported_dependency(other)),
    };

    // `brew services` registers a launchd job, so MySQL comes back after a reboot
    // the way the enabled systemd unit does on Linux.
    let follow_up = match dependency {
        "mysql" => vec![vec![brew.clone(), "services".to_string(), "start".to_string(), "mysql".to_string()]],
        _ => Vec::new(),
    };

    Ok(InstallPlan {
        preparation: Vec::new(),
        attempts: vec![vec![brew, "install".to_string(), formula.to_string()]],
        follow_up,
        hint: "Homebrew must be able to write to its prefix (/opt/homebrew on Apple silicon, /usr/local on Intel).".to_string(),
    })
}

/// Installing system packages needs root, which a windowed app cannot type a
/// password for: prefer pkexec's graphical prompt and fall back to passwordless
/// sudo before giving up.
#[cfg(all(unix, not(target_os = "macos")))]
fn elevation_prefix() -> Result<Vec<String>, String> {
    let is_root = Command::new("id")
        .arg("-u")
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "0")
        .unwrap_or(false);

    if is_root { return Ok(Vec::new()); }
    if executable_in_path("pkexec") { return Ok(vec!["pkexec".to_string()]); }
    if executable_in_path("sudo") { return Ok(vec!["sudo".to_string(), "-n".to_string()]); }
    Err("Installing packages needs root access, but neither pkexec nor sudo is available.".to_string())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn elevated(elevation: &[String], command: Vec<&str>) -> Vec<String> {
    elevation.iter().cloned().chain(command.into_iter().map(String::from)).collect()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn install_plan(dependency: &str) -> Result<InstallPlan, String> {
    let elevation = elevation_prefix()?;

    // Symfony packages its CLI under a single name, so setup runs apt on every
    // distribution instead of mapping the package per manager as it does below.
    if dependency == "symfony" {
        return Ok(InstallPlan {
            preparation: vec![elevated(&elevation, vec!["apt", "update"])],
            attempts: vec![elevated(&elevation, vec!["apt", "install", "-y", "symfony-cli"])],
            follow_up: Vec::new(),
            hint: "`apt install symfony-cli` needs root access and the Symfony CLI apt repository from https://symfony.com/download.".to_string(),
        });
    }

    // MySQL is an apt-only route as well: the package carries its own systemd
    // unit, so setup only has to name the package and enable the service.
    if dependency == "mysql" {
        if !executable_in_path("apt-get") {
            return Err("Automated MySQL installation needs apt-get, which was not found. Install a MySQL server package with your distribution's package manager, then run setup again.".to_string());
        }

        // apt-get would stop at the package's configuration prompts with no
        // terminal to answer them on, so the frontend is pinned to noninteractive.
        let install = |package| {
            elevated(
                &elevation,
                vec!["env", "DEBIAN_FRONTEND=noninteractive", "apt-get", "install", "-y", package],
            )
        };

        return Ok(InstallPlan {
            preparation: vec![elevated(&elevation, vec!["apt-get", "update"])],
            // Debian keeps `mysql-server` out of main; there the metapackage
            // installs MariaDB, which serves the same `mysql` service name.
            attempts: vec![install("mysql-server"), install("default-mysql-server")],
            // `enable --now` is enable-then-start in one call, which keeps the
            // number of authentication prompts down.
            follow_up: vec![elevated(&elevation, vec!["systemctl", "enable", "--now", "mysql"])],
            hint: "`apt-get install mysql-server` needs root access and a package index that carries MySQL.".to_string(),
        });
    }

    let manager = ["apt-get", "dnf", "pacman", "zypper"]
        .into_iter()
        .find(|manager| executable_in_path(manager))
        .ok_or_else(|| "No supported package manager was found (apt-get, dnf, pacman or zypper).".to_string())?;

    let (preparation, attempts): (Vec<Vec<&str>>, Vec<Vec<&str>>) = match (manager, dependency) {
        ("apt-get", "node") => (vec![vec!["apt-get", "update"]], vec![vec!["apt-get", "install", "-y", "nodejs", "npm"]]),
        ("apt-get", "php") => (vec![vec!["apt-get", "update"]], vec![vec!["apt-get", "install", "-y", "php-cli"]]),
        ("dnf", "node") => (Vec::new(), vec![vec!["dnf", "install", "-y", "nodejs", "npm"]]),
        ("dnf", "php") => (Vec::new(), vec![vec!["dnf", "install", "-y", "php-cli"]]),
        ("pacman", "node") => (Vec::new(), vec![vec!["pacman", "-Sy", "--noconfirm", "nodejs", "npm"]]),
        ("pacman", "php") => (Vec::new(), vec![vec!["pacman", "-Sy", "--noconfirm", "php"]]),
        ("zypper", "node") => (Vec::new(), vec![vec!["zypper", "--non-interactive", "install", "nodejs", "npm"]]),
        ("zypper", "php") => (
            Vec::new(),
            vec![vec!["zypper", "--non-interactive", "install", "php8-cli"], vec!["zypper", "--non-interactive", "install", "php-cli"]],
        ),
        (_, other) => return Err(unsupported_dependency(other)),
    };

    Ok(InstallPlan {
        preparation: preparation.into_iter().map(|command| elevated(&elevation, command)).collect(),
        attempts: attempts.into_iter().map(|command| elevated(&elevation, command)).collect(),
        follow_up: Vec::new(),
        hint: format!("{manager} needs root access; approve the authentication prompt when it appears."),
    })
}

/// Keeps installer noise off the setup screen while still surfacing the cause.
fn installer_tail(details: &str) -> String {
    let cleaned = details.split_whitespace().collect::<Vec<_>>().join(" ");
    let length = cleaned.chars().count();
    if length <= 300 { return cleaned; }
    cleaned.chars().skip(length - 300).collect()
}

fn run_installer(command: &[String]) -> Result<(), String> {
    let (program, arguments) = command
        .split_first()
        .ok_or_else(|| "an install command was empty".to_string())?;
    let label = command.join(" ");
    println!("install_dependency: running {label}");

    let output = Command::new(program)
        .args(arguments)
        .output()
        .map_err(|error| format!("`{label}` could not be started: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let details = if stderr.is_empty() { String::from_utf8_lossy(&output.stdout).trim().to_string() } else { stderr };
        return Err(format!("`{label}` exited with {}. {}", output.status, installer_tail(&details)));
    }

    println!("install_dependency: finished {label}");
    Ok(())
}

fn install_dependency_blocking(dependency: &str) -> Result<InstallOutcome, String> {
    let executable = executable_for(dependency)?;
    if dependency_present(dependency, executable) {
        println!("install_dependency: {dependency} is already installed, skipping");
        return Ok(InstallOutcome {
            installed: true,
            already_present: true,
            command: String::new(),
            restart_required: false,
        });
    }

    let plan = install_plan(dependency)?;
    for command in &plan.preparation {
        if let Err(error) = run_installer(command) {
            println!("install_dependency: preparation step failed, continuing anyway: {error}");
        }
    }

    let mut failures = Vec::new();
    for command in &plan.attempts {
        let label = command.join(" ");
        let outcome = run_installer(command);
        // A non-zero exit can still mean success (winget reports "already
        // installed" that way), so let the PATH lookup have the final say.
        if dependency_present(dependency, executable) {
            // Registering a service is not what the setup screen is checking for,
            // so a failure here is logged rather than turned into a failed install.
            for command in &plan.follow_up {
                if let Err(error) = run_installer(command) {
                    println!("install_dependency: follow-up step failed, continuing anyway: {error}");
                }
            }

            return Ok(InstallOutcome {
                installed: true,
                already_present: false,
                command: label,
                restart_required: requires_restart(dependency, executable),
            });
        }
        match outcome {
            Ok(()) => failures.push(format!("`{label}` succeeded but {executable} is still not on PATH.")),
            Err(error) => failures.push(error),
        }
    }

    Err(format!("{} {}", plan.hint, failures.join(" ")))
}

#[tauri::command]
async fn install_dependency(name: String) -> Result<InstallOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || install_dependency_blocking(&name))
        .await
        .map_err(|error| format!("installation task failed: {error}"))?
}

/// What the repository ships in `config.yaml` where a real value belongs, so the
/// setup screen offers an empty field rather than a placeholder to correct.
const CONFIG_PLACEHOLDER: &str = "...";

/// What the setup screen needs to know at launch: whether to appear at all, and
/// what to prefill. The API key is deliberately not sent back to the frontend.
#[derive(Debug, Serialize)]
struct SetupState { setup_completed: bool, model: String }

#[tauri::command]
fn load_setup_state() -> SetupState {
    match config::ConfigService::load_default() {
        Ok(config) => SetupState {
            setup_completed: config.setup_completed,
            model: if config.ollama.model == CONFIG_PLACEHOLDER { String::new() } else { config.ollama.model },
        },
        // A first launch has no configuration to read yet, which is exactly when
        // setup has to run.
        Err(error) => {
            println!("load_setup_state: {error}; treating setup as incomplete");
            SetupState { setup_completed: false, model: String::new() }
        }
    }
}

#[tauri::command]
fn save_provider_config(
    provider: String,
    model: String,
    api_key: String,
    ollama: tauri::State<'_, Arc<llm::OllamaService>>,
) -> Result<(), String> {
    if provider != "ollama" {
        return Err(format!("\"{provider}\" is not a supported provider yet."));
    }
    let model = model.trim().to_string();
    let api_key = api_key.trim().to_string();
    if model.is_empty() || api_key.is_empty() {
        return Err("A model and an API key are required.".to_string());
    }

    // Setup writes every field the file holds, and a first launch has nothing to
    // read, so an unreadable configuration is replaced rather than fatal.
    let mut config = config::ConfigService::load_default().unwrap_or_else(|error| {
        println!("save_provider_config: {error}; writing a fresh configuration");
        config::AppConfig { ollama: config::OllamaConfig { model: String::new(), api_key: String::new() }, setup_completed: false }
    });
    config.ollama = config::OllamaConfig { model, api_key };
    config.setup_completed = true;
    config::ConfigService::save_default(&config).map_err(|error| error.to_string())?;

    // The agent was built from the configuration read at start-up, so without
    // this the credentials just entered would only take effect after a restart.
    ollama.apply(config.ollama.clone());
    println!("save_provider_config: saved {provider} with model {}", config.ollama.model);
    Ok(())
}

#[tauri::command]
async fn list_projects(pool: tauri::State<'_, SqlitePool>) -> Result<Vec<Project>, String> {
    ProjectsRepository::list(pool.inner())
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn create_project(
    project: NewProject,
    pool: tauri::State<'_, SqlitePool>,
) -> Result<Project, String> {
    if project.title.trim().is_empty() || project.path.trim().is_empty() {
        return Err("A project title and folder are required.".to_string());
    }

    ProjectsRepository::create(pool.inner(), project)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn delete_project(id: i64, pool: tauri::State<'_, SqlitePool>) -> Result<(), String> {
    ProjectsRepository::delete(pool.inner(), id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn start_project(id: i64, pool: tauri::State<'_, SqlitePool>) -> Result<(), String> {
    ProjectsRepository::start(pool.inner(), id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn list_sessions(project_id: i64, pool: tauri::State<'_, SqlitePool>) -> Result<Vec<Session>, String> {
    SessionsRepository::list(pool.inner(), project_id).await.map_err(|error| error.to_string())
}

#[tauri::command]
async fn delete_session(project_id: i64, id: i64, pool: tauri::State<'_, SqlitePool>) -> Result<(), String> {
    SessionsRepository::delete(pool.inner(), project_id, id).await.map_err(|error| error.to_string())
}

#[tauri::command]
async fn send_message(project_id: i64, session_id: Option<i64>, message: String, app: tauri::AppHandle, pool: tauri::State<'_, SqlitePool>, agent: tauri::State<'_, llm::AgentService>) -> Result<Session, String> {
    let message = message.trim().to_string();
    if message.is_empty() { return Err("A message is required.".to_string()); }
    let event_sink = Arc::new(move |payload| { let _ = app.emit("agent-event", payload); });
    SessionsRepository::save_exchange(pool.inner(), project_id, session_id, message, agent.inner(), event_sink).await.map_err(|error| error.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let pool = tauri::async_runtime::block_on(db::initialize())
        .expect("failed to initialize SQLite database");
    // A missing or unconfigured file is what a first launch looks like: start
    // with empty provider settings and let the setup screen fill them in.
    let config = config::ConfigService::load_default().unwrap_or_else(|error| {
        println!("run: {error}; starting with empty provider settings");
        config::AppConfig {
            ollama: config::OllamaConfig { model: String::new(), api_key: String::new() },
            setup_completed: false,
        }
    });
    let ollama = Arc::new(llm::OllamaService::new(config.ollama));
    let agent = llm::AgentService::new(ollama.clone());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(pool)
        .manage(agent)
        .manage(ollama)
        .invoke_handler(tauri::generate_handler![
            greet,
            detect_dependencies,
            install_dependency,
            load_setup_state,
            save_provider_config,
            list_projects,
            create_project,
            delete_project,
            start_project,
            list_sessions,
            delete_session,
            send_message
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
