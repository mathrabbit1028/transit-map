use anyhow::{Context, Result};
use clap::Parser;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Route SVG generator
#[derive(Parser)]
#[command(name = "route-svg")]
struct Cli {
    /// Path to the route JSON file (e.g. data/route/5513_1.json)
    #[arg(short, long)]
    route: PathBuf,

    /// Directory containing position JSON files (e.g. data/position)
    #[arg(short, long)]
    position: PathBuf,

    /// Output SVG file path
    #[arg(short, long, default_value = "out.svg")]
    output: PathBuf,
}

// ── Data models ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct RouteStyle {
    color: String,
    #[allow(dead_code)]
    bidirectional: bool,
}

#[derive(Deserialize)]
struct Route {
    id: String,
    name: String,
    route: Vec<String>, // ids, may have trailing '*'
    style: RouteStyle,
}

#[derive(Deserialize)]
struct Stop {
    id: String,
    name: String,
    lat: f64,
    lng: f64,
}

// ── Projection helpers ────────────────────────────────────────────────────────

/// Map (lat, lng) to SVG canvas pixels.
/// lat  → y axis (inverted: higher lat = smaller y)
/// lng  → x axis
fn project(lat: f64, lng: f64, bounds: &Bounds, canvas: (f64, f64), pad: f64) -> (f64, f64) {
    let w = canvas.0 - 2.0 * pad;
    let h = canvas.1 - 2.0 * pad;

    let scale = f64::min(w / (bounds.max_lng - bounds.min_lng), h / (bounds.max_lat - bounds.min_lat));

    let x = (lng - bounds.min_lng) * scale + pad;
    // invert y so north is up
    let y = (h - (lat - bounds.min_lat) * scale) + pad;
    (x, y)
}

struct Bounds {
    min_lat: f64,
    max_lat: f64,
    min_lng: f64,
    max_lng: f64,
}

impl Bounds {
    fn from_stops(stops: &[&Stop]) -> Self {
        let min_lat = stops.iter().map(|s| s.lat).fold(f64::INFINITY, f64::min);
        let max_lat = stops.iter().map(|s| s.lat).fold(f64::NEG_INFINITY, f64::max);
        let min_lng = stops.iter().map(|s| s.lng).fold(f64::INFINITY, f64::min);
        let max_lng = stops.iter().map(|s| s.lng).fold(f64::NEG_INFINITY, f64::max);
        Self { min_lat, max_lat, min_lng, max_lng }
    }

    /// Expand bounds slightly so stops don't sit exactly on the edge.
    fn expand(&mut self, factor: f64) {
        let dlat = (self.max_lat - self.min_lat) * factor;
        let dlng = (self.max_lng - self.min_lng) * factor;
        self.min_lat -= dlat;
        self.max_lat += dlat;
        self.min_lng -= dlng;
        self.max_lng += dlng;
    }
}

// ── SVG helpers ───────────────────────────────────────────────────────────────

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn svg_polyline(points: &[(f64, f64)], color: &str, width: f64) -> String {
    let pts: Vec<String> = points.iter().map(|(x, y)| format!("{:.2},{:.2}", x, y)).collect();
    format!(
        r#"  <polyline points="{}" fill="none" stroke="{}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round"/>"#,
        pts.join(" "),
        color,
        width
    )
}

/// Draw a stop circle.
/// pass-through stops (*) are rendered as a smaller, semi-transparent circle.
fn svg_circle(x: f64, y: f64, passthrough: bool, color: &str) -> String {
    if passthrough {
        format!(
            r#"  <circle cx="{:.2}" cy="{:.2}" r="4" fill="{}" fill-opacity="0.4" stroke="{}" stroke-width="1.5" stroke-opacity="0.7"/>"#,
            x, y, color, color
        )
    } else {
        format!(
            r#"  <circle cx="{:.2}" cy="{:.2}" r="6" fill="white" stroke="{}" stroke-width="2.5"/>"#,
            x, y, color
        )
    }
}

/// Draw stop label. Labels for pass-through stops are lighter.
fn svg_label(x: f64, y: f64, name: &str, passthrough: bool, _color: &str) -> String {
    let opacity = if passthrough { "0.55" } else { "1.0" };
    let font_size = if passthrough { "10" } else { "11" };
    let font_weight = if passthrough { "normal" } else { "bold" };
    let dy = if passthrough { "-8" } else { "-10" };
    format!(
        r##"  <text x="{:.2}" y="{:.2}" dy="{}" text-anchor="middle" font-family="Noto Sans KR, sans-serif" font-size="{}" font-weight="{}" fill="#333333" fill-opacity="{}">{}</text>"##,
        x,
        y,
        dy,
        font_size,
        font_weight,
        opacity,
        escape_xml(name)
    )
}

// ── Main ──────────────────────────────────────────────────────────────────────
// cargo run -- --route ../data/route/{number}.json --position ../data/position --output ../out.svg

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load all position files into a flat map id → Stop
    let mut stop_map: HashMap<String, Stop> = HashMap::new();
    for entry in fs::read_dir(&cli.position)
        .with_context(|| format!("Cannot read position directory: {:?}", cli.position))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("Cannot read {:?}", path))?;
        let stops: Vec<Stop> = serde_json::from_str(&text)
            .with_context(|| format!("Cannot parse {:?}", path))?;
        for s in stops {
            stop_map.insert(s.id.clone(), s);
        }
    }

    // Load route file (array of routes)
    let route_text = fs::read_to_string(&cli.route)
        .with_context(|| format!("Cannot read route file: {:?}", cli.route))?;
    let routes: Vec<Route> = serde_json::from_str(&route_text)
        .with_context(|| format!("Cannot parse route file: {:?}", cli.route))?;

    // Canvas settings
    const WIDTH: f64 = 900.0;
    const HEIGHT: f64 = 900.0;
    const PAD: f64 = 80.0;

    // Collect all stops referenced by any route to compute bounds
    let all_stops: Vec<&Stop> = routes
        .iter()
        .flat_map(|r| r.route.iter())
        .filter_map(|raw_id| {
            let id = raw_id.trim_end_matches('*');
            stop_map.get(id)
        })
        .collect();

    if all_stops.is_empty() {
        anyhow::bail!("No matching stops found. Check that position directory and route IDs match.");
    }

    let mut bounds = Bounds::from_stops(&all_stops);
    bounds.expand(0.08);

    // Build SVG
    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">
  <style>text {{ font-family: 'Noto Sans KR', sans-serif; }}</style>
  <!-- background -->
  <rect width="{W}" height="{H}" fill="#f8f9fa"/>
"##,
        W = WIDTH as i32,
        H = HEIGHT as i32,
    ));

    // Per-route rendering: lines first, then circles, then labels
    for route in &routes {
        let color = &route.style.color;

        // Resolve stops in order
        struct ResolvedStop<'a> {
            stop: &'a Stop,
            passthrough: bool,
        }
        let resolved: Vec<ResolvedStop> = route
            .route
            .iter()
            .filter_map(|raw_id| {
                let passthrough = raw_id.ends_with('*');
                let id = raw_id.trim_end_matches('*');
                stop_map.get(id).map(|s| ResolvedStop { stop: s, passthrough })
            })
            .collect();

        if resolved.is_empty() {
            eprintln!("Warning: route '{}' has no resolvable stops", route.id);
            continue;
        }

        // Route label (title in top-left area)
        svg.push_str(&format!(
            r#"  <!-- Route: {} -->
"#,
            escape_xml(&route.name)
        ));

        // Line
        let pts: Vec<(f64, f64)> = resolved
            .iter()
            .map(|rs| project(rs.stop.lat, rs.stop.lng, &bounds, (WIDTH, HEIGHT), PAD))
            .collect();
        svg.push_str(&svg_polyline(&pts, color, 4.0));
        svg.push('\n');

        // Circles
        for (rs, &(x, y)) in resolved.iter().zip(pts.iter()) {
            svg.push_str(&svg_circle(x, y, rs.passthrough, color));
            svg.push('\n');
        }

        // Labels
        for (rs, &(x, y)) in resolved.iter().zip(pts.iter()) {
            svg.push_str(&svg_label(x, y, &rs.stop.name, rs.passthrough, color));
            svg.push('\n');
        }
    }

    // Legend
    let legend_x = PAD;
    let mut legend_y = HEIGHT - PAD + 30.0;
    svg.push_str(&format!(
        r##"  <!-- Legend -->
  <text x="{lx}" y="{ly}" font-size="12" font-weight="bold" fill="#333333">범례</text>
"##,
        lx = legend_x,
        ly = legend_y
    ));
    legend_y += 18.0;
    // Regular stop
    svg.push_str(&format!(
        r##"  <circle cx="{:.2}" cy="{:.2}" r="6" fill="white" stroke="#555555" stroke-width="2.5"/>
  <text x="{:.2}" y="{:.2}" dy="4" font-size="11" fill="#333333">일반 정류장</text>
"##,
        legend_x + 6.0,
        legend_y,
        legend_x + 16.0,
        legend_y
    ));
    legend_y += 18.0;
    // Pass-through stop
    svg.push_str(&format!(
        r##"  <circle cx="{:.2}" cy="{:.2}" r="4" fill="#555555" fill-opacity="0.4" stroke="#555555" stroke-width="1.5" stroke-opacity="0.7"/>
  <text x="{:.2}" y="{:.2}" dy="4" font-size="11" fill="#333333">통과 정류장 (*)</text>
"##,
        legend_x + 6.0,
        legend_y,
        legend_x + 16.0,
        legend_y
    ));

    // Route color swatches
    let mut swatch_x = legend_x + 160.0;
    let swatch_y = HEIGHT - PAD + 48.0;
    for route in &routes {
        svg.push_str(&format!(
            r##"  <line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{}" stroke-width="4" stroke-linecap="round"/>
  <text x="{:.2}" y="{:.2}" dy="4" font-size="11" fill="#333333">{}</text>
"##,
            swatch_x,
            swatch_y,
            swatch_x + 24.0,
            swatch_y,
            route.style.color,
            swatch_x + 28.0,
            swatch_y,
            escape_xml(&route.name)
        ));
        swatch_x += 160.0;
    }

    svg.push_str("</svg>\n");

    // Write output
    fs::write(&cli.output, &svg)
        .with_context(|| format!("Cannot write output file: {:?}", cli.output))?;

    println!("✓ SVG written to {:?}", cli.output);
    println!("  Routes : {}", routes.len());
    println!("  Stops  : {}", all_stops.len());

    Ok(())
}
