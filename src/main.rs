use std::path::PathBuf;
use std::process;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
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

    /// Partition tests for CI sharding (e.g., slice:1/4 or hash:2/3)
    #[arg(long, global = true)]
    partition: Option<String>,

    /// Only run tests if source files changed (git-based impact analysis)
    #[arg(long, global = true)]
    affected: Option<Option<String>>,

    /// Use smart caching — skip re-running if nothing changed
    #[arg(long, global = true)]
    cache: bool,

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
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Stress test: run tests N times to detect flaky tests
    Stress {
        /// Number of iterations to run
        #[arg(short = 'n', long, default_value = "10")]
        count: usize,
        /// Stop on first failure
        #[arg(long)]
        fail_fast: bool,
        /// Maximum total duration in seconds
        #[arg(long)]
        max_duration: Option<u64>,
        /// Extra arguments to pass through to the underlying test runner
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Analyze which tests are affected by recent git changes
    Impact {
        /// Diff mode: head, staged, branch:<name>, commit:<sha>
        #[arg(short, long, default_value = "head")]
        mode: String,
    },
    /// Interactively pick tests to run using fuzzy search
    Pick {
        /// Extra arguments to pass through to the underlying test runner
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Clear the smart test cache
    CacheClear,
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
        Commands::Completions { shell } => {
            testx::completions::generate_completions(shell, &mut Cli::command());
            Ok(())
        }

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

        Commands::Impact { mode } => {
            use testx::impact;

            if !impact::is_git_repo(&project_dir) {
                anyhow::bail!("Not a git repository. Impact analysis requires git.");
            }

            let diff_mode = impact::DiffMode::parse(&mode)
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            let analysis = impact::analyze_impact(&project_dir, &diff_mode)
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            println!("{}", "testx".bold().cyan());
            println!();
            println!("{}", impact::format_impact(&analysis));

            if analysis.should_run_tests {
                println!();
                println!(
                    "  {} Tests should be run — {} relevant file(s) changed.",
                    "▸".bold(),
                    analysis.relevant_files.len()
                );
            } else {
                println!();
                println!(
                    "  {} No test-relevant changes — tests can be skipped.",
                    "✓".green().bold()
                );
            }
            println!();
            Ok(())
        }

        Commands::CacheClear => {
            use testx::cache::CacheStore;

            let mut store = CacheStore::load(&project_dir);
            let count = store.len();
            store.clear();
            store
                .save(&project_dir)
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            println!(
                "{} Cleared {} cache entries.",
                "✓".green().bold(),
                count
            );
            Ok(())
        }

        Commands::Stress {
            count,
            fail_fast,
            max_duration,
            args: stress_args,
        } => {
            use testx::stress::{StressConfig, StressAccumulator, format_stress_report};

            let config = Config::load(&project_dir);

            let extra_args = if !stress_args.is_empty() {
                stress_args
            } else if !cli.args.is_empty() {
                cli.args
            } else {
                config.args.clone()
            };

            let detected = engine.detect(&project_dir).context(
                "No test framework detected. Try 'testx detect' to diagnose.",
            )?;
            let adapter = engine.adapter(detected.adapter_index);

            if let Some(missing) = adapter.check_runner() {
                anyhow::bail!("Test runner '{}' not found.", missing);
            }

            let mut stress_cfg = StressConfig::new(count).with_fail_fast(fail_fast);
            if let Some(secs) = max_duration {
                stress_cfg = stress_cfg.with_max_duration(std::time::Duration::from_secs(secs));
            }

            println!(
                "{} {} stress test: {} iterations on {}",
                "testx".bold().cyan(),
                "▸".bold(),
                count,
                adapter.name().bold()
            );
            println!();

            let mut acc = StressAccumulator::new(stress_cfg);

            loop {
                let iteration = acc.completed() + 1;
                eprint!(
                    "  {} Iteration {}/{}...",
                    "▸".dimmed(),
                    iteration,
                    acc.requested()
                );

                let mut cmd = adapter
                    .build_command(&project_dir, &extra_args)
                    .context("Failed to build test command")?;

                for (key, value) in &config.env {
                    cmd.env(key, value);
                }

                let start = std::time::Instant::now();
                let cmd_output = cmd
                    .output()
                    .context("Failed to execute test command")?;
                let elapsed = start.elapsed();

                let stdout = String::from_utf8_lossy(&cmd_output.stdout).into_owned();
                let stderr = String::from_utf8_lossy(&cmd_output.stderr).into_owned();
                let exit_code = cmd_output.status.code().unwrap_or(1);

                let mut result = adapter.parse_output(&stdout, &stderr, exit_code);
                if result.duration.as_millis() == 0 {
                    result.duration = elapsed;
                }

                let passed = result.is_success();
                if passed {
                    eprintln!(
                        " {} ({:.1}ms)",
                        "PASS".green().bold(),
                        elapsed.as_secs_f64() * 1000.0
                    );
                } else {
                    eprintln!(
                        " {} ({:.1}ms, {} failed)",
                        "FAIL".red().bold(),
                        elapsed.as_secs_f64() * 1000.0,
                        result.total_failed()
                    );
                }

                if !acc.record(result, elapsed) {
                    break;
                }
            }

            let report = acc.report();
            println!();
            println!("{}", format_stress_report(&report));

            if !report.all_passed {
                process::exit(1);
            }
            Ok(())
        }

        Commands::Pick { args: pick_args } => {
            use testx::picker;

            let config = Config::load(&project_dir);

            let extra_args = if !pick_args.is_empty() {
                pick_args
            } else if !cli.args.is_empty() {
                cli.args.clone()
            } else {
                config.args.clone()
            };

            let detected = engine.detect(&project_dir).context(
                "No test framework detected. Try 'testx detect' to diagnose.",
            )?;
            let adapter = engine.adapter(detected.adapter_index);

            if let Some(missing) = adapter.check_runner() {
                anyhow::bail!("Test runner '{}' not found.", missing);
            }

            // First do a dry run to list available tests
            let mut cmd = adapter
                .build_command(&project_dir, &extra_args)
                .context("Failed to build test command")?;

            for (key, value) in &config.env {
                cmd.env(key, value);
            }

            let cmd_output = cmd
                .output()
                .context("Failed to execute test command")?;

            let stdout = String::from_utf8_lossy(&cmd_output.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&cmd_output.stderr).into_owned();
            let exit_code = cmd_output.status.code().unwrap_or(1);

            let result = adapter.parse_output(&stdout, &stderr, exit_code);

            let multi_suite = result.suites.len() > 1;
            let test_names: Vec<String> = result
                .suites
                .iter()
                .flat_map(|s| {
                    let suite_name = s.name.clone();
                    s.tests.iter().map(move |t| {
                        if multi_suite {
                            format!("{}::{}", suite_name, t.name)
                        } else {
                            t.name.clone()
                        }
                    })
                })
                .collect();

            if test_names.is_empty() {
                println!("No tests found to pick from.");
                return Ok(());
            }

            let prompt = format!(
                "{} {} — {} tests available",
                "testx pick".bold().cyan(),
                adapter.name().bold(),
                test_names.len()
            );

            let selected = picker::interactive_pick(&test_names, &prompt)
                .context("Interactive picker failed")?;

            if selected.is_empty() {
                println!("No tests selected.");
                return Ok(());
            }

            println!(
                "\n{} Running {} selected test(s)...\n",
                "▸".bold(),
                selected.len()
            );
            for name in &selected {
                println!("  {}", name);
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

            // --- Impact analysis (--affected) ---
            if let Some(ref affected) = cli.affected {
                use testx::impact;

                if !impact::is_git_repo(&project_dir) {
                    anyhow::bail!("--affected requires a git repository.");
                }

                let mode_str = affected.as_deref().unwrap_or("head");
                let diff_mode = impact::DiffMode::parse(mode_str)
                    .map_err(|e| anyhow::anyhow!("{}", e))?;

                let analysis = impact::analyze_impact(&project_dir, &diff_mode)
                    .map_err(|e| anyhow::anyhow!("{}", e))?;

                if cli.verbose {
                    eprintln!("{}", impact::format_impact(&analysis));
                }

                if !analysis.should_run_tests {
                    println!(
                        "{} {} {}",
                        "testx".bold().cyan(),
                        "✓".green().bold(),
                        "No test-relevant changes — skipping tests.".dimmed()
                    );
                    return Ok(());
                }

                if matches!(cli.output, OutputFormat::Pretty) {
                    println!(
                        "{} {} {} relevant file(s) changed — running tests.",
                        "testx".bold().cyan(),
                        "▸".bold(),
                        analysis.relevant_files.len()
                    );
                }
            }

            // --- Smart caching (--cache) ---
            if cli.cache {
                use testx::cache;

                let detected_for_cache = engine.detect(&project_dir);
                if let Some(ref det) = detected_for_cache {
                    let adapter_name = engine.adapter(det.adapter_index).name();
                    let cache_config = cache::CacheConfig::default();

                    if let Ok(hash) = cache::compute_project_hash(&project_dir, adapter_name)
                        && let Some(cached) = cache::check_cache(&project_dir, &hash, &cache_config)
                    {
                        if cached.passed {
                            println!(
                                "{} {} {}",
                                "testx".bold().cyan(),
                                "✓".green().bold(),
                                cache::format_cache_hit(&cached)
                            );
                            return Ok(());
                        } else if cli.verbose {
                            eprintln!(
                                "{} Previous run failed — re-running tests.",
                                "cache:".dimmed()
                            );
                        }
                    }
                }
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

            // --- Apply CI sharding (--partition) ---
            if let Some(ref partition_str) = cli.partition {
                use testx::sharding::ShardingMode;

                let mode = ShardingMode::parse(partition_str)
                    .map_err(|e| anyhow::anyhow!("{}", e))?;

                let original_count = result.total_tests();
                result = mode.apply(&result);

                if matches!(cli.output, OutputFormat::Pretty) {
                    println!(
                        "  {} Shard {}: {} of {} tests",
                        "▸".dimmed(),
                        mode.description(),
                        result.total_tests(),
                        original_count
                    );
                }
            }

            // --- Cache the result (--cache) ---
            if cli.cache {
                use testx::cache;

                let cache_config = cache::CacheConfig::default();
                if let Ok(hash) = cache::compute_project_hash(&project_dir, adapter.name()) {
                    let _ = cache::cache_result(
                        &project_dir,
                        &hash,
                        adapter.name(),
                        &result,
                        &extra_args,
                        &cache_config,
                    );
                }
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
