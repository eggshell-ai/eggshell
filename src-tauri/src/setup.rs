//! Dependency setup helpers.

#[cfg(windows)]
pub(crate) const MYSQL_URL: &str = "https://cdn.mysql.com/Downloads/MySQL-8.0/mysql-8.0.46-winx64.zip";

#[cfg(windows)]
pub(crate) fn mysql_fallback_command() -> Vec<String> {
    let script = format!(r#"$ErrorActionPreference = 'Stop'; $root = Join-Path $env:LOCALAPPDATA 'MySQL'; $base = Join-Path $root 'mysql-8.0.46-winx64'; $archive = Join-Path $root 'mysql-8.0.46-winx64.zip'; $data = Join-Path $base 'data'; New-Item -ItemType Directory -Force -Path $root | Out-Null; if (-not (Test-Path (Join-Path $base 'bin\mysqld.exe'))) {{ [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -Uri '{MYSQL_URL}' -OutFile $archive -UseBasicParsing; Expand-Archive -LiteralPath $archive -DestinationPath $root -Force; Remove-Item -LiteralPath $archive -Force }}; New-Item -ItemType Directory -Force -Path $data | Out-Null; $baseForIni = $base.Replace('\', '/'); $dataForIni = $data.Replace('\', '/'); @"
[mysqld]
basedir     = $baseForIni
datadir     = $dataForIni
port        = 3306

[client]
port        = 3306
"@ | Set-Content -LiteralPath (Join-Path $base 'my.ini') -Encoding ascii; if (-not (Test-Path (Join-Path $data 'mysql'))) {{ & (Join-Path $base 'bin\mysqld.exe') '--defaults-file=' + (Join-Path $base 'my.ini') '--initialize-insecure'; if ($LASTEXITCODE -ne 0) {{ throw "mysqld initialization failed with exit code $LASTEXITCODE" }} }}"#);
    vec!["powershell".into(), "-NoProfile".into(), "-NonInteractive".into(), "-ExecutionPolicy".into(), "Bypass".into(), "-Command".into(), script]
}

#[cfg(not(windows))]
pub(crate) fn mysql_fallback_command() -> Vec<String> { Vec::new() }
