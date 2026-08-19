use crate::llm::{LlmResult, Shell};
use async_trait::async_trait;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Initializes the Symfony backend included with an AdminPanel project.
pub struct SymfonyShell;

impl SymfonyShell {
    pub fn new() -> Self {
        Self
    }

    fn template_path() -> io::Result<PathBuf> {
        Ok(std::env::current_dir()?
            .join("..")
            .join("templates")
            .join("backend"))
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

    async fn run_command(command: &str, cwd: &Path) -> LlmResult<()> {
        let command = command.to_string();
        let cwd = cwd.to_path_buf();
        tauri::async_runtime::spawn_blocking(move || run_command_blocking(&command, &cwd))
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
    async fn init(&self, project_dir: &str) -> LlmResult<()> {
        let template_path = Self::template_path()?;
        let target_path = Path::new(project_dir).join("backend");

        println!(
            "SymfonyShell: Copying template from {} to {}",
            template_path.display(),
            target_path.display()
        );
        Self::copy_template(&template_path, &target_path)?;
        println!("SymfonyShell: Template copied successfully");

        Self::run_command("composer install", &target_path).await?;
        println!("SymfonyShell: Composer install completed");

        Self::run_command(
            "php bin/console lexik:jwt:generate-keypair --overwrite",
            &target_path,
        )
        .await?;
        println!("SymfonyShell: JWT keypair generated");

        Self::run_command("php bin/console app:init", &target_path).await?;
        println!("SymfonyShell: Database and user initialized");

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
}

fn run_command_blocking(command: &str, cwd: &Path) -> LlmResult<()> {
    println!(
        "SymfonyShell: Running command \"{command}\" in {}",
        cwd.display()
    );

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
    let output = process.current_dir(cwd).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !stdout.is_empty() {
        println!("SymfonyShell: stdout: {stdout}");
    }
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "command failed with {}: {command}\n{stderr}",
            output.status
        ))
        .into());
    }
    Ok(())
}
