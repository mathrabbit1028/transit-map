mod optimizer;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use optimizer::{
    load_positions, load_routes, optimize_routes, render_svg, Canvas, OptimizedMap, OptimizerConfig,
};
use std::fs;
use std::path::PathBuf;

/// Octilinear route-map optimizer and SVG generator.
#[derive(Parser)]
#[command(name = "route-optimizer")]
struct Cli {
    /// Stage to run: all = optimize + write intermediate + SVG.
    #[arg(long, value_enum, default_value_t = Step::All)]
    step: Step,

    /// Path to the route JSON file, e.g. ../data/route/5513.json.
    #[arg(short, long)]
    route: Option<PathBuf>,

    /// Directory containing site-checker position JSON files.
    #[arg(short, long)]
    position: Option<PathBuf>,

    /// Intermediate optimizer JSON path.
    #[arg(short, long, default_value = "optimized-map.json")]
    intermediate: PathBuf,

    /// Output SVG file path.
    #[arg(short, long, default_value = "optimized-map.svg")]
    output: PathBuf,

    /// SVG canvas width.
    #[arg(long, default_value_t = 900.0)]
    width: f64,

    /// SVG canvas height.
    #[arg(long, default_value_t = 900.0)]
    height: f64,

    /// Padding used when projecting lat/lng into canvas coordinates.
    #[arg(long, default_value_t = 80.0)]
    padding: f64,

    /// Gradient-descent iterations for the optimization stage.
    #[arg(long, default_value_t = 900)]
    iterations: usize,

    /// Adam learning rate in SVG pixels per iteration.
    #[arg(long, default_value_t = 0.25)]
    learning_rate: f64,
}

#[derive(Clone, ValueEnum)]
enum Step {
    All,
    Optimize,
    Svg,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.step {
        Step::All => {
            let optimized = run_optimization(&cli)?;
            write_intermediate(&cli.intermediate, &optimized)?;
            write_svg(&cli.output, &optimized)?;
            print_summary(&optimized, Some(&cli.intermediate), Some(&cli.output));
        }
        Step::Optimize => {
            let optimized = run_optimization(&cli)?;
            write_intermediate(&cli.intermediate, &optimized)?;
            print_summary(&optimized, Some(&cli.intermediate), None);
        }
        Step::Svg => {
            let optimized = read_intermediate(&cli.intermediate)?;
            write_svg(&cli.output, &optimized)?;
            print_summary(&optimized, None, Some(&cli.output));
        }
    }

    Ok(())
}

fn run_optimization(cli: &Cli) -> Result<OptimizedMap> {
    let route_path = cli
        .route
        .as_ref()
        .context("--route is required for all/optimize steps")?;
    let position_dir = cli
        .position
        .as_ref()
        .context("--position is required for all/optimize steps")?;

    let routes = load_routes(route_path)?;
    let stops = load_positions(position_dir)?;

    let mut config = OptimizerConfig::default();
    config.canvas = Canvas {
        width: cli.width,
        height: cli.height,
        padding: cli.padding,
    };
    config.iterations = cli.iterations;
    config.learning_rate = cli.learning_rate;

    optimize_routes(routes, stops, config)
}

fn read_intermediate(path: &PathBuf) -> Result<OptimizedMap> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("Cannot read intermediate JSON: {:?}", path))?;
    serde_json::from_str(&text)
        .with_context(|| format!("Cannot parse intermediate JSON: {:?}", path))
}

fn write_intermediate(path: &PathBuf, optimized: &OptimizedMap) -> Result<()> {
    let text = serde_json::to_string_pretty(optimized)?;
    fs::write(path, format!("{text}\n"))
        .with_context(|| format!("Cannot write intermediate JSON: {:?}", path))
}

fn write_svg(path: &PathBuf, optimized: &OptimizedMap) -> Result<()> {
    fs::write(path, render_svg(optimized))
        .with_context(|| format!("Cannot write SVG output: {:?}", path))
}

fn print_summary(optimized: &OptimizedMap, intermediate: Option<&PathBuf>, svg: Option<&PathBuf>) {
    if let Some(path) = intermediate {
        println!("Intermediate JSON written to {:?}", path);
    }
    if let Some(path) = svg {
        println!("SVG written to {:?}", path);
    }

    println!("Routes: {}", optimized.routes.len());
    println!("Stops : {}", optimized.stops.len());
    println!(
        "Cost  : {:.2} -> {:.2}",
        optimized.optimization.initial_cost.total, optimized.optimization.final_cost.total
    );

    if !optimized.optimization.warnings.is_empty() {
        println!("Warnings:");
        for warning in &optimized.optimization.warnings {
            println!("  - {warning}");
        }
    }
}
