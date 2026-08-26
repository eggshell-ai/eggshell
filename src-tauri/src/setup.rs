//! Dependency setup helpers: where Eggshell keeps the tools it installs itself,
//! and the commands that download them when a package manager cannot.
//!
//! The log the setup screen streams through lives in [`crate::progress`], which
//! project creation shares.

/// The portable build setup falls back to, named once: the archive unpacks into a
/// directory of the same name, and `lib.rs` starts the server from inside it.
#[cfg(windows)]
pub(crate) const MYSQL_DIRECTORY: &str = "mysql-8.0.46-winx64";

pub(crate) const PHP_DIRECTORY: &str = "php";

/// The Node archive contains this one top-level directory. Keeping that name
/// means the fallback can extract directly into Eggshell's app-data directory.
#[cfg(windows)]
pub(crate) const NODE_DIRECTORY: &str = "node-v24.19.0-win-x64";

#[cfg(windows)]
pub(crate) fn managed_node_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("APPDATA").map(|root| {
        std::path::PathBuf::from(root)
            .join("eggshell")
            .join(NODE_DIRECTORY)
    })
}

#[cfg(windows)]
pub(crate) fn managed_node_present() -> bool {
    managed_node_dir().is_some_and(|directory| directory.join("node.exe").is_file())
}

pub(crate) fn managed_php_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("APPDATA").map(|root| {
        std::path::PathBuf::from(root)
            .join("eggshell")
            .join(PHP_DIRECTORY)
    })
}

pub(crate) fn managed_php_present() -> bool {
    managed_php_dir().is_some_and(|directory| directory.join("php.exe").is_file())
}

/// Where the fallback route leaves the server: the base directory holding
/// `bin/mysqld.exe`, `my.ini` and `data`. `None` on the platforms where setup
/// installs MySQL through a package manager instead; those installations are
/// owned outside Eggshell and are never started as an install side effect.
#[cfg(windows)]
pub(crate) fn managed_mysql_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|root| {
        std::path::PathBuf::from(root)
            .join("MySQL")
            .join(MYSQL_DIRECTORY)
    })
}

/// MySQL's Windows installer puts the server in a versioned directory under
/// Program Files without adding its binaries to PATH.
#[cfg(windows)]
pub(crate) fn mysql_in_program_files() -> bool {
    ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"]
        .into_iter()
        .filter_map(std::env::var_os)
        .filter_map(|root| std::fs::read_dir(std::path::PathBuf::from(root).join("MySQL")).ok())
        .flatten()
        .filter_map(Result::ok)
        .any(|entry| entry.path().join("bin").join("mysql.exe").is_file())
}

#[cfg(not(windows))]
pub(crate) fn mysql_in_program_files() -> bool {
    false
}

#[cfg(not(windows))]
pub(crate) fn managed_mysql_dir() -> Option<std::path::PathBuf> {
    None
}

#[cfg(windows)]
pub(crate) fn mysql_fallback_command() -> Vec<String> {
    let url = format!("https://cdn.mysql.com/Downloads/MySQL-8.0/{MYSQL_DIRECTORY}.zip");
    let script = format!(
        r#"$ErrorActionPreference = 'Stop'; $root = Join-Path $env:LOCALAPPDATA 'MySQL'; $base = Join-Path $root '{MYSQL_DIRECTORY}'; $archive = Join-Path $root '{MYSQL_DIRECTORY}.zip'; $data = Join-Path $base 'data'; New-Item -ItemType Directory -Force -Path $root | Out-Null; if (-not (Test-Path (Join-Path $base 'bin\mysqld.exe'))) {{ [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -Uri '{url}' -OutFile $archive -UseBasicParsing; Expand-Archive -LiteralPath $archive -DestinationPath $root -Force; Remove-Item -LiteralPath $archive -Force }}; New-Item -ItemType Directory -Force -Path $data | Out-Null; $baseForIni = $base.Replace('\', '/'); $dataForIni = $data.Replace('\', '/'); @"
[mysqld]
basedir     = $baseForIni
datadir     = $dataForIni
port        = 3306

[client]
port        = 3306
"@ | Set-Content -LiteralPath (Join-Path $base 'my.ini') -Encoding ascii; if (-not (Test-Path (Join-Path $data 'mysql'))) {{ & (Join-Path $base 'bin\mysqld.exe') "--defaults-file=$(Join-Path $base 'my.ini')" '--initialize-insecure'; if ($LASTEXITCODE -ne 0) {{ throw "mysqld initialization failed with exit code $LASTEXITCODE" }} }}"#
    );
    vec![
        "powershell".into(),
        "-NoProfile".into(),
        "-NonInteractive".into(),
        "-ExecutionPolicy".into(),
        "Bypass".into(),
        "-Command".into(),
        script,
    ]
}

#[cfg(windows)]
pub(crate) fn php_fallback_command() -> Vec<String> {
    let url = "https://downloads.php.net/~windows/releases/php-8.5.9-nts-Win32-vs17-x64.zip";
    let settings = "extension_dir = \"ext\"`r`nextension=curl`r`nextension=fileinfo`r`nextension=gd`r`nextension=intl`r`nextension=mbstring`r`nextension=openssl`r`nextension=pdo_mysql`r`nextension=pdo_pgsql`r`nextension=pdo_sqlite`r`nextension=sodium`r`nextension=sqlite3`r`nextension=xsl`r`nextension=zip";
    let script = format!(
        r#"$ErrorActionPreference = 'Stop'; $root = Join-Path $env:APPDATA 'eggshell'; $base = Join-Path $root '{PHP_DIRECTORY}'; $archive = Join-Path $root 'php-8.5.9.zip'; New-Item -ItemType Directory -Force -Path $root | Out-Null; if (-not (Test-Path (Join-Path $base 'php.exe'))) {{ Invoke-WebRequest -Uri '{url}' -OutFile $archive -UseBasicParsing; New-Item -ItemType Directory -Force -Path $base | Out-Null; Expand-Archive -LiteralPath $archive -DestinationPath $base -Force; Remove-Item -LiteralPath $archive -Force }}; $ini = Join-Path $base 'php.ini'; Move-Item -LiteralPath (Join-Path $base 'php.ini-development') -Destination $ini -Force; Add-Content -LiteralPath $ini -Value "`r`n{settings}`r`n" -Encoding ascii"#
    );
    vec![
        "powershell".into(),
        "-NoProfile".into(),
        "-NonInteractive".into(),
        "-ExecutionPolicy".into(),
        "Bypass".into(),
        "-Command".into(),
        script,
    ]
}

/// Downloads the pinned portable Node build when winget is unavailable (or its
/// installer fails). The archive's sole directory is extracted beneath AppData,
/// leaving `node.exe` and `npm.cmd` together in Eggshell's managed location.
#[cfg(windows)]
pub(crate) fn node_fallback_command() -> Vec<String> {
    let url = "https://nodejs.org/dist/v24.19.0/node-v24.19.0-win-x64.zip";
    let script = format!(
        r#"$ErrorActionPreference = 'Stop'; $root = Join-Path $env:APPDATA 'eggshell'; $base = Join-Path $root '{NODE_DIRECTORY}'; $archive = Join-Path $root 'node-v24.19.0-win-x64.zip'; New-Item -ItemType Directory -Force -Path $root | Out-Null; if (-not (Test-Path (Join-Path $base 'node.exe'))) {{ [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -Uri '{url}' -OutFile $archive -UseBasicParsing; Expand-Archive -LiteralPath $archive -DestinationPath $root -Force; Remove-Item -LiteralPath $archive -Force }}"#
    );
    vec![
        "powershell".into(),
        "-NoProfile".into(),
        "-NonInteractive".into(),
        "-ExecutionPolicy".into(),
        "Bypass".into(),
        "-Command".into(),
        script,
    ]
}

#[cfg(not(windows))]
pub(crate) fn mysql_fallback_command() -> Vec<String> {
    Vec::new()
}

#[cfg(not(windows))]
pub(crate) fn managed_php_dir() -> Option<std::path::PathBuf> {
    None
}

#[cfg(not(windows))]
pub(crate) fn managed_node_dir() -> Option<std::path::PathBuf> {
    None
}

#[cfg(not(windows))]
pub(crate) fn managed_node_present() -> bool {
    false
}

#[cfg(not(windows))]
pub(crate) fn managed_php_present() -> bool {
    false
}

#[cfg(not(windows))]
pub(crate) fn php_fallback_command() -> Vec<String> {
    Vec::new()
}

#[cfg(not(windows))]
pub(crate) fn node_fallback_command() -> Vec<String> {
    Vec::new()
}
