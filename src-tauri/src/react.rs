use crate::llm::{LlmResult, Shell};
use crate::progress::{pump_output, ProgressLog};
use async_trait::async_trait;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The channel the React output is logged under, which the window shows as its
/// frontend tab.
const CHANNEL: &str = "react";

/// Initializes the React frontend included with an AdminPanel project.
pub struct ReactShell;

impl ReactShell {
    pub fn new() -> Self {
        Self
    }

    fn template_path(template_root: &Path) -> PathBuf {
        template_root.join("admin-panel")
    }

    fn copy_template(template_path: &Path, target_path: &Path) -> io::Result<()> {
        if !template_path.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "React template directory does not exist: {}",
                    template_path.display()
                ),
            ));
        }

        fs::create_dir_all(target_path)?;
        copy_directory(template_path, target_path)
    }

    async fn run_npm_install(cwd: &Path, log: &ProgressLog) -> LlmResult<()> {
        let cwd = cwd.to_path_buf();
        // The blocking task outlives this borrow, so it takes a copy of the log.
        let log = log.clone();
        tauri::async_runtime::spawn_blocking(move || {
            run_command_blocking(&["install", "--force"], &cwd, &log)
        })
        .await
        .map_err(|error| io::Error::other(format!("npm install task failed: {error}")))??;
        Ok(())
    }

    fn start_dev_server(cwd: &Path) -> io::Result<()> {
        crate::logger::Logger::global().info(
            format!("ReactShell: Starting dev server in {}", cwd.display()),
            false,
        );

        let mut command = npm_command(&["run", "dev"]);
        configure_environment(&mut command);
        command
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NEW_CONSOLE | CREATE_NEW_PROCESS_GROUP: keep the Vite/
            // React output visible and independent from the Tauri process.
            command.creation_flags(0x00000010 | 0x00000200);
        }

        let child = command.spawn()?;
        crate::logger::Logger::global().info(
            format!("ReactShell: Dev server started (PID {})", child.id()),
            false,
        );
        drop(child);
        Ok(())
    }
}

impl Default for ReactShell {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Shell for ReactShell {
    async fn init(
        &self,
        project_path: &str,
        _slug: &str,
        template_root: &Path,
        log: &ProgressLog,
        _mysql_password: &str,
    ) -> LlmResult<()> {
        let log = log.for_channel(CHANNEL);
        let template_path = Self::template_path(template_root);
        let target_path = Path::new(project_path).join("frontend");

        log.line(
            "info",
            format!(
                "Copying template from {} to {}",
                template_path.display(),
                target_path.display()
            ),
        );
        Self::copy_template(&template_path, &target_path).inspect_err(|error| {
            log.line("error", format!("Could not copy the template: {error}"))
        })?;
        log.line("info", "Template copied successfully");

        Self::run_npm_install(&target_path, &log).await?;
        log.line("info", "npm install completed");

        log.line("done", "Frontend ready");
        Ok(())
    }

    async fn start(&self, project_path: &str) -> LlmResult<()> {
        let target_path = Path::new(project_path).join("frontend");
        Self::start_dev_server(&target_path)?;
        Ok(())
    }
}

fn copy_directory(source: &Path, destination: &Path) -> io::Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_name = entry.file_name();

        if source_path.is_dir() {
            if [".git", "node_modules"].contains(&file_name.to_string_lossy().as_ref()) {
                crate::logger::Logger::global().info(
                    format!("ReactShell: Skipping folder {}", file_name.to_string_lossy()),
                    false,
                );
                continue;
            }

            fs::create_dir_all(&destination_path)?;
            copy_directory(&source_path, &destination_path)?;
        } else {
            fs::copy(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

fn npm_command(args: &[&str]) -> Command {
    let mut command = if cfg!(windows) {
        let mut command = Command::new("cmd");
        command.arg("/C").arg("npm");
        command
    } else {
        Command::new("npm")
    };
    command.args(args);
    command
}

/// Runs one npm command to completion, logging its output line by line as it
/// arrives.
///
/// Piped rather than captured: `npm install --force` takes minutes and has nothing
/// to show for itself until it finishes, and the log window is where that wait is
/// explained.
fn run_command_blocking(args: &[&str], cwd: &Path, log: &ProgressLog) -> LlmResult<()> {
    let label = format!("npm {}", args.join(" "));
    log.line("command", format!("$ {label}"));

    let mut command = npm_command(args);
    configure_environment(&mut command);
    command
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // The output is in the app now, so the console this would otherwise flash up
    // for every command is pure noise.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let mut child = command.spawn().inspect_err(|error| {
        log.line("error", format!("`{label}` could not be started: {error}"));
    })?;

    // One pipe is drained on a thread of its own: npm writes its whole progress
    // display to stderr, and a command nobody reads blocks forever waiting for room.
    let stderr = child.stderr.take();
    let stderr_log = log.clone();
    let reader = std::thread::spawn(move || {
        stderr
            .map(|pipe| pump_output(pipe, "stderr", &stderr_log))
            .unwrap_or_default()
    });
    let stdout_lines = child
        .stdout
        .take()
        .map(|pipe| pump_output(pipe, "stdout", log))
        .unwrap_or_default();
    let stderr_lines = reader.join().unwrap_or_default();

    let status = child.wait()?;
    if !status.success() {
        let details = if stderr_lines.is_empty() {
            stdout_lines.join("\n")
        } else {
            stderr_lines.join("\n")
        };
        let message = format!("command failed with {status}: {label}\n{details}");
        log.line("error", format!("`{label}` exited with {status}"));
        return Err(io::Error::other(message).into());
    }
    Ok(())
}

fn configure_environment(command: &mut Command) {
    command.env_clear();
    for name in [
        "PATH",
        "HOME",
        "APPDATA",
        "USERPROFILE",
        "SystemRoot",
        "ComSpec",
        "PATHEXT",
        "TEMP",
        "TMP",
        "LOCALAPPDATA",
    ] {
        if let Some(value) = std::env::var_os(name) {
            let value: OsString = value;
            command.env(name, value);
        }
    }

    #[cfg(windows)]
    if let Some(directory) = crate::setup::managed_node_dir() {
        if directory.join("node.exe").is_file() {
            persist_on_user_path(&directory);
            prepend_to_path(command, &directory);
        }
    }
}

fn prepend_to_path(command: &mut Command, directory: &Path) {
    let mut value = directory.as_os_str().to_os_string();
    if let Some(existing) = std::env::var_os("PATH").filter(|existing| !existing.is_empty()) {
        value.push(if cfg!(windows) { ";" } else { ":" });
        value.push(existing);
    }
    command.env("PATH", value);
}

/// Make Eggshell's portable Node available to processes the user starts after
/// this one. The current command also receives the directory directly above.
#[cfg(windows)]
fn persist_on_user_path(directory: &Path) {
    static PERSISTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if PERSISTED.set(()).is_err() {
        return;
    }

    let quoted_directory = directory.display().to_string().replace('\'', "''");
    let script = format!(
        "$directory = '{quoted_directory}'; \
         $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment'); \
         $current = if ($key) {{ [string]$key.GetValue('Path', '', 'DoNotExpandEnvironmentNames') }} else {{ '' }}; \
         if ($current -split ';' | Where-Object {{ $_.Trim().TrimEnd('\\') -ieq $directory.TrimEnd('\\') }}) {{ exit 0 }}; \
         $updated = if ($current) {{ \"$current;$directory\" }} else {{ $directory }}; \
         if ($updated.Length -gt 1024) {{ exit 3 }}; \
         setx PATH $updated | Out-Null; \
         exit $LASTEXITCODE"
    );

    match Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
    {
        Ok(output) if output.status.success() => {
            crate::logger::Logger::global().info(
                format!("ReactShell: {} is on the user PATH", directory.display()),
                false,
            );
        }
        Ok(output) if output.status.code() == Some(3) => {
            crate::logger::Logger::global().warning(
                format!(
                    "ReactShell: leaving the user PATH alone because adding {} would exceed setx's 1024 character limit",
                    directory.display()
                ),
                false,
            );
        }
        Ok(output) => {
            crate::logger::Logger::global().error(
                format!(
                    "ReactShell: could not add {} to the user PATH: {}",
                    directory.display(),
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
                false,
            );
        }
        Err(error) => crate::logger::Logger::global().error(
            format!("ReactShell: could not run setx: {error}"),
            false,
        ),
    }
}
