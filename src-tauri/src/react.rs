use crate::llm::{LlmResult, Shell};
use async_trait::async_trait;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Initializes the React frontend included with an AdminPanel project.
pub struct ReactShell;

impl ReactShell {
    pub fn new() -> Self {
        Self
    }

    fn template_path() -> io::Result<PathBuf> {
        Ok(std::env::current_dir()?
            .join("..")
            .join("templates")
            .join("admin-panel"))
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

    async fn run_npm_install(cwd: &Path) -> LlmResult<()> {
        let cwd = cwd.to_path_buf();
        tauri::async_runtime::spawn_blocking(move || {
            run_command_blocking(&["install", "--force"], &cwd)
        })
        .await
        .map_err(|error| io::Error::other(format!("npm install task failed: {error}")))??;
        Ok(())
    }

    fn start_dev_server(cwd: &Path) -> io::Result<()> {
        println!("ReactShell: Starting dev server in {}", cwd.display());

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
        println!("ReactShell: Dev server started (PID {})", child.id());
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
    async fn init(&self, project_path: &str) -> LlmResult<()> {
        let template_path = Self::template_path()?;
        let target_path = Path::new(project_path).join("frontend");

        println!(
            "ReactShell: Copying template from {} to {}",
            template_path.display(),
            target_path.display()
        );
        Self::copy_template(&template_path, &target_path)?;
        println!("ReactShell: Template copied successfully");

        Self::run_npm_install(&target_path).await?;
        println!("ReactShell: npm install completed");

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
                println!(
                    "ReactShell: Skipping folder {}",
                    file_name.to_string_lossy()
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

fn run_command_blocking(args: &[&str], cwd: &Path) -> LlmResult<()> {
    println!(
        "ReactShell: Running command \"npm {}\" in {}",
        args.join(" "),
        cwd.display()
    );

    let mut command = npm_command(args);
    configure_environment(&mut command);
    let output = command.current_dir(cwd).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !stdout.is_empty() {
        println!("ReactShell: stdout: {stdout}");
    }
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "command failed with {}: npm {}\n{stderr}",
            output.status,
            args.join(" ")
        ))
        .into());
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
            println!("ReactShell: {} is on the user PATH", directory.display());
        }
        Ok(output) if output.status.code() == Some(3) => {
            println!("ReactShell: leaving the user PATH alone because adding {} would exceed setx's 1024 character limit", directory.display());
        }
        Ok(output) => {
            println!("ReactShell: could not add {} to the user PATH: {}", directory.display(), String::from_utf8_lossy(&output.stderr).trim());
        }
        Err(error) => println!("ReactShell: could not run setx: {error}"),
    }
}
