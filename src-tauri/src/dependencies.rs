use serde::Serialize;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tauri::Emitter;

use crate::logger;
use crate::progress::{self, ProgressLog};
use crate::setup;

#[derive(Debug, Clone, Serialize)]
pub struct DependencyStatus {
    pub node: bool,
    pub php: bool,
    pub composer: bool,
    pub symfony: bool,
    pub mysql: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallOutcome {
    pub installed: bool,
    pub already_present: bool,
    pub command: String,
    pub restart_required: bool,
}

/// Commands that install one dependency: `preparation` runs first and may fail
/// harmlessly (index refreshes), then the first `attempts` entry that leaves the
/// executable on PATH wins, and `follow_up` runs once something is installed —
/// service registration, which is not part of "is it installed".
pub struct InstallPlan {
    pub preparation: Vec<Vec<String>>,
    pub attempts: Vec<Vec<String>>,
    pub follow_up: Vec<Vec<String>>,
    pub hint: String,
}

#[derive(Debug, Clone)]
pub struct DetectionDetails {
    pub present: bool,
    pub method: Option<&'static str>,
    pub path: Option<PathBuf>,
}

impl DetectionDetails {
    pub fn not_found() -> Self {
        Self {
            present: false,
            method: None,
            path: None,
        }
    }

    pub fn found(method: &'static str, path: impl Into<PathBuf>) -> Self {
        Self {
            present: true,
            method: Some(method),
            path: Some(path.into()),
        }
    }
}

/// Symfony ships no Windows installer we can drive silently, so setup unpacks a
/// pinned release archive itself instead of tracking whatever is newest.
#[cfg(windows)]
const SYMFONY_CLI_VERSION: &str = "5.17.1";

/// The directory Eggshell owns for binaries it installs by hand. `symfony.rs`
/// puts it on PATH before running any Symfony command.
#[cfg(windows)]
pub fn managed_bin_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|appdata| PathBuf::from(appdata).join("eggshell"))
}

#[cfg(not(windows))]
pub fn managed_bin_dir() -> Option<PathBuf> {
    None
}

/// Composer's installer writes a PHAR, so Eggshell also creates this launcher in
/// its managed bin directory. Keeping both together lets Symfony invoke the
/// normal `composer` command without special cases.
#[cfg(windows)]
pub fn managed_composer_present() -> bool {
    managed_composer_path().is_some()
}

#[cfg(not(windows))]
pub fn managed_composer_present() -> bool {
    false
}

#[cfg(windows)]
pub fn managed_composer_path() -> Option<PathBuf> {
    let base = managed_bin_dir()?;
    let bat = base.join("composer.bat");
    let phar = base.join("composer.phar");
    (bat.is_file() && phar.is_file()).then_some(bat)
}

#[cfg(not(windows))]
pub fn managed_composer_path() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
pub fn managed_symfony_present() -> bool {
    managed_symfony_path().is_some()
}

#[cfg(not(windows))]
pub fn managed_symfony_present() -> bool {
    false
}

#[cfg(windows)]
pub fn managed_symfony_path() -> Option<PathBuf> {
    let path = managed_bin_dir()?.join("symfony.exe");
    path.is_file().then_some(path)
}

#[cfg(not(windows))]
pub fn managed_symfony_path() -> Option<PathBuf> {
    None
}

/// The daemon setup's fallback route downloaded, when that route ran. `None` when
/// MySQL arrived through winget, Homebrew or apt instead: those register a service
/// that owns the daemon, and a copy started behind its back would do nothing but
/// fight it over the data directory.
pub fn managed_mysqld() -> Option<PathBuf> {
    let daemon = setup::managed_mysql_dir()?
        .join("bin")
        .join(if cfg!(windows) {
            "mysqld.exe"
        } else {
            "mysqld"
        });
    daemon.is_file().then_some(daemon)
}

/// Locates an executable in the current process PATH using `where` or `which`.
pub fn locate_in_process_path(executable: &str) -> Option<PathBuf> {
    let locator = if cfg!(windows) { "where" } else { "which" };
    match Command::new(locator).arg(executable).output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .map(PathBuf::from)
        }
        Ok(_) => None,
        Err(error) => {
            logger::Logger::global().warning(
                format!("dependencies: could not run {locator} for {executable}: {error}"),
                false,
            );
            None
        }
    }
}

pub fn executable_in_process_path(executable: &str) -> bool {
    locate_in_process_path(executable).is_some()
}

/// Installers only extend PATH for *new* processes, so a tool installed while
/// Eggshell is running stays invisible to `where`/`which` until a restart. Look
/// at the freshly written environment too before calling a dependency missing.
#[cfg(windows)]
pub fn locate_in_installed_path(executable: &str) -> Option<PathBuf> {
    let script = format!(
        "$env:PATH = [Environment]::GetEnvironmentVariable('PATH', 'Machine') + ';' + \
         [Environment]::GetEnvironmentVariable('PATH', 'User'); \
         $cmd = Get-Command '{executable}' -ErrorAction SilentlyContinue; \
         if ($cmd) {{ Write-Output $cmd.Source; exit 0 }} else {{ exit 1 }}"
    );
    match Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
    {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .map(PathBuf::from)
        }
        Ok(_) => None,
        Err(error) => {
            logger::Logger::global().warning(
                format!("dependencies: could not refresh PATH for {executable}: {error}"),
                false,
            );
            None
        }
    }
}

#[cfg(not(windows))]
pub fn locate_in_installed_path(executable: &str) -> Option<PathBuf> {
    let symfony_installer_prefix = std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".symfony5").join("bin"));

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
    .map(PathBuf::from)
    .chain(symfony_installer_prefix)
    .map(|directory| directory.join(executable))
    .find(|candidate| candidate.is_file())
}

pub fn locate_in_path(executable: &str) -> Option<(PathBuf, &'static str)> {
    if let Some(path) = locate_in_process_path(executable) {
        return Some((path, "process PATH"));
    }
    if let Some(path) = locate_in_installed_path(executable) {
        return Some((path, "system/user PATH"));
    }
    None
}

pub fn executable_in_path(executable: &str) -> bool {
    locate_in_path(executable).is_some()
}

#[cfg(windows)]
pub fn locate_mysql_in_program_files() -> Option<PathBuf> {
    ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"]
        .into_iter()
        .filter_map(std::env::var_os)
        .filter_map(|root| std::fs::read_dir(PathBuf::from(root).join("MySQL")).ok())
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("bin").join("mysql.exe"))
        .find(|candidate| candidate.is_file())
}

#[cfg(not(windows))]
pub fn locate_mysql_in_program_files() -> Option<PathBuf> {
    None
}

pub fn detect_node() -> DetectionDetails {
    if setup::managed_node_present() {
        if let Some(dir) = setup::managed_node_dir() {
            return DetectionDetails::found("Eggshell managed directory", dir.join("node.exe"));
        }
    }
    if let Some((path, method)) = locate_in_path("node") {
        return DetectionDetails::found(method, path);
    }
    DetectionDetails::not_found()
}

pub fn detect_php() -> DetectionDetails {
    if setup::managed_php_present() {
        if let Some(dir) = setup::managed_php_dir() {
            return DetectionDetails::found("Eggshell managed directory", dir.join("php.exe"));
        }
    }
    if let Some((path, method)) = locate_in_path("php") {
        return DetectionDetails::found(method, path);
    }
    DetectionDetails::not_found()
}

pub fn detect_composer() -> DetectionDetails {
    #[cfg(windows)]
    if let Some(path) = managed_composer_path() {
        return DetectionDetails::found("Eggshell managed directory", path);
    }
    if let Some((path, method)) = locate_in_path("composer") {
        return DetectionDetails::found(method, path);
    }
    DetectionDetails::not_found()
}

pub fn detect_symfony() -> DetectionDetails {
    if let Some((path, method)) = locate_in_path("symfony") {
        return DetectionDetails::found(method, path);
    }
    #[cfg(windows)]
    if let Some(path) = managed_symfony_path() {
        return DetectionDetails::found("Eggshell managed directory", path);
    }
    DetectionDetails::not_found()
}

pub fn detect_mysql() -> DetectionDetails {
    if let Some((path, method)) = locate_in_path("mysql") {
        return DetectionDetails::found(method, path);
    }
    if let Some((path, method)) = locate_in_path("mysqld") {
        return DetectionDetails::found(method, path);
    }
    if let Some(path) = locate_mysql_in_program_files() {
        return DetectionDetails::found("Program Files", path);
    }
    if let Some(path) = managed_mysqld() {
        return DetectionDetails::found("Eggshell managed directory", path);
    }
    DetectionDetails::not_found()
}

#[derive(Debug, Clone, Serialize)]
pub struct DependencyDetectionEvent {
    pub key: String,
    pub present: bool,
}

pub fn detect_dependencies_blocking(app: &tauri::AppHandle, log: &ProgressLog) -> DependencyStatus {
    let check_and_notify = |key: &'static str, name: &'static str, details: DetectionDetails| {
        let _ = app.emit(
            "dependency-detected",
            DependencyDetectionEvent {
                key: key.to_string(),
                present: details.present,
            },
        );

        if details.present {
            let method = details.method.unwrap_or("unknown");
            let path_str = details
                .path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            log.line(
                "info",
                format!("{name} is ready (found via {method}: {path_str})"),
            );
            logger::Logger::global().info(
                format!("dependencies: {name} detected via {method} at {path_str}"),
                false,
            );
        } else {
            log.line("info", format!("{name} was not found"));
            logger::Logger::global().info(format!("dependencies: {name} not found"), false);
        }

        details.present
    };

    let node = check_and_notify("node", "Node JS", detect_node());
    let php = check_and_notify("php", "PHP", detect_php());
    let composer = check_and_notify("composer", "Composer", detect_composer());
    let symfony = check_and_notify("symfony", "Symfony CLI", detect_symfony());
    let mysql = check_and_notify("mysql", "MySQL", detect_mysql());

    DependencyStatus {
        node,
        php,
        composer,
        symfony,
        mysql,
    }
}

pub fn mysql_present() -> bool {
    detect_mysql().present
}

pub fn dependency_present(dependency: &str, executable: &str) -> bool {
    match dependency {
        "node" => detect_node().present,
        "php" => detect_php().present,
        "composer" => detect_composer().present,
        "symfony" => detect_symfony().present,
        "mysql" => detect_mysql().present,
        _ => executable_in_path(executable),
    }
}

pub fn unsupported_dependency(dependency: &str) -> String {
    format!("Automated installation is not available for \"{dependency}\" yet.")
}

pub fn executable_for(dependency: &str) -> Result<&'static str, String> {
    match dependency {
        "node" => Ok("node"),
        "php" => Ok("php"),
        "composer" => Ok("composer"),
        "symfony" => Ok("symfony"),
        "mysql" => Ok("mysql"),
        other => Err(unsupported_dependency(other)),
    }
}

/// A dependency Eggshell manages itself is usable immediately, because Eggshell
/// prepends its directory to PATH every time it runs a command. Projects reach
/// MySQL over TCP rather than by running its client, so PATH never matters there.
pub fn requires_restart(dependency: &str, executable: &str) -> bool {
    if dependency == "node" && setup::managed_node_present() {
        return false;
    }
    if dependency == "php" && setup::managed_php_present() {
        return false;
    }
    if dependency == "composer" && managed_composer_present() {
        return false;
    }
    if dependency == "symfony" && managed_symfony_present() {
        return false;
    }
    if dependency == "mysql" {
        return false;
    }
    !executable_in_process_path(executable)
}

/// Make the portable Node, PHP and Composer available to every command Eggshell starts
/// during this launch. This is deliberately process-local: it does not rewrite
/// the user's PATH.
pub fn add_managed_tools_to_process_path() {
    #[cfg(windows)]
    {
        let mut directories = Vec::new();
        if setup::managed_node_present() {
            if let Some(directory) = setup::managed_node_dir() {
                directories.push(directory);
            }
        }
        if managed_composer_present() {
            if let Some(directory) = managed_bin_dir() {
                directories.push(directory);
            }
        }
        if setup::managed_php_present() {
            if let Some(directory) = setup::managed_php_dir() {
                directories.push(directory);
            }
        }
        if directories.is_empty() {
            return;
        }

        let existing = std::env::var_os("PATH").unwrap_or_default();
        let mut paths = directories;
        paths.extend(std::env::split_paths(&existing));
        if let Ok(path) = std::env::join_paths(paths) {
            std::env::set_var("PATH", path);
        }
    }
}

/// The Composer installer can run immediately after portable PHP is installed,
/// before the process-wide PATH has been refreshed.
pub fn add_managed_tools_to_command_path(command: &mut Command) {
    #[cfg(windows)]
    {
        let mut directories = Vec::new();
        if setup::managed_node_present() {
            if let Some(directory) = setup::managed_node_dir() {
                directories.push(directory);
            }
        }
        if managed_composer_present() {
            if let Some(directory) = managed_bin_dir() {
                directories.push(directory);
            }
        }
        if setup::managed_php_present() {
            if let Some(directory) = setup::managed_php_dir() {
                directories.push(directory);
            }
        }
        if directories.is_empty() {
            return;
        }

        let existing = std::env::var_os("PATH").unwrap_or_default();
        directories.extend(std::env::split_paths(&existing));
        if let Ok(path) = std::env::join_paths(directories) {
            command.env("PATH", path);
        }
    }
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
    let quoted_directory = directory.display().to_string().replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         $env:PATH = $env:PATH + ';' + [Environment]::GetEnvironmentVariable('PATH', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('PATH', 'User'); \
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

/// Composer's official installer produces `composer.phar`. Run it in Eggshell's
/// app-data bin directory and add a tiny launcher so Windows resolves `composer`
/// from PATH like a regular executable. The installer is intentionally used
/// without a hash check as a temporary compatibility workaround.
#[cfg(windows)]
fn composer_download_plan() -> Result<InstallPlan, String> {
    let directory = managed_bin_dir().ok_or_else(|| {
        "APPDATA is not set, so Eggshell has nowhere to keep Composer.".to_string()
    })?;
    let quoted_directory = directory.display().to_string().replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; \
         $directory = '{quoted_directory}'; \
         New-Item -ItemType Directory -Force -Path $directory | Out-Null; \
         Push-Location $directory; \
         try {{ \
           Invoke-WebRequest -Uri 'https://getcomposer.org/installer' -OutFile 'composer-setup.php' -UseBasicParsing; \
           & php composer-setup.php; \
           if (-not (Test-Path (Join-Path $directory 'composer.phar'))) {{ throw 'composer.phar was not created by installer' }}; \
           Set-Content -LiteralPath (Join-Path $directory 'composer.bat') -Value '@php \"%~dp0composer.phar\" %*' -Encoding ascii \
         }} finally {{ \
           if (Test-Path 'composer-setup.php') {{ Remove-Item -LiteralPath 'composer-setup.php' -Force }}; \
           Pop-Location \
         }}"
    );

    Ok(InstallPlan {
        preparation: Vec::new(),
        attempts: vec![vec![
            "powershell".to_string(),
            "-NoProfile".to_string(),
            "-NonInteractive".to_string(),
            "-ExecutionPolicy".to_string(),
            "Bypass".to_string(),
            "-Command".to_string(),
            script,
        ]],
        follow_up: Vec::new(),
        hint: format!(
            "Eggshell installs Composer into {} using getcomposer.org; PHP must be installed first.",
            directory.display()
        ),
    })
}

#[cfg(windows)]
pub fn install_plan(dependency: &str) -> Result<InstallPlan, String> {
    if dependency == "symfony" {
        return symfony_download_plan();
    }
    if dependency == "composer" {
        return composer_download_plan();
    }

    if dependency == "node" && !executable_in_path("winget") {
        return Ok(InstallPlan {
            preparation: Vec::new(),
            attempts: vec![setup::node_fallback_command()],
            follow_up: Vec::new(),
            hint: "Eggshell downloads portable Node JS from nodejs.org.".to_string(),
        });
    }

    if dependency == "mysql" && !executable_in_path("winget") {
        return Ok(InstallPlan {
            preparation: Vec::new(),
            attempts: vec![setup::mysql_fallback_command()],
            follow_up: Vec::new(),
            hint: "Eggshell downloads the portable MySQL server from cdn.mysql.com.".to_string(),
        });
    }

    if dependency == "php" && !executable_in_path("winget") {
        return Ok(InstallPlan {
            preparation: Vec::new(),
            attempts: vec![setup::php_fallback_command()],
            follow_up: Vec::new(),
            hint: "Eggshell downloads portable PHP from downloads.php.net.".to_string(),
        });
    }

    if !executable_in_path("winget") {
        return Err("winget was not found. Install \"App Installer\" from the Microsoft Store, then run setup again.".to_string());
    }

    let packages: &[&str] = match dependency {
        "node" => &["OpenJS.NodeJS.LTS"],
        "php" => &["PHP.PHP.8.4", "PHP.PHP.8.3"],
        "composer" => &["Composer.Composer"],
        "mysql" => &["Oracle.MySQL"],
        other => return Err(unsupported_dependency(other)),
    };

    let mut attempts: Vec<Vec<String>> = packages
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
    if dependency == "mysql" {
        attempts.push(setup::mysql_fallback_command());
    }
    if dependency == "php" {
        attempts.push(setup::php_fallback_command());
    }
    if dependency == "node" {
        attempts.push(setup::node_fallback_command());
    }

    Ok(InstallPlan {
        preparation: Vec::new(),
        attempts,
        follow_up: Vec::new(),
        hint: "winget may ask for administrator approval; accept the prompt when it appears."
            .to_string(),
    })
}

#[cfg(target_os = "macos")]
pub fn install_plan(dependency: &str) -> Result<InstallPlan, String> {
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
        .ok_or_else(|| {
            "Homebrew was not found. Install it from https://brew.sh, then run setup again."
                .to_string()
        })?;

    let formula = match dependency {
        "node" => "node",
        "php" => "php",
        "composer" => "composer",
        "mysql" => "mysql",
        other => return Err(unsupported_dependency(other)),
    };

    let follow_up = match dependency {
        "mysql" => vec![vec![
            brew.clone(),
            "services".to_string(),
            "start".to_string(),
            "mysql".to_string(),
        ]],
        _ => Vec::new(),
    };

    Ok(InstallPlan {
        preparation: Vec::new(),
        attempts: vec![vec![brew, "install".to_string(), formula.to_string()]],
        follow_up,
        hint: "Homebrew must be able to write to its prefix (/opt/homebrew on Apple silicon, /usr/local on Intel).".to_string(),
    })
}

#[cfg(all(unix, not(target_os = "macos")))]
fn elevation_prefix() -> Result<Vec<String>, String> {
    let is_root = Command::new("id")
        .arg("-u")
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "0")
        .unwrap_or(false);

    if is_root {
        return Ok(Vec::new());
    }
    if executable_in_path("pkexec") {
        return Ok(vec!["pkexec".to_string()]);
    }
    if executable_in_path("sudo") {
        return Ok(vec!["sudo".to_string(), "-n".to_string()]);
    }
    Err(
        "Installing packages needs root access, but neither pkexec nor sudo is available."
            .to_string(),
    )
}

#[cfg(all(unix, not(target_os = "macos")))]
fn elevated(elevation: &[String], command: Vec<&str>) -> Vec<String> {
    elevation
        .iter()
        .cloned()
        .chain(command.into_iter().map(String::from))
        .collect()
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn install_plan(dependency: &str) -> Result<InstallPlan, String> {
    let elevation = elevation_prefix()?;

    if dependency == "symfony" {
        return Ok(InstallPlan {
            preparation: vec![elevated(&elevation, vec!["apt", "update"])],
            attempts: vec![elevated(&elevation, vec!["apt", "install", "-y", "symfony-cli"])],
            follow_up: Vec::new(),
            hint: "`apt install symfony-cli` needs root access and the Symfony CLI apt repository from https://symfony.com/download.".to_string(),
        });
    }

    if dependency == "mysql" {
        if !executable_in_path("apt-get") {
            return Err("Automated MySQL installation needs apt-get, which was not found. Install a MySQL server package with your distribution's package manager, then run setup again.".to_string());
        }

        let install = |package| {
            elevated(
                &elevation,
                vec![
                    "env",
                    "DEBIAN_FRONTEND=noninteractive",
                    "apt-get",
                    "install",
                    "-y",
                    package,
                ],
            )
        };

        return Ok(InstallPlan {
            preparation: vec![elevated(&elevation, vec!["apt-get", "update"])],
            attempts: vec![install("mysql-server"), install("default-mysql-server")],
            follow_up: vec![elevated(&elevation, vec!["systemctl", "enable", "--now", "mysql"])],
            hint: "`apt-get install mysql-server` needs root access and a package index that carries MySQL.".to_string(),
        });
    }

    let manager = ["apt-get", "dnf", "pacman", "zypper"]
        .into_iter()
        .find(|manager| executable_in_path(manager))
        .ok_or_else(|| {
            "No supported package manager was found (apt-get, dnf, pacman or zypper).".to_string()
        })?;

    let (preparation, attempts): (Vec<Vec<&str>>, Vec<Vec<&str>>) = match (manager, dependency) {
        ("apt-get", "node") => (
            vec![vec!["apt-get", "update"]],
            vec![vec!["apt-get", "install", "-y", "nodejs", "npm"]],
        ),
        ("apt-get", "php") => (
            vec![vec!["apt-get", "update"]],
            vec![vec!["apt-get", "install", "-y", "php-cli"]],
        ),
        ("apt-get", "composer") => (
            vec![vec!["apt-get", "update"]],
            vec![vec!["apt-get", "install", "-y", "composer"]],
        ),
        ("dnf", "node") => (
            Vec::new(),
            vec![vec!["dnf", "install", "-y", "nodejs", "npm"]],
        ),
        ("dnf", "php") => (Vec::new(), vec![vec!["dnf", "install", "-y", "php-cli"]]),
        ("dnf", "composer") => (Vec::new(), vec![vec!["dnf", "install", "-y", "composer"]]),
        ("pacman", "node") => (
            Vec::new(),
            vec![vec!["pacman", "-Sy", "--noconfirm", "nodejs", "npm"]],
        ),
        ("pacman", "php") => (
            Vec::new(),
            vec![vec!["pacman", "-Sy", "--noconfirm", "php"]],
        ),
        ("pacman", "composer") => (
            Vec::new(),
            vec![vec!["pacman", "-Sy", "--noconfirm", "composer"]],
        ),
        ("zypper", "node") => (
            Vec::new(),
            vec![vec![
                "zypper",
                "--non-interactive",
                "install",
                "nodejs",
                "npm",
            ]],
        ),
        ("zypper", "php") => (
            Vec::new(),
            vec![
                vec!["zypper", "--non-interactive", "install", "php8-cli"],
                vec!["zypper", "--non-interactive", "install", "php-cli"],
            ],
        ),
        ("zypper", "composer") => (
            Vec::new(),
            vec![vec!["zypper", "--non-interactive", "install", "composer"]],
        ),
        (_, other) => return Err(unsupported_dependency(other)),
    };

    Ok(InstallPlan {
        preparation: preparation
            .into_iter()
            .map(|command| elevated(&elevation, command))
            .collect(),
        attempts: attempts
            .into_iter()
            .map(|command| elevated(&elevation, command))
            .collect(),
        follow_up: Vec::new(),
        hint: format!(
            "{manager} needs root access; approve the authentication prompt when it appears."
        ),
    })
}

/// Keeps installer noise off the setup screen while still surfacing the cause.
pub fn installer_tail(details: &str) -> String {
    let cleaned = details.split_whitespace().collect::<Vec<_>>().join(" ");
    let length = cleaned.chars().count();
    if length <= 300 {
        return cleaned;
    }
    cleaned.chars().skip(length - 300).collect()
}

pub fn run_installer(command: &[String], log: &ProgressLog) -> Result<(), String> {
    let (program, arguments) = command
        .split_first()
        .ok_or_else(|| "an install command was empty".to_string())?;
    let label = command.join(" ");
    log.line("command", format!("$ {label}"));

    let mut process = Command::new(program);
    add_managed_tools_to_command_path(&mut process);
    let mut child = process
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            let message = format!("`{label}` could not be started: {error}");
            log.line("error", message.clone());
            message
        })?;

    let stderr = child.stderr.take();
    let stderr_log = log.clone();
    let reader = std::thread::spawn(move || {
        stderr
            .map(|pipe| progress::pump_output(pipe, "stderr", &stderr_log))
            .unwrap_or_default()
    });
    let stdout_lines = child
        .stdout
        .take()
        .map(|pipe| progress::pump_output(pipe, "stdout", log))
        .unwrap_or_default();
    let stderr_lines = reader.join().unwrap_or_default();

    let status = child
        .wait()
        .map_err(|error| format!("`{label}` could not be waited for: {error}"))?;
    if !status.success() {
        let details = if stderr_lines.is_empty() {
            stdout_lines.join(" ")
        } else {
            stderr_lines.join(" ")
        };
        let message = format!(
            "`{label}` exited with {status}. {}",
            installer_tail(&details)
        );
        log.line("error", message.clone());
        return Err(message);
    }

    log.line("info", format!("finished {label}"));
    Ok(())
}

pub fn install_dependency_blocking(
    dependency: &str,
    log: &ProgressLog,
) -> Result<InstallOutcome, String> {
    let executable = executable_for(dependency)?;
    if dependency_present(dependency, executable) {
        log.line(
            "info",
            format!("{dependency} is already installed, skipping"),
        );
        return Ok(InstallOutcome {
            installed: true,
            already_present: true,
            command: String::new(),
            restart_required: false,
        });
    }

    log.line("info", format!("installing {dependency}"));
    let plan = install_plan(dependency).inspect_err(|error| log.line("error", error.clone()))?;
    for command in &plan.preparation {
        if let Err(error) = run_installer(command, log) {
            log.line(
                "info",
                format!("preparation step failed, continuing anyway: {error}"),
            );
        }
    }

    let mut failures = Vec::new();
    for command in &plan.attempts {
        let label = command.join(" ");
        let outcome = run_installer(command, log);
        if dependency_present(dependency, executable) {
            for command in &plan.follow_up {
                if let Err(error) = run_installer(command, log) {
                    log.line(
                        "info",
                        format!("follow-up step failed, continuing anyway: {error}"),
                    );
                }
            }

            log.line("info", format!("{dependency} is ready"));
            return Ok(InstallOutcome {
                installed: true,
                already_present: false,
                command: label,
                restart_required: requires_restart(dependency, executable),
            });
        }
        match outcome {
            Ok(()) => {
                let message = format!("`{label}` succeeded but {executable} is still not on PATH.");
                log.line("error", message.clone());
                failures.push(message);
            }
            Err(error) => failures.push(error),
        }
    }

    Err(format!("{} {}", plan.hint, failures.join(" ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_mysql_reports_location_if_present() {
        let details = detect_mysql();
        println!("MySQL detection result: {:?}", details);
        if details.present {
            assert!(details.method.is_some());
            assert!(details.path.is_some());
            assert!(details.path.unwrap().is_file());
        }
    }

    #[test]
    fn test_detect_node_returns_details() {
        let details = detect_node();
        println!("Node detection result: {:?}", details);
        if details.present {
            assert!(details.method.is_some());
            assert!(details.path.is_some());
        }
    }
}

