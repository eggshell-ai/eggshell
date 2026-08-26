use crate::llm::{LlmResult, Shell};
use crate::progress::{pump_output, ProgressLog};
use async_trait::async_trait;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The channel Symfony's output is logged under, which the window shows as its
/// backend tab.
const CHANNEL: &str = "symfony";

/// Initializes the Symfony backend included with an AdminPanel project.
pub struct SymfonyShell;

impl SymfonyShell {
    pub fn new() -> Self {
        Self
    }

    fn template_path(template_root: &Path) -> PathBuf {
        template_root.join("backend")
    }

    fn copy_template(template_path: &Path, target_path: &Path) -> io::Result<()> {
        if !template_path.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "Symfony template directory does not exist: {}",
                    template_path.display()
                ),
            ));
        }

        fs::create_dir_all(target_path)?;
        copy_directory(template_path, target_path, template_path)
    }

    async fn run_command(command: &str, cwd: &Path, log: &ProgressLog) -> LlmResult<()> {
        let command = command.to_string();
        let cwd = cwd.to_path_buf();
        // The blocking task outlives this borrow, so it takes a copy of the log.
        let log = log.clone();
        tauri::async_runtime::spawn_blocking(move || run_command_blocking(&command, &cwd, &log))
            .await
            .map_err(|error| io::Error::other(format!("command task failed: {error}")))??;
        Ok(())
    }

    async fn run_php_command(args: &[&str], cwd: &Path, log: &ProgressLog) -> LlmResult<()> {
        let args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        let cwd = cwd.to_path_buf();
        let log = log.clone();
        tauri::async_runtime::spawn_blocking(move || {
            run_process_blocking("php", &args, &cwd, &log)
        })
        .await
        .map_err(|error| io::Error::other(format!("command task failed: {error}")))??;
        Ok(())
    }

    fn start_server(cwd: &Path) -> io::Result<()> {
        println!("SymfonyShell: Starting Symfony server in {}", cwd.display());

        let mut command = if cfg!(windows) {
            let mut command = Command::new("cmd");
            command.args(["/C", "symfony", "server:start"]);
            command
        } else {
            let mut command = Command::new("symfony");
            command.args(["server:start"]);
            command
        };

        configure_environment(&mut command);
        command.current_dir(cwd);
        command.stdin(Stdio::null());

        // Keep stdout/stderr attached so the independently-created Windows
        // console remains useful for Symfony's server output.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x00000010 | 0x00000200); // CREATE_NEW_CONSOLE | CREATE_NEW_PROCESS_GROUP
        }

        let child = command.spawn()?;
        println!("SymfonyShell: Symfony server started (PID {})", child.id());
        drop(child);
        Ok(())
    }
}

impl Default for SymfonyShell {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Shell for SymfonyShell {
    async fn init(
        &self,
        project_dir: &str,
        template_root: &Path,
        log: &ProgressLog,
        mysql_password: &str,
    ) -> LlmResult<()> {
        let log = log.for_channel(CHANNEL);
        let template_path = Self::template_path(template_root);
        let target_path = Path::new(project_dir).join("backend");

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

        Self::run_command("composer install", &target_path, &log).await?;
        log.line("info", "Composer install completed");

        Self::run_command(
            "php bin/console lexik:jwt:generate-keypair --overwrite",
            &target_path,
            &log,
        )
        .await?;
        log.line("info", "JWT keypair generated");

        Self::run_php_command(
            &["bin/console", "app:init", "--", mysql_password],
            &target_path,
            &log,
        )
        .await?;
        log.line("info", "Database and user initialized");

        log.line("done", "Backend ready");
        Ok(())
    }

    async fn start(&self, project_path: &str) -> LlmResult<()> {
        let target_path = Path::new(project_path).join("backend");
        Self::start_server(&target_path)?;
        Ok(())
    }
}

fn copy_directory(source: &Path, destination: &Path, template_root: &Path) -> io::Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();

        if source_path.is_dir() {
            let relative_path = source_path
                .strip_prefix(template_root)
                .unwrap_or(&source_path);

            if [".git", "vendor", "var"].contains(&file_name.as_ref()) {
                println!("SymfonyShell: Skipping folder {file_name}");
                continue;
            }
            if relative_path == Path::new("config").join("jwt")
                || relative_path.starts_with(Path::new("config").join("jwt"))
            {
                println!("SymfonyShell: Skipping path {}", relative_path.display());
                continue;
            }

            fs::create_dir_all(&destination_path)?;
            copy_directory(&source_path, &destination_path, template_root)?;
        } else if file_name == "composer.lock" {
            println!("SymfonyShell: Skipping file {file_name}");
        } else {
            fs::copy(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

fn configure_environment(command: &mut Command) {
    command.env_clear();
    for name in [
        "PATH",
        "HOME",
        "APPDATA",
        "COMPOSER_HOME",
        "USERPROFILE",
        "SystemRoot",
    ] {
        if let Some(value) = std::env::var_os(name) {
            let value: OsString = value;
            command.env(name, value);
        }
    }

    if let Some(directory) = symfony_install_dir() {
        #[cfg(windows)]
        persist_on_user_path(&directory);
        prepend_to_path(command, &directory);
    }

    #[cfg(windows)]
    if crate::managed_composer_present() {
        if let Some(directory) = crate::managed_bin_dir() {
            prepend_to_path(command, &directory);
        }
    }

    #[cfg(windows)]
    if let Some(directory) = crate::setup::managed_php_dir() {
        prepend_to_path(command, &directory);
    }
}

/// Where setup put the Symfony CLI, when it is somewhere PATH does not already
/// cover: a directory Eggshell owns on Windows, and the prefix the get.symfony.com
/// installer uses everywhere else. `None` once the CLI is installed system-wide.
#[cfg(windows)]
fn symfony_install_dir() -> Option<PathBuf> {
    let directory = crate::managed_bin_dir()?;
    directory.join("symfony.exe").is_file().then_some(directory)
}

#[cfg(not(windows))]
fn symfony_install_dir() -> Option<PathBuf> {
    let directory = PathBuf::from(std::env::var_os("HOME")?)
        .join(".symfony5")
        .join("bin");
    directory.join("symfony").is_file().then_some(directory)
}

fn prepend_to_path(command: &mut Command, directory: &Path) {
    let mut value = directory.as_os_str().to_os_string();
    if let Some(existing) = std::env::var_os("PATH").filter(|existing| !existing.is_empty()) {
        value.push(if cfg!(windows) { ";" } else { ":" });
        value.push(existing);
    }
    command.env("PATH", value);
}

/// `setx` makes the Symfony CLI Eggshell installed visible to every process the
/// user starts from now on, including terminals opened outside the app.
///
/// It only ever appends to the *user* PATH, only when the directory is missing,
/// and only while the result still fits setx's 1024 character limit — anything
/// longer is silently truncated, which would take the user's other entries with
/// it. The unexpanded registry value is what gets rewritten, so entries written
/// as `%USERPROFILE%\...` stay that way.
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

    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            println!("SymfonyShell: {} is on the user PATH", directory.display());
        }
        Ok(output) if output.status.code() == Some(3) => {
            println!(
                "SymfonyShell: leaving the user PATH alone because adding {} would exceed setx's 1024 character limit",
                directory.display()
            );
        }
        Ok(output) => {
            println!(
                "SymfonyShell: could not add {} to the user PATH: {}",
                directory.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Err(error) => {
            println!("SymfonyShell: could not run setx: {error}");
        }
    }
}

/// Runs one command to completion, logging its output line by line as it arrives.
///
/// Piped rather than captured: `composer install` takes minutes and has nothing to
/// show for itself until it finishes, and the log window is where that wait is
/// explained.
fn run_command_blocking(command: &str, cwd: &Path, log: &ProgressLog) -> LlmResult<()> {
    log.line("command", format!("$ {command}"));
    println!("SymfonyShell: running \"{command}\" in {}", cwd.display());

    let mut process = if cfg!(windows) {
        let mut process = Command::new("cmd");
        process.args(["/C", command]);
        process
    } else {
        let mut process = Command::new("sh");
        process.args(["-c", command]);
        process
    };

    configure_environment(&mut process);
    process
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // The output is in the app now, so the console this would otherwise flash up
    // for every command is pure noise.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        process.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let mut child = process.spawn().inspect_err(|error| {
        log.line(
            "error",
            format!("`{command}` could not be started: {error}"),
        );
    })?;

    // One pipe is drained on a thread of its own: a command that fills stderr while
    // nobody reads it blocks forever waiting for room, and vice versa.
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
        // stderr says why when it says anything at all; some tools report their
        // failures on stdout instead.
        let details = if stderr_lines.is_empty() {
            stdout_lines.join("\n")
        } else {
            stderr_lines.join("\n")
        };
        let message = format!("command failed with {status}: {command}\n{details}");
        log.line("error", format!("`{command}` exited with {status}"));
        return Err(io::Error::other(message).into());
    }
    Ok(())
}

fn run_process_blocking(
    program: &str,
    args: &[String],
    cwd: &Path,
    log: &ProgressLog,
) -> LlmResult<()> {
    let display_command = format!("{} {}", program, args.join(" "));
    log.line("command", format!("$ {display_command}"));
    println!(
        "SymfonyShell: running \"{display_command}\" in {}",
        cwd.display()
    );

    let mut process = Command::new(program);
    process
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_environment(&mut process);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        process.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let mut child = process.spawn().inspect_err(|error| {
        log.line(
            "error",
            format!("`{display_command}` could not be started: {error}"),
        );
    })?;
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
        log.line("error", format!("`{display_command}` exited with {status}"));
        return Err(io::Error::other(format!(
            "command failed with {status}: {display_command}\n{details}"
        ))
        .into());
    }
    Ok(())
}
