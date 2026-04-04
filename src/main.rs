use std::path::PathBuf;
use std::process;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use colored::Colorize;
use wait_timeout::ChildExt;

use testx::{config::Config, detection, output};

#[derive(ValueEnum, Clone, Default)]
enum OutputFormat {
    #[default]
    Pretty,
    Json,
    Junit,
    Tap,
}

#[derive(ValueEnum, Clone)]
enum ReporterKind {
    Github,
    Markdown,
    Html,
    Notify,
}

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

    /// Watch mode — re-run tests on file changes
    #[arg(short, long, global = true)]
    watch: bool,

    /// Retry failed tests N times before reporting failure
    #[arg(long, global = true)]
    retries: Option<u32>,

    /// Number of parallel jobs (0 = auto-detect CPUs)
    #[arg(short, long, global = true)]
    jobs: Option<usize>,

    /// Activate a reporter plugin (github, markdown, html, notify)
    #[arg(long, global = true)]
    reporter: Option<ReporterKind>,

    /// Extra arguments to pass through to the underlying test runner (after --)
    #[arg(last = true)]
    args: Vec<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run tests (default)
    Run {
        /// Filter tests by name pattern (supports glob: *foo*, test_*)
        #[arg(short, long)]
        filter: Option<String>,
        /// Exclude tests matching pattern
        #[arg(long)]
        exclude: Option<String>,
        /// Stop on first failure
        #[arg(long)]
        fail_fast: bool,
        /// Enable code coverage collection
        #[arg(long)]
        coverage: bool,
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
        /// Minimum pass rate threshold (0.0–1.0). Exit 1 if any flaky test is below this.
        #[arg(long)]
        threshold: Option<f64>,
        /// Number of parallel stress workers (0 = sequential, default)
        #[arg(long, default_value = "0")]
        parallel_stress: usize,
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
    /// Scan a workspace/monorepo and run tests across all detected projects
    Workspace {
        /// Maximum directory depth to scan (0 = unlimited)
        #[arg(long, default_value = "5")]
        max_depth: usize,
        /// Maximum parallel jobs (0 = auto-detect CPUs)
        #[arg(short, long, default_value = "0")]
        jobs: Option<usize>,
        /// Run projects sequentially instead of in parallel
        #[arg(long)]
        sequential: bool,
        /// Stop on first project failure
        #[arg(long)]
        fail_fast: bool,
        /// Filter to specific languages (comma-separated, e.g., "rust,python")
        #[arg(long)]
        filter: Option<String>,
        /// Only list discovered projects, don't run tests
        #[arg(long)]
        list: bool,
        /// Extra arguments to pass through to the underlying test runners
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Clear the smart test cache
    CacheClear,
    /// Show test history, trends, and flaky test analytics
    History {
        /// What to show: summary, runs, flaky, slow, health
        #[arg(value_enum, default_value = "summary")]
        view: HistoryView,
        /// Number of recent runs to analyze
        #[arg(short, long, default_value = "20")]
        last: usize,
    },
}

#[derive(ValueEnum, Clone, Default)]
enum HistoryView {
    #[default]
    Summary,
    Runs,
    Flaky,
    Slow,
    Health,
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

    match cli.command.unwrap_or(Commands::Run {
        args: vec![],
        filter: None,
        exclude: None,
        fail_fast: false,
        coverage: false,
    }) {
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
# See: https://github.com/whoisdinanath/testx

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

            let diff_mode = impact::DiffMode::parse(&mode).map_err(|e| anyhow::anyhow!("{}", e))?;

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

        Commands::Workspace {
            max_depth,
            jobs,
            sequential,
            fail_fast,
            filter,
            list,
            args: ws_args,
        } => {
            use testx::workspace::{self, WorkspaceConfig};

            let config = Config::load(&project_dir);

            let extra_args = if !ws_args.is_empty() {
                ws_args
            } else if !cli.args.is_empty() {
                cli.args
            } else {
                config.args.clone()
            };

            let filter_languages: Vec<String> = filter
                .map(|f| f.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default();

            let ws_config = WorkspaceConfig {
                max_depth,
                parallel: !sequential,
                max_jobs: jobs.unwrap_or(0),
                fail_fast,
                filter_languages,
                skip_dirs: Vec::new(),
            };

            println!(
                "{} {} scanning workspace at {}",
                "testx".bold().cyan(),
                "▸".bold(),
                project_dir.display()
            );
            println!();

            let projects = workspace::discover_projects(&project_dir, &engine, &ws_config);

            if projects.is_empty() {
                println!("  {} No testable projects found.", "⚠".yellow());
                println!();
                return Ok(());
            }

            if list {
                println!(
                    "  {} Discovered {} project(s):",
                    "✓".green(),
                    projects.len()
                );
                println!();
                for p in &projects {
                    let rel = p.path.strip_prefix(&project_dir).unwrap_or(&p.path);
                    println!(
                        "  {} {} ({}, {}, {:.0}% confidence)",
                        "▸".dimmed(),
                        rel.display(),
                        p.language,
                        p.framework,
                        p.confidence * 100.0,
                    );
                }
                println!();
                return Ok(());
            }

            println!(
                "  Running tests in {} project(s){}...",
                projects.len(),
                if ws_config.parallel {
                    format!(" ({} jobs)", ws_config.effective_jobs())
                } else {
                    " (sequential)".to_string()
                }
            );
            println!();

            let env_vars: Vec<(String, String)> = config
                .env
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            let report = workspace::run_workspace(
                &projects,
                &engine,
                &extra_args,
                &ws_config,
                &env_vars,
                cli.verbose,
            );

            match cli.output {
                OutputFormat::Json => {
                    let json = workspace::workspace_report_json(&report);
                    println!("{}", serde_json::to_string_pretty(&json).unwrap());
                }
                _ => {
                    println!("{}", workspace::format_workspace_report(&report));
                }
            }

            if report.projects_failed > 0 {
                process::exit(1);
            }
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

            println!("{} Cleared {} cache entries.", "✓".green().bold(), count);
            Ok(())
        }

        Commands::History { view, last } => {
            use testx::history::TestHistory;
            use testx::history::analytics::HealthScore;
            use testx::history::display;

            let history = TestHistory::open(&project_dir).map_err(|e| anyhow::anyhow!("{}", e))?;

            if history.run_count() == 0 {
                println!(
                    "{} {} {}",
                    "testx".bold().cyan(),
                    "history:".bold(),
                    "No test runs recorded yet. Run tests first!".dimmed()
                );
                return Ok(());
            }

            println!(
                "{} {} {} runs recorded",
                "testx".bold().cyan(),
                "history".bold(),
                history.run_count()
            );

            match view {
                HistoryView::Summary => {
                    print!("{}", display::format_stats_summary(&history));
                    let flaky = history.get_flaky_tests(3, 0.95);
                    if !flaky.is_empty() {
                        print!("{}", display::format_flaky_tests(&flaky));
                    }
                    let slow = history.get_slowest_trending(last, 3);
                    if !slow.is_empty() {
                        print!("{}", display::format_slow_tests(&slow));
                    }
                }
                HistoryView::Runs => {
                    print!("{}", display::format_recent_runs(&history, last));
                }
                HistoryView::Flaky => {
                    let flaky = history.get_flaky_tests(3, 0.95);
                    print!("{}", display::format_flaky_tests(&flaky));
                }
                HistoryView::Slow => {
                    let slow = history.get_slowest_trending(last, 3);
                    print!("{}", display::format_slow_tests(&slow));
                }
                HistoryView::Health => {
                    let score = HealthScore::compute(&history);
                    println!();
                    println!(
                        "  {} Test Health Score: {:.0}/100 ({})",
                        score.indicator(),
                        score.score,
                        score.grade()
                    );
                    println!("     Pass Rate:    {:.1}%", score.pass_rate);
                    println!("     Stability:    {:.1}%", score.stability);
                    println!("     Performance:  {:.1}%", score.performance);
                    println!();
                }
            }
            Ok(())
        }

        Commands::Stress {
            count,
            fail_fast,
            max_duration,
            threshold,
            parallel_stress,
            args: stress_args,
        } => {
            use testx::stress::{
                StressAccumulator, StressConfig, format_stress_report, stress_report_json,
            };

            let config = Config::load(&project_dir);

            let extra_args = if !stress_args.is_empty() {
                stress_args
            } else if !cli.args.is_empty() {
                cli.args
            } else {
                config.args.clone()
            };

            let detected = engine
                .detect(&project_dir)
                .context("No test framework detected. Try 'testx detect' to diagnose.")?;
            let adapter = engine.adapter(detected.adapter_index);

            if let Some(missing) = adapter.check_runner() {
                anyhow::bail!("Test runner '{}' not found.", missing);
            }

            let mut stress_cfg = StressConfig::new(count)
                .with_fail_fast(fail_fast)
                .with_parallel_workers(parallel_stress);
            if let Some(secs) = max_duration {
                stress_cfg = stress_cfg.with_max_duration(std::time::Duration::from_secs(secs));
            }
            if let Some(t) = threshold {
                stress_cfg = stress_cfg.with_threshold(t);
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
                let cmd_output = cmd.output().context("Failed to execute test command")?;
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

            if matches!(cli.output, OutputFormat::Json) {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&stress_report_json(&report)).unwrap()
                );
            } else {
                println!("{}", format_stress_report(&report));
            }

            if report.threshold_passed == Some(false) {
                process::exit(1);
            }
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

            let detected = engine
                .detect(&project_dir)
                .context("No test framework detected. Try 'testx detect' to diagnose.")?;
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

            let cmd_output = cmd.output().context("Failed to execute test command")?;

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

        Commands::Run {
            args: run_args,
            filter,
            exclude,
            fail_fast,
            coverage,
        } => {
            // Load config file
            let config = Config::load(&project_dir);

            // Merge args: CLI args take precedence, then config args
            let extra_args: Vec<String> = if !run_args.is_empty() {
                run_args
            } else if !cli.args.is_empty() {
                cli.args
            } else {
                config.args.clone()
            };

            // Resolve config-merged values (CLI takes precedence)
            let verbose = cli.verbose || config.output_config().verbose.unwrap_or(false);
            let slowest = cli.slowest.or(config.output_config().slowest);
            let timeout_secs = cli.timeout.or(config.timeout);
            let retries = cli.retries.or(config.retries).unwrap_or(0);
            let fail_fast = fail_fast || config.fail_fast.unwrap_or(false);

            // Resolve adapter override from config
            let adapter_override = config.adapter.clone();

            // Resolve filter: CLI --filter > config filter.include
            let filter_include = filter.or_else(|| {
                config
                    .filter_config()
                    .include
                    .as_ref()
                    .map(|s| s.to_string())
            });
            let filter_exclude = exclude.or_else(|| {
                config
                    .filter_config()
                    .exclude
                    .as_ref()
                    .map(|s| s.to_string())
            });

            // Resolve coverage: CLI --coverage > config coverage.enabled
            let coverage_enabled = coverage || config.coverage_config().enabled;

            // Resolve history: config history.enabled (default true)
            let history_enabled = config.history.as_ref().map(|h| h.enabled).unwrap_or(true);

            // --- Watch mode (--watch) ---
            if cli.watch || config.is_watch_enabled() {
                use testx::runner::RunnerConfig;
                use testx::watcher::{WatchRunner, WatchRunnerOptions};

                let detected = engine
                    .detect(&project_dir)
                    .context("No test framework detected. Try 'testx detect' to diagnose.")?;
                let adapter = engine.adapter(detected.adapter_index);

                if let Some(missing) = adapter.check_runner() {
                    anyhow::bail!("Test runner '{}' not found.", missing);
                }

                let mut runner_config = RunnerConfig::new(project_dir.clone());
                runner_config.merge_config(&config);
                runner_config.extra_args = extra_args;
                runner_config.verbose = verbose;

                let watch_config = config.watch_config();
                let mut options = WatchRunnerOptions::from_config(&watch_config);
                options.verbose = verbose;

                let mut watch_runner =
                    WatchRunner::new(project_dir.clone(), runner_config, options);

                watch_runner
                    .start(&watch_config)
                    .map_err(|e| anyhow::anyhow!("{}", e))?;

                return Ok(());
            }

            // --- Impact analysis (--affected) ---
            if let Some(ref affected) = cli.affected {
                use testx::impact;

                if !impact::is_git_repo(&project_dir) {
                    anyhow::bail!("--affected requires a git repository.");
                }

                let mode_str = affected.as_deref().unwrap_or("head");
                let diff_mode =
                    impact::DiffMode::parse(mode_str).map_err(|e| anyhow::anyhow!("{}", e))?;

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
                        } else if verbose {
                            eprintln!(
                                "{} Previous run failed — re-running tests.",
                                "cache:".dimmed()
                            );
                        }
                    }
                }
            }

            let detected = if let Some(ref override_name) = adapter_override {
                // Find adapter by name override from config
                let idx = engine
                    .adapters()
                    .iter()
                    .position(|a| a.name().to_lowercase() == override_name.to_lowercase())
                    .with_context(|| {
                        format!("Unknown adapter '{}' in testx.toml", override_name)
                    })?;
                let det = engine.adapter(idx).detect(&project_dir).with_context(|| {
                    format!(
                        "Adapter '{}' does not detect a project at {}",
                        override_name,
                        project_dir.display()
                    )
                })?;
                testx::detection::DetectedProject {
                    adapter_index: idx,
                    detection: det,
                }
            } else {
                engine.detect(&project_dir).context(
                    "No test framework detected. Try 'testx detect' to diagnose, or 'testx list' for supported frameworks.",
                )?
            };

            let adapter = engine.adapter(detected.adapter_index);

            // Set up event bus for lifecycle events
            let mut event_bus = testx::events::EventBus::new();

            // Fire RunStarted event
            event_bus.emit(testx::events::TestEvent::RunStarted {
                adapter: adapter.name().to_string(),
                framework: detected.detection.framework.clone(),
                project_dir: project_dir.clone(),
            });

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

            // --- Coverage: inject coverage arguments (--coverage) ---
            if coverage_enabled {
                let adapter_lower = adapter.name().to_lowercase();
                if let Some(cov_config) = testx::coverage::default_coverage_tool(&adapter_lower) {
                    for arg in &cov_config.extra_args {
                        cmd.arg(arg);
                    }
                    for (key, value) in &cov_config.env {
                        cmd.env(key, value);
                    }
                    if matches!(cli.output, OutputFormat::Pretty) {
                        println!(
                            "  {} Coverage enabled via {}",
                            "▸".dimmed(),
                            cov_config.tool
                        );
                    }
                } else if matches!(cli.output, OutputFormat::Pretty) {
                    eprintln!(
                        "  {} Coverage not supported for {}",
                        "⚠".yellow(),
                        adapter.name()
                    );
                }
            }

            // Set environment variables from config
            for (key, value) in &config.env {
                cmd.env(key, value);
            }

            if verbose {
                eprintln!("{} {:?}", "cmd:".dimmed(), cmd);
            }

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

            // Fire RunFinished event
            event_bus.emit(testx::events::TestEvent::RunFinished {
                result: result.clone(),
            });
            event_bus.flush();

            // --- Apply CI sharding (--partition) ---
            if let Some(ref partition_str) = cli.partition {
                use testx::sharding::ShardingMode;

                let mode =
                    ShardingMode::parse(partition_str).map_err(|e| anyhow::anyhow!("{}", e))?;

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

            // --- Apply test filter (--filter / config filter) ---
            if filter_include.is_some() || filter_exclude.is_some() {
                use testx::filter::TestFilter;

                let mut test_filter = TestFilter::new();
                if let Some(ref pattern) = filter_include {
                    test_filter = test_filter.include_csv(pattern);
                }
                if let Some(ref pattern) = filter_exclude {
                    test_filter = test_filter.exclude_csv(pattern);
                }

                if test_filter.is_active() {
                    let original_count = result.total_tests();
                    result = test_filter.apply(&result);

                    event_bus.emit(testx::events::TestEvent::FilterApplied {
                        pattern: filter_include
                            .clone()
                            .or(filter_exclude.clone())
                            .unwrap_or_default(),
                        matched_count: result.total_tests(),
                    });

                    if matches!(cli.output, OutputFormat::Pretty) {
                        println!(
                            "  {} Filter: {} of {} tests",
                            "▸".dimmed(),
                            result.total_tests(),
                            original_count
                        );
                    }
                }
            }

            // --- Retry failed tests (--retries) ---
            let mut retries_fixed = 0;
            if retries > 0 && result.total_failed() > 0 && !fail_fast {
                use testx::retry::{RetryConfig, merge_retry_result};

                let retry_cfg = RetryConfig::new(retries);

                for attempt in 1..=retry_cfg.max_retries {
                    if result.total_failed() == 0 {
                        break;
                    }

                    if matches!(cli.output, OutputFormat::Pretty) {
                        eprintln!(
                            "  {} Retry {}/{} — {} failed test(s)...",
                            "↻".yellow().bold(),
                            attempt,
                            retry_cfg.max_retries,
                            result.total_failed()
                        );
                    }

                    // Re-run the same command
                    let mut retry_cmd = adapter
                        .build_command(&project_dir, &extra_args)
                        .context("Failed to build retry command")?;
                    for (key, value) in &config.env {
                        retry_cmd.env(key, value);
                    }

                    let retry_output = retry_cmd
                        .output()
                        .context("Failed to execute retry command")?;

                    let retry_stdout = String::from_utf8_lossy(&retry_output.stdout).into_owned();
                    let retry_stderr = String::from_utf8_lossy(&retry_output.stderr).into_owned();
                    let retry_exit = retry_output.status.code().unwrap_or(1);

                    let retry_result =
                        adapter.parse_output(&retry_stdout, &retry_stderr, retry_exit);

                    let before_failed = result.total_failed();
                    result = merge_retry_result(&result, &retry_result);
                    let after_failed = result.total_failed();
                    retries_fixed += before_failed.saturating_sub(after_failed);

                    if retry_cfg.stop_on_pass && result.total_failed() == 0 {
                        break;
                    }
                }

                if retries_fixed > 0 && matches!(cli.output, OutputFormat::Pretty) {
                    println!(
                        "  {} {} test(s) fixed by retries",
                        "✓".green().bold(),
                        retries_fixed
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

            // --- Record history ---
            if history_enabled {
                use testx::history::TestHistory;

                if let Ok(mut history) = TestHistory::open(&project_dir) {
                    let _ = history.record(&result);
                }
            }

            // --- Reporter plugins (--reporter) ---
            if let Some(ref reporter) = cli.reporter {
                use testx::plugin::Plugin;

                match reporter {
                    ReporterKind::Github => {
                        use testx::plugin::reporters::github::{GithubConfig, GithubReporter};

                        let mut r = GithubReporter::new(GithubConfig::default());
                        let _ = r.on_result(&result);
                        for line in r.output() {
                            println!("{}", line);
                        }
                    }
                    ReporterKind::Markdown => {
                        use testx::plugin::reporters::markdown::{
                            MarkdownConfig, MarkdownReporter,
                        };

                        let mut r = MarkdownReporter::new(MarkdownConfig::default());
                        let _ = r.on_result(&result);
                        println!("{}", r.output());
                    }
                    ReporterKind::Html => {
                        use testx::plugin::reporters::html::{HtmlConfig, HtmlReporter};

                        let mut r = HtmlReporter::new(HtmlConfig::default());
                        let _ = r.on_result(&result);
                        let report_path = project_dir.join("testx-report.html");
                        if let Err(e) = std::fs::write(&report_path, r.output()) {
                            eprintln!("{} Failed to write HTML report: {}", "⚠".yellow(), e);
                        } else {
                            println!(
                                "  {} HTML report: {}",
                                "✓".green().bold(),
                                report_path.display()
                            );
                        }
                    }
                    ReporterKind::Notify => {
                        use testx::plugin::reporters::notify::{NotifyConfig, NotifyReporter};

                        let mut r = NotifyReporter::new(NotifyConfig::default());
                        let _ = r.on_result(&result);
                    }
                }
            }

            match cli.output {
                OutputFormat::Pretty => {
                    output::print_results(&result);

                    if let Some(n) = slowest {
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
