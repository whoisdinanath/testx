use std::path::PathBuf;
use std::process;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use colored::Colorize;
use wait_timeout::ChildExt;

use testx::{config::Config, detection, output};

#[derive(Parser)]
#[command(
    name = "testx",
    about = "Universal test runner — one command to test any project",
    version,
    author
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to the project directory (defaults to current directory)
    #[arg(short, long, global = true)]
    path: Option<PathBuf>,

    /// Output format
    #[arg(short, long, global = true, default_value = "pretty")]
    output: OutputFormat,

    /// Show N slowest tests
    #[arg(long, global = true)]
    slowest: Option<usize>,

    /// Show raw output from the underlying test runner
    #[arg(long, global = true)]
    raw: bool,

    /// Show verbose output (detection details, commands)
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Timeout in seconds — kill test process after N seconds
    #[arg(short, long, global = true)]
    timeout: Option<u64>,

    /// Extra arguments to pass through to the underlying test runner (after --)
    #[arg(last = true)]
    args: Vec<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run tests (default)
    Run {
        /// Extra arguments to pass through to the underlying test runner
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Detect the test framework without running tests
    Detect,
    /// List all supported adapters
    List,
    /// Generate a testx.toml config file for this project
    Init,
}

#[derive(ValueEnum, Clone, Default)]
enum OutputFormat {
    #[default]
    Pretty,
    Json,
    Junit,
    Tap,
}

fn main() {
    // Respect NO_COLOR, CI, and TERM=dumb for disabling colors
    if std::env::var_os("NO_COLOR").is_some()
        || std::env::var("CI").is_ok()
        || std::env::var("TERM").as_deref() == Ok("dumb")
    {
        colored::control::set_override(false);
    }

    let cli = Cli::parse();

    if let Err(e) = run(cli) {
        eprintln!("{} {}", "error:".red().bold(), e);
        process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    let project_dir = cli
        .path
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let project_dir = project_dir
        .canonicalize()
        .context("Failed to resolve project directory")?;

    let engine = detection::DetectionEngine::new();

    match cli.command.unwrap_or(Commands::Run { args: vec![] }) {
        Commands::List => {
            println!("{}", "Supported test frameworks:".bold());
            println!();
            for adapter in engine.adapters() {
                println!("  {} {}", "▸".bold(), adapter.name());
            }
            println!();
            Ok(())
        }

        Commands::Init => {
            let config_path = project_dir.join("testx.toml");
            if config_path.exists() {
                anyhow::bail!("testx.toml already exists at {}", config_path.display());
            }

            let detected = engine.detect_all(&project_dir);
            let adapter_name = detected
                .first()
                .map(|d| engine.adapter(d.adapter_index).name().to_lowercase())
                .unwrap_or_else(|| "auto".into());

            let content = format!(
                r#"# testx configuration
# See: https://github.com/bibekblockchain/testx

# Override adapter selection (auto-detected: "{adapter}")
# adapter = "{adapter}"

# Extra arguments to pass to the test runner
args = []

# Timeout in seconds (0 = no timeout)
# timeout = 60

# Environment variables
# [env]
# CI = "true"
"#,
                adapter = adapter_name,
            );

            std::fs::write(&config_path, content).context("Failed to write testx.toml")?;
            println!("{} Created {}", "✓".green().bold(), config_path.display());
            Ok(())
        }

        Commands::Detect => {
            println!(
                "{} {}",
                "testx".bold().cyan(),
                format!("scanning {}", project_dir.display()).dimmed(),
            );
            println!();

            let detected = engine.detect_all(&project_dir);
            if detected.is_empty() {
                println!("  {} No test framework detected.", "⚠".yellow());
                println!();
                return Ok(());
            }

            println!("  {} Detected frameworks:", "✓".green());
            for d in &detected {
                output::print_detection(d);
            }
            println!();
            Ok(())
        }

        Commands::Run { args: run_args } => {
            // Load config file
            let config = Config::load(&project_dir);

            // Merge args: CLI args take precedence, then config args
            let mut extra_args: Vec<String> = if !run_args.is_empty() {
                run_args
            } else if !cli.args.is_empty() {
                cli.args
            } else {
                config.args.clone()
            };

            // If CLI provided args AND config has args, append config args
            if extra_args.is_empty() {
                extra_args = config.args.clone();
            }

            let detected = engine.detect(&project_dir)
                .context("No test framework detected. Try 'testx detect' to diagnose, or 'testx list' for supported frameworks.")?;

            let adapter = engine.adapter(detected.adapter_index);

            if matches!(cli.output, OutputFormat::Pretty) {
                output::print_header(adapter.name(), &detected);
            }

            // Pre-flight: check if test runner is available
            if let Some(missing) = adapter.check_runner() {
                anyhow::bail!(
                    "Test runner '{}' not found. Install it and try again.",
                    missing
                );
            }

            let mut cmd = adapter
                .build_command(&project_dir, &extra_args)
                .context("Failed to build test command")?;

            // Set environment variables from config
            for (key, value) in &config.env {
                cmd.env(key, value);
            }

            if cli.verbose {
                eprintln!("{} {:?}", "cmd:".dimmed(), cmd);
            }

            // Resolve timeout: CLI flag > config > none
            let timeout_secs = cli.timeout.or(config.timeout);

            let start = std::time::Instant::now();

            let (stdout, stderr, exit_code) = if let Some(secs) = timeout_secs {
                // Spawn with timeout
                let mut child = cmd
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .context("Failed to execute test command. Is the test runner installed?")?;

                let timeout_dur = std::time::Duration::from_secs(secs);
                match child.wait_timeout(timeout_dur) {
                    Ok(Some(status)) => {
                        let mut stdout_buf = Vec::new();
                        let mut stderr_buf = Vec::new();
                        if let Some(mut out) = child.stdout.take() {
                            std::io::Read::read_to_end(&mut out, &mut stdout_buf).ok();
                        }
                        if let Some(mut err) = child.stderr.take() {
                            std::io::Read::read_to_end(&mut err, &mut stderr_buf).ok();
                        }
                        (
                            String::from_utf8_lossy(&stdout_buf).into_owned(),
                            String::from_utf8_lossy(&stderr_buf).into_owned(),
                            status.code().unwrap_or(1),
                        )
                    }
                    Ok(None) => {
                        // Timeout — kill the process
                        child.kill().ok();
                        child.wait().ok();
                        eprintln!("{} Test timed out after {}s", "✗".red().bold(), secs,);
                        (String::new(), format!("Timed out after {}s", secs), 124)
                    }
                    Err(e) => {
                        anyhow::bail!("Failed waiting for test process: {e}");
                    }
                }
            } else {
                let output = cmd
                    .output()
                    .context("Failed to execute test command. Is the test runner installed?")?;
                (
                    String::from_utf8_lossy(&output.stdout).into_owned(),
                    String::from_utf8_lossy(&output.stderr).into_owned(),
                    output.status.code().unwrap_or(1),
                )
            };

            let elapsed = start.elapsed();

            let mut result = adapter.parse_output(&stdout, &stderr, exit_code);

            // Use wall-clock time if parser didn't capture duration
            if result.duration.as_millis() == 0 {
                result.duration = elapsed;
            }

            match cli.output {
                OutputFormat::Pretty => {
                    output::print_results(&result);

                    if let Some(n) = cli.slowest {
                        output::print_slowest_tests(&result, n);
                    }

                    // Show raw output on failure or when --raw is passed
                    if cli.raw || !result.is_success() {
                        output::print_raw_output(&stdout, &stderr);
                    }
                }
                OutputFormat::Json => {
                    output::print_json(&result);
                }
                OutputFormat::Junit => {
                    output::print_junit_xml(&result);
                }
                OutputFormat::Tap => {
                    output::print_tap(&result);
                }
            }

            if !result.is_success() {
                process::exit(1);
            }

            Ok(())
        }
    }
}
