use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::Path;

const SCHEMA: &str = "transit-map.route-optimizer.v1";
const DIR_45: f64 = 0.7071067811865476;
const OCTILINEAR_DIRECTIONS: [Direction; 8] = [
    Direction::new(1.0, 0.0),
    Direction::new(-1.0, 0.0),
    Direction::new(0.0, 1.0),
    Direction::new(0.0, -1.0),
    Direction::new(DIR_45, DIR_45),
    Direction::new(-DIR_45, -DIR_45),
    Direction::new(DIR_45, -DIR_45),
    Direction::new(-DIR_45, DIR_45),
];
const OVERLAP_SAMPLES: [f64; 3] = [0.25, 0.5, 0.75];

#[derive(Clone, Copy)]
struct Direction {
    x: f64,
    y: f64,
}

impl Direction {
    const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum TransferAxis {
    Horizontal,
    Vertical,
    DiagonalDown,
    DiagonalUp,
}

impl TransferAxis {
    const ALL: [Self; 4] = [
        Self::Horizontal,
        Self::Vertical,
        Self::DiagonalDown,
        Self::DiagonalUp,
    ];

    fn direction(self) -> Direction {
        match self {
            Self::Horizontal => Direction::new(1.0, 0.0),
            Self::Vertical => Direction::new(0.0, 1.0),
            Self::DiagonalDown => Direction::new(DIR_45, DIR_45),
            Self::DiagonalUp => Direction::new(DIR_45, -DIR_45),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Canvas {
    pub width: f64,
    pub height: f64,
    pub padding: f64,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            width: 900.0,
            height: 900.0,
            padding: 80.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Bounds {
    pub min_lat: f64,
    pub max_lat: f64,
    pub min_lng: f64,
    pub max_lng: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteStyle {
    pub color: String,
    pub bidirectional: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub id: String,
    pub name: String,
    pub route: Vec<String>,
    pub style: RouteStyle,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Stop {
    pub id: String,
    pub name: String,
    pub lat: f64,
    pub lng: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizedMap {
    pub schema: String,
    pub canvas: Canvas,
    pub bounds: Bounds,
    pub routes: Vec<OptimizedRoute>,
    pub stops: Vec<OptimizedStop>,
    pub optimization: OptimizationReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizedRoute {
    pub id: String,
    pub name: String,
    pub style: RouteStyle,
    pub stops: Vec<OptimizedRouteStop>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizedRouteStop {
    pub id: String,
    pub name: String,
    pub passthrough: bool,
    pub point: Point,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizedStop {
    pub id: String,
    pub name: String,
    pub lat: f64,
    pub lng: f64,
    pub projected: Point,
    pub optimized: Point,
    pub visits: Vec<OptimizedStopVisit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizedStopVisit {
    pub route_id: String,
    pub route_name: String,
    pub passthrough: bool,
    pub point: Point,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationReport {
    pub iterations: usize,
    pub learning_rate: f64,
    pub gradient_clip: f64,
    pub weights: CostWeights,
    pub parameters: CostParameters,
    pub initial_cost: CostBreakdown,
    pub final_cost: CostBreakdown,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OptimizerConfig {
    pub canvas: Canvas,
    pub iterations: usize,
    pub learning_rate: f64,
    pub gradient_clip: f64,
    pub weights: CostWeights,
    pub parameters: CostParameters,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            canvas: Canvas::default(),
            iterations: 900,
            learning_rate: 0.22,
            gradient_clip: 500.0,
            weights: CostWeights::default(),
            parameters: CostParameters::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct CostWeights {
    pub anchor: f64,
    pub octilinear: f64,
    pub direction_preference: f64,
    pub bend: f64,
    pub bend_angle_preference: f64,
    pub overlap: f64,
    pub self_crossing: f64,
    pub shared_corridor_bundle: f64,
    pub shared_stop_compactness: f64,
    pub shared_stop_lane_gap: f64,
    pub transfer_alignment: f64,
    pub shared_segment_order: f64,
    pub stop_spacing: f64,
    pub station_line_clearance: f64,
    pub label_spacing: f64,
    pub segment_length: f64,
    pub bounds: f64,
}

impl Default for CostWeights {
    fn default() -> Self {
        Self {
            anchor: 0.18,
            octilinear: 0.12,
            direction_preference: 0.015,
            bend: 0.06,
            bend_angle_preference: 0.0012,
            overlap: 0.018,
            self_crossing: 1.60,
            shared_corridor_bundle: 1.15,
            shared_stop_compactness: 1.30,
            shared_stop_lane_gap: 0.25,
            transfer_alignment: 12.00,
            shared_segment_order: 1.20,
            stop_spacing: 0.010,
            station_line_clearance: 0.016,
            label_spacing: 0.018,
            segment_length: 0.010,
            bounds: 3.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct CostParameters {
    pub bounds_expand_factor: f64,
    pub overlap_clearance: f64,
    pub stop_clearance: f64,
    pub station_line_clearance: f64,
    pub self_crossing_clearance: f64,
    pub label_clearance: f64,
    pub label_char_width: f64,
    pub label_height: f64,
    pub label_offset_y: f64,
    pub shared_lane_gap: f64,
    pub transfer_order_search_limit: usize,
    pub transfer_order_update_interval: usize,
    pub transfer_alignment_hardness: f64,
    pub shared_corridor_gap: f64,
    pub min_segment_length: f64,
    pub min_bend_segment_length: f64,
    pub bend_angle_preference_margin: f64,
    pub initial_shared_offset: f64,
}

impl Default for CostParameters {
    fn default() -> Self {
        Self {
            bounds_expand_factor: 0.08,
            overlap_clearance: 22.0,
            stop_clearance: 30.0,
            station_line_clearance: 18.0,
            self_crossing_clearance: 34.0,
            label_clearance: 6.0,
            label_char_width: 7.0,
            label_height: 14.0,
            label_offset_y: -18.0,
            shared_lane_gap: 7.0,
            transfer_order_search_limit: 8,
            transfer_order_update_interval: 40,
            transfer_alignment_hardness: 1.6,
            shared_corridor_gap: 7.0,
            min_segment_length: 28.0,
            min_bend_segment_length: 44.0,
            bend_angle_preference_margin: 260.0,
            initial_shared_offset: 5.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CostBreakdown {
    pub total: f64,
    pub anchor: f64,
    pub octilinear: f64,
    pub direction_preference: f64,
    pub bend: f64,
    pub bend_angle_preference: f64,
    pub overlap: f64,
    pub self_crossing: f64,
    pub shared_corridor_bundle: f64,
    pub shared_stop_compactness: f64,
    pub shared_stop_lane_gap: f64,
    pub transfer_alignment: f64,
    pub shared_segment_order: f64,
    pub stop_spacing: f64,
    pub station_line_clearance: f64,
    pub label_spacing: f64,
    pub segment_length: f64,
    pub bounds: f64,
}

struct Problem {
    canvas: Canvas,
    bounds: Bounds,
    routes: Vec<RouteSpec>,
    visits: Vec<VisitSpec>,
    segments: Vec<SegmentSpec>,
    stop_groups: Vec<StopGroup>,
    corridor_groups: Vec<CorridorGroup>,
    initial_coords: Vec<f64>,
    config: OptimizerConfig,
    warnings: Vec<String>,
}

#[derive(Clone)]
struct RouteSpec {
    id: String,
    name: String,
    style: RouteStyle,
    visit_indices: Vec<usize>,
}

#[derive(Clone)]
struct VisitSpec {
    route_index: usize,
    route_position: usize,
    stop_id: String,
    stop_name: String,
    lat: f64,
    lng: f64,
    passthrough: bool,
    projected: Point,
}

#[derive(Clone)]
struct SegmentSpec {
    route_index: usize,
    segment_index: usize,
    a_visit: usize,
    b_visit: usize,
}

#[derive(Clone)]
struct StopGroup {
    stop_id: String,
    stop_name: String,
    lat: f64,
    lng: f64,
    projected: Point,
    visits: Vec<usize>,
}

#[derive(Clone)]
struct CorridorGroup {
    members: Vec<CorridorMember>,
}

#[derive(Clone)]
struct CorridorMember {
    segment_index: usize,
    key_forward: bool,
    lane_offset: f64,
}

#[derive(Clone, Copy)]
struct AdPoint {
    x: Var,
    y: Var,
}

#[derive(Clone, Copy)]
struct LabelBox {
    center: AdPoint,
    half_width: f64,
    half_height: f64,
}

#[derive(Clone)]
struct PlacedLabel {
    name: String,
    center: Point,
    passthrough_only: bool,
    bbox: RenderLabelBox,
}

#[derive(Clone, Copy)]
struct RenderLabelBox {
    center: Point,
    half_width: f64,
    half_height: f64,
}

#[derive(Clone, Copy)]
struct RenderCircle {
    center: Point,
    radius: f64,
}

#[derive(Clone, Copy)]
struct TransferArm {
    route_index: usize,
    visit_index: usize,
    from: Point,
    to: Point,
}

#[derive(Clone, Copy)]
struct CostVars {
    anchor: Var,
    octilinear: Var,
    direction_preference: Var,
    bend: Var,
    bend_angle_preference: Var,
    overlap: Var,
    self_crossing: Var,
    shared_corridor_bundle: Var,
    shared_stop_compactness: Var,
    shared_stop_lane_gap: Var,
    transfer_alignment: Var,
    shared_segment_order: Var,
    stop_spacing: Var,
    station_line_clearance: Var,
    label_spacing: Var,
    segment_length: Var,
    bounds: Var,
}

#[derive(Clone, Copy)]
struct Var {
    index: usize,
}

struct Node {
    value: f64,
    grad: f64,
    parents: Vec<(usize, f64)>,
}

struct Tape {
    nodes: Vec<Node>,
    variables: Vec<usize>,
}

pub fn load_routes(path: &Path) -> Result<Vec<Route>> {
    let text =
        fs::read_to_string(path).with_context(|| format!("Cannot read route file: {:?}", path))?;
    serde_json::from_str(&text).with_context(|| format!("Cannot parse route file: {:?}", path))
}

pub fn load_positions(position_dir: &Path) -> Result<HashMap<String, Stop>> {
    let mut stop_map = HashMap::new();

    for entry in fs::read_dir(position_dir)
        .with_context(|| format!("Cannot read position directory: {:?}", position_dir))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        let text = fs::read_to_string(&path).with_context(|| format!("Cannot read {:?}", path))?;
        let stops: Vec<Stop> =
            serde_json::from_str(&text).with_context(|| format!("Cannot parse {:?}", path))?;

        for stop in stops {
            stop_map.insert(stop.id.clone(), stop);
        }
    }

    Ok(stop_map)
}

pub fn optimize_routes(
    routes: Vec<Route>,
    stop_map: HashMap<String, Stop>,
    config: OptimizerConfig,
) -> Result<OptimizedMap> {
    let mut problem = Problem::new(routes, stop_map, config)?;
    let mut coords = problem.initial_coords.clone();
    let initial_cost = problem.cost_breakdown(&coords);

    let mut first_moment = vec![0.0; coords.len()];
    let mut second_moment = vec![0.0; coords.len()];
    let beta1 = 0.9_f64;
    let beta2 = 0.999_f64;
    let epsilon = 1e-8_f64;

    for iteration in 1..=problem.config.iterations {
        let order_interval = problem.config.parameters.transfer_order_update_interval;
        if order_interval > 0 && iteration > 1 && (iteration - 1) % order_interval == 0 {
            problem.update_transfer_orders_for_coords(&coords);
        }

        let (_, gradient) = problem.gradient_and_cost(&coords);
        let bias1 = 1.0 - beta1.powi(iteration as i32);
        let bias2 = 1.0 - beta2.powi(iteration as i32);

        for i in 0..coords.len() {
            let grad =
                gradient[i].clamp(-problem.config.gradient_clip, problem.config.gradient_clip);
            first_moment[i] = beta1 * first_moment[i] + (1.0 - beta1) * grad;
            second_moment[i] = beta2 * second_moment[i] + (1.0 - beta2) * grad * grad;

            let m_hat = first_moment[i] / bias1;
            let v_hat = second_moment[i] / bias2;
            coords[i] -= problem.config.learning_rate * m_hat / (v_hat.sqrt() + epsilon);
        }
    }

    let final_cost = problem.cost_breakdown(&coords);
    Ok(problem.to_optimized_map(&coords, initial_cost, final_cost))
}

pub fn render_svg(map: &OptimizedMap) -> String {
    let mut svg = String::new();
    let width = map.canvas.width;
    let height = map.canvas.height;
    let pad = map.canvas.padding;
    let max_route_name_chars = map
        .routes
        .iter()
        .map(|route| route.name.chars().count())
        .max()
        .unwrap_or(0) as f64;
    let legend_available_width = (width - 2.0 * pad).max(150.0);
    let legend_item_width = (max_route_name_chars * 7.2 + 48.0)
        .max(170.0)
        .min(legend_available_width);
    let legend_columns = ((width - 2.0 * pad) / legend_item_width).floor().max(1.0) as usize;
    let legend_rows = map.routes.len().max(1).div_ceil(legend_columns);
    let legend_height = 46.0 + legend_rows as f64 * 22.0;
    let output_height = height + legend_height;

    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0}" height="{output_height:.0}" viewBox="0 0 {width:.0} {output_height:.0}">
  <style>text {{ font-family: 'Noto Sans KR', sans-serif; }}</style>
  <rect width="{width:.0}" height="{output_height:.0}" fill="#f8f9fa"/>
"##
    ));

    let route_paths: Vec<Vec<Point>> = map.routes.iter().map(route_path_points).collect();

    for (route, path_points) in map.routes.iter().zip(route_paths.iter()) {
        svg.push_str(&format!("  <!-- Route: {} -->\n", escape_xml(&route.name)));
        svg.push_str(&svg_polyline(path_points, &route.style.color, 4.5));
        svg.push('\n');
    }

    for route in &map.routes {
        for stop in &route.stops {
            svg.push_str(&svg_stop_circle(
                stop.point,
                stop.passthrough,
                &route.style.color,
            ));
            svg.push('\n');
        }
    }

    for label in place_labels(map, &route_paths) {
        svg.push_str(&svg_label(
            label.center,
            &label.name,
            label.passthrough_only,
        ));
        svg.push('\n');
    }

    svg.push_str(&format!(
        r##"  <rect x="0" y="{:.2}" width="{:.0}" height="{:.2}" fill="#eeeeee"/>
  <line x1="0" y1="{:.2}" x2="{:.0}" y2="{:.2}" stroke="#d2d2d2" stroke-width="1"/>
  <text x="{:.2}" y="{:.2}" font-size="12" font-weight="bold" fill="#333333">범례</text>
"##,
        height,
        width,
        legend_height,
        height,
        width,
        height,
        pad,
        height + 24.0
    ));

    for (index, route) in map.routes.iter().enumerate() {
        let col = index % legend_columns;
        let row = index / legend_columns;
        let swatch_x = pad + col as f64 * legend_item_width;
        let swatch_y = height + 48.0 + row as f64 * 22.0;
        svg.push_str(&format!(
            r##"  <line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{}" stroke-width="4.5" stroke-linecap="round"/>
  <text x="{:.2}" y="{:.2}" dy="4" font-size="11" fill="#333333">{}</text>
"##,
            swatch_x,
            swatch_y,
            swatch_x + 24.0,
            swatch_y,
            route.style.color,
            swatch_x + 30.0,
            swatch_y,
            escape_xml(&route.name)
        ));
    }

    svg.push_str("</svg>\n");
    svg
}

impl Problem {
    fn new(
        routes: Vec<Route>,
        stop_map: HashMap<String, Stop>,
        config: OptimizerConfig,
    ) -> Result<Self> {
        let referenced_stops = collect_referenced_stops(&routes, &stop_map);
        if referenced_stops.is_empty() {
            bail!("No matching stops found. Check that route IDs match position IDs.");
        }

        let mut bounds = Bounds::from_stops(&referenced_stops);
        bounds.expand(config.parameters.bounds_expand_factor);

        let mut route_specs = Vec::new();
        let mut visits = Vec::new();
        let mut warnings = Vec::new();

        for route in routes {
            let route_index = route_specs.len();
            let mut visit_indices = Vec::new();

            for (route_position, raw_id) in route.route.iter().enumerate() {
                let passthrough = raw_id.ends_with('*');
                let id = raw_id.trim_end_matches('*');

                let Some(stop) = stop_map.get(id) else {
                    warnings.push(format!(
                        "Route '{}' references missing stop '{}'",
                        route.id, id
                    ));
                    continue;
                };

                let projected = project(stop.lat, stop.lng, &bounds, config.canvas);
                let visit_index = visits.len();
                visits.push(VisitSpec {
                    route_index,
                    route_position,
                    stop_id: stop.id.clone(),
                    stop_name: stop.name.clone(),
                    lat: stop.lat,
                    lng: stop.lng,
                    passthrough,
                    projected,
                });
                visit_indices.push(visit_index);
            }

            if visit_indices.len() < 2 {
                warnings.push(format!(
                    "Route '{}' has fewer than two resolved stops",
                    route.id
                ));
            }

            route_specs.push(RouteSpec {
                id: route.id,
                name: route.name,
                style: route.style,
                visit_indices,
            });
        }

        let segments = build_segments(&route_specs);
        let mut stop_groups = build_stop_groups(&visits);
        let projected_points = visits
            .iter()
            .map(|visit| visit.projected)
            .collect::<Vec<_>>();
        optimize_transfer_stop_orders(
            &mut stop_groups,
            &route_specs,
            &visits,
            config.parameters,
            &projected_points,
        );
        let corridor_groups = build_corridor_groups(&segments, &visits, config.parameters);
        let initial_coords = initial_coords(&visits, &stop_groups, config.parameters);

        Ok(Self {
            canvas: config.canvas,
            bounds,
            routes: route_specs,
            visits,
            segments,
            stop_groups,
            corridor_groups,
            initial_coords,
            config,
            warnings,
        })
    }

    fn update_transfer_orders_for_coords(&mut self, coords: &[f64]) {
        let points = (0..self.visits.len())
            .map(|visit_index| point_from_coords(coords, visit_index))
            .collect::<Vec<_>>();
        optimize_transfer_stop_orders(
            &mut self.stop_groups,
            &self.routes,
            &self.visits,
            self.config.parameters,
            &points,
        );
    }

    fn gradient_and_cost(&self, coords: &[f64]) -> (CostBreakdown, Vec<f64>) {
        let mut tape = Tape::new();
        let vars: Vec<Var> = coords.iter().map(|&value| tape.variable(value)).collect();
        let terms = self.cost_terms(&vars, &mut tape);
        let total = self.weighted_total(terms, &mut tape);
        let breakdown = self.breakdown_from_vars(terms, total, &tape);
        let gradient = tape.backward(total);
        (breakdown, gradient)
    }

    fn cost_breakdown(&self, coords: &[f64]) -> CostBreakdown {
        let mut tape = Tape::new();
        let vars: Vec<Var> = coords.iter().map(|&value| tape.variable(value)).collect();
        let terms = self.cost_terms(&vars, &mut tape);
        let total = self.weighted_total(terms, &mut tape);
        self.breakdown_from_vars(terms, total, &tape)
    }

    fn cost_terms(&self, vars: &[Var], tape: &mut Tape) -> CostVars {
        let centroids = self.group_centroids(vars, tape);
        CostVars {
            anchor: self.anchor_cost(vars, tape),
            octilinear: self.octilinear_cost(vars, tape),
            direction_preference: self.direction_preference_cost(vars, tape),
            bend: self.bend_cost(vars, tape),
            bend_angle_preference: self.bend_angle_preference_cost(vars, tape),
            overlap: self.overlap_cost(vars, tape),
            self_crossing: self.self_crossing_cost(vars, tape),
            shared_corridor_bundle: self.shared_corridor_bundle_cost(vars, tape),
            shared_stop_compactness: self.shared_stop_compactness_cost(vars, &centroids, tape),
            shared_stop_lane_gap: self.shared_stop_lane_gap_cost(vars, tape),
            transfer_alignment: self.transfer_alignment_cost(vars, &centroids, tape),
            shared_segment_order: self.shared_segment_order_cost(vars, tape),
            stop_spacing: self.stop_spacing_cost(&centroids, tape),
            station_line_clearance: self.station_line_clearance_cost(vars, &centroids, tape),
            label_spacing: self.label_spacing_cost(&centroids, tape),
            segment_length: self.segment_length_cost(vars, tape),
            bounds: self.bounds_cost(vars, tape),
        }
    }

    fn weighted_total(&self, terms: CostVars, tape: &mut Tape) -> Var {
        let weights = self.config.weights;
        let mut total = tape.zero();

        for (term, weight) in [
            (terms.anchor, weights.anchor),
            (terms.octilinear, weights.octilinear),
            (terms.direction_preference, weights.direction_preference),
            (terms.bend, weights.bend),
            (terms.bend_angle_preference, weights.bend_angle_preference),
            (terms.overlap, weights.overlap),
            (terms.self_crossing, weights.self_crossing),
            (terms.shared_corridor_bundle, weights.shared_corridor_bundle),
            (
                terms.shared_stop_compactness,
                weights.shared_stop_compactness,
            ),
            (terms.shared_stop_lane_gap, weights.shared_stop_lane_gap),
            (terms.transfer_alignment, weights.transfer_alignment),
            (terms.shared_segment_order, weights.shared_segment_order),
            (terms.stop_spacing, weights.stop_spacing),
            (terms.station_line_clearance, weights.station_line_clearance),
            (terms.label_spacing, weights.label_spacing),
            (terms.segment_length, weights.segment_length),
            (terms.bounds, weights.bounds),
        ] {
            let scaled = tape.scale(term, weight);
            total = tape.add(total, scaled);
        }

        total
    }

    fn breakdown_from_vars(&self, terms: CostVars, total: Var, tape: &Tape) -> CostBreakdown {
        CostBreakdown {
            total: tape.value(total),
            anchor: tape.value(terms.anchor),
            octilinear: tape.value(terms.octilinear),
            direction_preference: tape.value(terms.direction_preference),
            bend: tape.value(terms.bend),
            bend_angle_preference: tape.value(terms.bend_angle_preference),
            overlap: tape.value(terms.overlap),
            self_crossing: tape.value(terms.self_crossing),
            shared_corridor_bundle: tape.value(terms.shared_corridor_bundle),
            shared_stop_compactness: tape.value(terms.shared_stop_compactness),
            shared_stop_lane_gap: tape.value(terms.shared_stop_lane_gap),
            transfer_alignment: tape.value(terms.transfer_alignment),
            shared_segment_order: tape.value(terms.shared_segment_order),
            stop_spacing: tape.value(terms.stop_spacing),
            station_line_clearance: tape.value(terms.station_line_clearance),
            label_spacing: tape.value(terms.label_spacing),
            segment_length: tape.value(terms.segment_length),
            bounds: tape.value(terms.bounds),
        }
    }

    fn anchor_cost(&self, vars: &[Var], tape: &mut Tape) -> Var {
        let mut cost = tape.zero();
        for (visit_index, visit) in self.visits.iter().enumerate() {
            let term =
                squared_distance_to_point(visit_point(vars, visit_index), visit.projected, tape);
            cost = tape.add(cost, term);
        }
        cost
    }

    fn octilinear_cost(&self, vars: &[Var], tape: &mut Tape) -> Var {
        let mut cost = tape.zero();
        for segment in &self.segments {
            let a = visit_point(vars, segment.a_visit);
            let b = visit_point(vars, segment.b_visit);
            let direction = nearest_octilinear_direction(value_delta(a, b, tape));
            let term = segment_direction_cost(a, b, direction, tape);
            cost = tape.add(cost, term);
        }
        cost
    }

    fn direction_preference_cost(&self, vars: &[Var], tape: &mut Tape) -> Var {
        let mut cost = tape.zero();
        for route in &self.routes {
            for window in route.visit_indices.windows(3) {
                let a = visit_point(vars, window[0]);
                let b = visit_point(vars, window[1]);
                let c = visit_point(vars, window[2]);
                let ab_dir = nearest_octilinear_direction(value_delta(a, b, tape));
                let bc_dir = nearest_octilinear_direction(value_delta(b, c, tape));
                let dot = ab_dir.x * bc_dir.x + ab_dir.y * bc_dir.y;
                if dot > 0.98 {
                    continue;
                }

                let ab_len = squared_distance(a, b, tape);
                let bc_len = squared_distance(b, c, tape);
                let turn_strength = if dot < -0.50 { 2.0 } else { 1.0 };
                let total_len = tape.add(ab_len, bc_len);
                let term = tape.scale(total_len, turn_strength);
                cost = tape.add(cost, term);
            }
        }
        cost
    }

    fn bend_cost(&self, vars: &[Var], tape: &mut Tape) -> Var {
        let mut cost = tape.zero();
        let min_len2 = self.config.parameters.min_bend_segment_length.powi(2);

        for route in &self.routes {
            for window in route.visit_indices.windows(3) {
                let a = visit_point(vars, window[0]);
                let b = visit_point(vars, window[1]);
                let c = visit_point(vars, window[2]);
                let ab_dir = nearest_octilinear_direction(value_delta(a, b, tape));
                let bc_dir = nearest_octilinear_direction(value_delta(b, c, tape));
                if directions_collinear(ab_dir, bc_dir) {
                    continue;
                }

                let straightness = point_to_line_cost(b, a, c, tape);
                cost = tape.add(cost, straightness);

                for dist2 in [squared_distance(a, b, tape), squared_distance(b, c, tape)] {
                    if tape.value(dist2) < min_len2 {
                        let limit = tape.constant(min_len2);
                        let gap = tape.sub(limit, dist2);
                        let term = tape.sqr(gap);
                        cost = tape.add(cost, term);
                    }
                }
            }
        }

        cost
    }

    fn bend_angle_preference_cost(&self, vars: &[Var], tape: &mut Tape) -> Var {
        let mut cost = tape.zero();
        let margin = self.config.parameters.bend_angle_preference_margin;

        for route in &self.routes {
            for window in route.visit_indices.windows(3) {
                let a = visit_point(vars, window[0]);
                let b = visit_point(vars, window[1]);
                let c = visit_point(vars, window[2]);

                let ba_x = tape.sub(a.x, b.x);
                let ba_y = tape.sub(a.y, b.y);
                let bc_x = tape.sub(c.x, b.x);
                let bc_y = tape.sub(c.y, b.y);
                let dot_x = tape.mul(ba_x, bc_x);
                let dot_y = tape.mul(ba_y, bc_y);
                let dot = tape.add(dot_x, dot_y);

                if tape.value(dot) + margin <= 0.0 {
                    continue;
                }

                let threshold = tape.constant(margin);
                let hinge = tape.add(dot, threshold);
                let term = tape.sqr(hinge);
                cost = tape.add(cost, term);
            }
        }

        cost
    }

    fn overlap_cost(&self, vars: &[Var], tape: &mut Tape) -> Var {
        let mut cost = tape.zero();
        let clearance2 = self.config.parameters.overlap_clearance.powi(2);

        for i in 0..self.segments.len() {
            for j in (i + 1)..self.segments.len() {
                let a = &self.segments[i];
                let b = &self.segments[j];
                if skip_segment_pair_for_spacing(a, b, &self.visits) {
                    continue;
                }

                for ta in OVERLAP_SAMPLES {
                    for tb in OVERLAP_SAMPLES {
                        let pa = segment_sample(a, ta, vars, tape);
                        let pb = segment_sample(b, tb, vars, tape);
                        let dist2 = squared_distance(pa, pb, tape);
                        if tape.value(dist2) < clearance2 {
                            let limit = tape.constant(clearance2);
                            let gap = tape.sub(limit, dist2);
                            let term = tape.sqr(gap);
                            cost = tape.add(cost, term);
                        }
                    }
                }
            }
        }

        cost
    }

    fn self_crossing_cost(&self, vars: &[Var], tape: &mut Tape) -> Var {
        let mut cost = tape.zero();
        let clearance2 = self.config.parameters.self_crossing_clearance.powi(2);

        for i in 0..self.segments.len() {
            for j in (i + 1)..self.segments.len() {
                let a = &self.segments[i];
                let b = &self.segments[j];
                if a.route_index != b.route_index || same_or_adjacent_segment(a, b) {
                    continue;
                }
                if !segments_currently_cross(a, b, vars, tape) {
                    continue;
                }

                let ab_term = signed_side_separation_cost(a, b, clearance2, vars, tape);
                cost = tape.add(cost, ab_term);
                let ba_term = signed_side_separation_cost(b, a, clearance2, vars, tape);
                cost = tape.add(cost, ba_term);
            }
        }

        cost
    }

    fn shared_corridor_bundle_cost(&self, vars: &[Var], tape: &mut Tape) -> Var {
        let mut cost = tape.zero();

        for group in &self.corridor_groups {
            if group.members.len() <= 1 {
                continue;
            }

            let (center_a, center_b) = corridor_centroids(group, &self.segments, vars, tape);
            let direction = nearest_octilinear_direction(value_delta(center_a, center_b, tape));
            let normal = Direction::new(-direction.y, direction.x);

            for member in &group.members {
                let (point_a, point_b) = corridor_member_points(member, &self.segments, vars);
                let a_term = lane_endpoint_cost(
                    point_a,
                    center_a,
                    direction,
                    normal,
                    member.lane_offset,
                    tape,
                );
                cost = tape.add(cost, a_term);
                let b_term = lane_endpoint_cost(
                    point_b,
                    center_b,
                    direction,
                    normal,
                    member.lane_offset,
                    tape,
                );
                cost = tape.add(cost, b_term);
                let parallel = segment_direction_cost(point_a, point_b, direction, tape);
                let parallel = tape.scale(parallel, 0.18);
                cost = tape.add(cost, parallel);
            }
        }

        cost
    }

    fn shared_stop_compactness_cost(
        &self,
        vars: &[Var],
        centroids: &[AdPoint],
        tape: &mut Tape,
    ) -> Var {
        let mut cost = tape.zero();
        for (group, centroid) in self.stop_groups.iter().zip(centroids.iter()) {
            if group.visits.len() <= 1 {
                continue;
            }
            for &visit_index in &group.visits {
                let term = squared_distance(visit_point(vars, visit_index), *centroid, tape);
                cost = tape.add(cost, term);
            }
        }
        cost
    }

    fn shared_stop_lane_gap_cost(&self, vars: &[Var], tape: &mut Tape) -> Var {
        let mut cost = tape.zero();
        let target2 = self.config.parameters.shared_lane_gap.powi(2);

        for group in &self.stop_groups {
            if group.visits.len() <= 1 {
                continue;
            }
            for pair in group.visits.windows(2) {
                let a = visit_point(vars, pair[0]);
                let b = visit_point(vars, pair[1]);
                let dist2 = squared_distance(a, b, tape);
                let target = tape.constant(target2);
                let delta = tape.sub(dist2, target);
                let term = tape.sqr(delta);
                cost = tape.add(cost, term);
            }
        }

        cost
    }

    fn transfer_alignment_cost(&self, vars: &[Var], centroids: &[AdPoint], tape: &mut Tape) -> Var {
        let mut cost = tape.zero();

        for (group, centroid) in self.stop_groups.iter().zip(centroids.iter()) {
            if group.visits.len() <= 1 {
                continue;
            }

            let axis = best_transfer_axis(
                group,
                vars,
                tape,
                self.config.parameters.shared_lane_gap,
                self.config.parameters.transfer_alignment_hardness,
            );
            let dir = axis.direction();
            let middle = (group.visits.len() as f64 - 1.0) / 2.0;
            let gap2 = self.config.parameters.shared_lane_gap.powi(2).max(1.0);

            for (slot, &visit_index) in group.visits.iter().enumerate() {
                let offset = (slot as f64 - middle) * self.config.parameters.shared_lane_gap;
                let target_x_offset = tape.constant(dir.x * offset);
                let target_y_offset = tape.constant(dir.y * offset);
                let target = AdPoint {
                    x: tape.add(centroid.x, target_x_offset),
                    y: tape.add(centroid.y, target_y_offset),
                };
                let d2 = squared_distance(visit_point(vars, visit_index), target, tape);
                let d4 = tape.sqr(d2);
                let hard_term = tape.scale(
                    d4,
                    self.config.parameters.transfer_alignment_hardness / gap2,
                );
                let term = tape.add(d2, hard_term);
                cost = tape.add(cost, term);
            }
        }

        cost
    }

    fn shared_segment_order_cost(&self, vars: &[Var], tape: &mut Tape) -> Var {
        let mut cost = tape.zero();

        for group in &self.corridor_groups {
            if group.members.len() <= 1 {
                continue;
            }

            let (center_a, center_b) = corridor_centroids(group, &self.segments, vars, tape);
            let direction = nearest_octilinear_direction(value_delta(center_a, center_b, tape));
            let normal = Direction::new(-direction.y, direction.x);

            for pair in group.members.windows(2) {
                let left = &pair[0];
                let right = &pair[1];
                let (left_a, left_b) = corridor_member_points(left, &self.segments, vars);
                let (right_a, right_b) = corridor_member_points(right, &self.segments, vars);
                let expected_gap = right.lane_offset - left.lane_offset;
                let a_term =
                    signed_lane_gap_cost(left_a, right_a, center_a, normal, expected_gap, tape);
                cost = tape.add(cost, a_term);
                let b_term =
                    signed_lane_gap_cost(left_b, right_b, center_b, normal, expected_gap, tape);
                cost = tape.add(cost, b_term);
            }
        }

        cost
    }

    fn stop_spacing_cost(&self, centroids: &[AdPoint], tape: &mut Tape) -> Var {
        let mut cost = tape.zero();
        let clearance2 = self.config.parameters.stop_clearance.powi(2);

        for i in 0..centroids.len() {
            for j in (i + 1)..centroids.len() {
                let dist2 = squared_distance(centroids[i], centroids[j], tape);
                if tape.value(dist2) < clearance2 {
                    let limit = tape.constant(clearance2);
                    let gap = tape.sub(limit, dist2);
                    let term = tape.sqr(gap);
                    cost = tape.add(cost, term);
                }
            }
        }

        cost
    }

    fn station_line_clearance_cost(
        &self,
        vars: &[Var],
        centroids: &[AdPoint],
        tape: &mut Tape,
    ) -> Var {
        let mut cost = tape.zero();
        let clearance2 = self.config.parameters.station_line_clearance.powi(2);

        for (group_index, group) in self.stop_groups.iter().enumerate() {
            let stop_center = centroids[group_index];

            for segment in &self.segments {
                let a_stop = &self.visits[segment.a_visit].stop_id;
                let b_stop = &self.visits[segment.b_visit].stop_id;
                if a_stop == &group.stop_id || b_stop == &group.stop_id {
                    continue;
                }

                for t in OVERLAP_SAMPLES {
                    let sample = segment_sample(segment, t, vars, tape);
                    let dist2 = squared_distance(stop_center, sample, tape);
                    if tape.value(dist2) < clearance2 {
                        let limit = tape.constant(clearance2);
                        let gap = tape.sub(limit, dist2);
                        let term = tape.sqr(gap);
                        cost = tape.add(cost, term);
                    }
                }
            }
        }

        cost
    }

    fn label_spacing_cost(&self, centroids: &[AdPoint], tape: &mut Tape) -> Var {
        let mut cost = tape.zero();
        let params = self.config.parameters;

        for i in 0..self.stop_groups.len() {
            let label_i = label_box(centroids[i], &self.stop_groups[i].stop_name, params, tape);

            for j in (i + 1)..self.stop_groups.len() {
                let label_j = label_box(centroids[j], &self.stop_groups[j].stop_name, params, tape);
                if let Some(term) = box_overlap_cost(label_i, label_j, params.label_clearance, tape)
                {
                    cost = tape.add(cost, term);
                }
            }

            for (j, stop_center) in centroids.iter().enumerate() {
                if i == j {
                    continue;
                }
                if let Some(term) = label_point_overlap_cost(
                    label_i,
                    *stop_center,
                    params.stop_clearance * 0.38,
                    tape,
                ) {
                    cost = tape.add(cost, term);
                }
            }
        }

        cost
    }

    fn segment_length_cost(&self, vars: &[Var], tape: &mut Tape) -> Var {
        let mut cost = tape.zero();
        let min_len2 = self.config.parameters.min_segment_length.powi(2);

        for segment in &self.segments {
            let dist2 = squared_distance(
                visit_point(vars, segment.a_visit),
                visit_point(vars, segment.b_visit),
                tape,
            );
            if tape.value(dist2) < min_len2 {
                let limit = tape.constant(min_len2);
                let gap = tape.sub(limit, dist2);
                let term = tape.sqr(gap);
                cost = tape.add(cost, term);
            }
        }

        cost
    }

    fn bounds_cost(&self, vars: &[Var], tape: &mut Tape) -> Var {
        let mut cost = tape.zero();
        let min_x = self.canvas.padding * 0.45;
        let min_y = self.canvas.padding * 0.45;
        let max_x = self.canvas.width - self.canvas.padding * 0.45;
        let max_y = self.canvas.height - self.canvas.padding * 0.45;

        for visit_index in 0..self.visits.len() {
            let p = visit_point(vars, visit_index);
            let x_term = outside_interval_cost(p.x, min_x, max_x, tape);
            let y_term = outside_interval_cost(p.y, min_y, max_y, tape);
            cost = tape.add(cost, x_term);
            cost = tape.add(cost, y_term);
        }

        cost
    }

    fn group_centroids(&self, vars: &[Var], tape: &mut Tape) -> Vec<AdPoint> {
        self.stop_groups
            .iter()
            .map(|group| {
                let mut x = tape.zero();
                let mut y = tape.zero();
                for &visit_index in &group.visits {
                    let p = visit_point(vars, visit_index);
                    x = tape.add(x, p.x);
                    y = tape.add(y, p.y);
                }
                let scale = 1.0 / group.visits.len() as f64;
                AdPoint {
                    x: tape.scale(x, scale),
                    y: tape.scale(y, scale),
                }
            })
            .collect()
    }

    fn to_optimized_map(
        &self,
        coords: &[f64],
        initial_cost: CostBreakdown,
        final_cost: CostBreakdown,
    ) -> OptimizedMap {
        let routes = self
            .routes
            .iter()
            .map(|route| OptimizedRoute {
                id: route.id.clone(),
                name: route.name.clone(),
                style: route.style.clone(),
                stops: route
                    .visit_indices
                    .iter()
                    .map(|&visit_index| {
                        let visit = &self.visits[visit_index];
                        OptimizedRouteStop {
                            id: visit.stop_id.clone(),
                            name: visit.stop_name.clone(),
                            passthrough: visit.passthrough,
                            point: point_from_coords(coords, visit_index),
                        }
                    })
                    .collect(),
            })
            .collect();

        let stops = self
            .stop_groups
            .iter()
            .map(|group| {
                let optimized = centroid_from_coords(coords, &group.visits);
                let visits = group
                    .visits
                    .iter()
                    .map(|&visit_index| {
                        let visit = &self.visits[visit_index];
                        let route = &self.routes[visit.route_index];
                        OptimizedStopVisit {
                            route_id: route.id.clone(),
                            route_name: route.name.clone(),
                            passthrough: visit.passthrough,
                            point: point_from_coords(coords, visit_index),
                        }
                    })
                    .collect();

                OptimizedStop {
                    id: group.stop_id.clone(),
                    name: group.stop_name.clone(),
                    lat: group.lat,
                    lng: group.lng,
                    projected: group.projected,
                    optimized,
                    visits,
                }
            })
            .collect();

        OptimizedMap {
            schema: SCHEMA.to_string(),
            canvas: self.canvas,
            bounds: self.bounds,
            routes,
            stops,
            optimization: OptimizationReport {
                iterations: self.config.iterations,
                learning_rate: self.config.learning_rate,
                gradient_clip: self.config.gradient_clip,
                weights: self.config.weights,
                parameters: self.config.parameters,
                initial_cost,
                final_cost,
                warnings: self.warnings.clone(),
            },
        }
    }
}

impl Bounds {
    fn from_stops(stops: &[&Stop]) -> Self {
        let min_lat = stops.iter().map(|s| s.lat).fold(f64::INFINITY, f64::min);
        let max_lat = stops
            .iter()
            .map(|s| s.lat)
            .fold(f64::NEG_INFINITY, f64::max);
        let min_lng = stops.iter().map(|s| s.lng).fold(f64::INFINITY, f64::min);
        let max_lng = stops
            .iter()
            .map(|s| s.lng)
            .fold(f64::NEG_INFINITY, f64::max);

        Self {
            min_lat,
            max_lat,
            min_lng,
            max_lng,
        }
    }

    fn expand(&mut self, factor: f64) {
        let lat_span = (self.max_lat - self.min_lat).abs().max(0.00001);
        let lng_span = (self.max_lng - self.min_lng).abs().max(0.00001);
        self.min_lat -= lat_span * factor;
        self.max_lat += lat_span * factor;
        self.min_lng -= lng_span * factor;
        self.max_lng += lng_span * factor;
    }
}

impl Tape {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            variables: Vec::new(),
        }
    }

    fn variable(&mut self, value: f64) -> Var {
        let var = self.push(value, Vec::new());
        self.variables.push(var.index);
        var
    }

    fn constant(&mut self, value: f64) -> Var {
        self.push(value, Vec::new())
    }

    fn zero(&mut self) -> Var {
        self.constant(0.0)
    }

    fn value(&self, var: Var) -> f64 {
        self.nodes[var.index].value
    }

    fn add(&mut self, a: Var, b: Var) -> Var {
        self.push(
            self.value(a) + self.value(b),
            vec![(a.index, 1.0), (b.index, 1.0)],
        )
    }

    fn sub(&mut self, a: Var, b: Var) -> Var {
        self.push(
            self.value(a) - self.value(b),
            vec![(a.index, 1.0), (b.index, -1.0)],
        )
    }

    fn scale(&mut self, a: Var, factor: f64) -> Var {
        self.push(self.value(a) * factor, vec![(a.index, factor)])
    }

    fn mul(&mut self, a: Var, b: Var) -> Var {
        self.push(
            self.value(a) * self.value(b),
            vec![(a.index, self.value(b)), (b.index, self.value(a))],
        )
    }

    fn sqr(&mut self, a: Var) -> Var {
        self.push(
            self.value(a) * self.value(a),
            vec![(a.index, 2.0 * self.value(a))],
        )
    }

    fn backward(&mut self, loss: Var) -> Vec<f64> {
        self.nodes[loss.index].grad = 1.0;

        for index in (0..self.nodes.len()).rev() {
            let grad = self.nodes[index].grad;
            let parents = self.nodes[index].parents.clone();
            for (parent, local_grad) in parents {
                self.nodes[parent].grad += grad * local_grad;
            }
        }

        self.variables
            .iter()
            .map(|&index| self.nodes[index].grad)
            .collect()
    }

    fn push(&mut self, value: f64, parents: Vec<(usize, f64)>) -> Var {
        let index = self.nodes.len();
        self.nodes.push(Node {
            value,
            grad: 0.0,
            parents,
        });
        Var { index }
    }
}

fn collect_referenced_stops<'a>(
    routes: &[Route],
    stop_map: &'a HashMap<String, Stop>,
) -> Vec<&'a Stop> {
    routes
        .iter()
        .flat_map(|route| route.route.iter())
        .filter_map(|raw_id| stop_map.get(raw_id.trim_end_matches('*')))
        .collect()
}

fn project(lat: f64, lng: f64, bounds: &Bounds, canvas: Canvas) -> Point {
    let drawable_width = canvas.width - 2.0 * canvas.padding;
    let drawable_height = canvas.height - 2.0 * canvas.padding;
    let lng_span = (bounds.max_lng - bounds.min_lng).abs().max(f64::EPSILON);
    let lat_span = (bounds.max_lat - bounds.min_lat).abs().max(f64::EPSILON);
    let scale = f64::min(drawable_width / lng_span, drawable_height / lat_span);

    Point {
        x: (lng - bounds.min_lng) * scale + canvas.padding,
        y: (drawable_height - (lat - bounds.min_lat) * scale) + canvas.padding,
    }
}

fn build_segments(routes: &[RouteSpec]) -> Vec<SegmentSpec> {
    let mut segments = Vec::new();
    for (route_index, route) in routes.iter().enumerate() {
        for (segment_index, pair) in route.visit_indices.windows(2).enumerate() {
            segments.push(SegmentSpec {
                route_index,
                segment_index,
                a_visit: pair[0],
                b_visit: pair[1],
            });
        }
    }
    segments
}

fn build_stop_groups(visits: &[VisitSpec]) -> Vec<StopGroup> {
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (visit_index, visit) in visits.iter().enumerate() {
        groups
            .entry(visit.stop_id.clone())
            .or_default()
            .push(visit_index);
    }

    groups
        .into_iter()
        .map(|(stop_id, mut group_visits)| {
            group_visits.sort_by_key(|&visit_index| {
                let visit = &visits[visit_index];
                (visit.route_index, visit.route_position, visit_index)
            });
            let first = &visits[group_visits[0]];
            StopGroup {
                stop_id,
                stop_name: first.stop_name.clone(),
                lat: first.lat,
                lng: first.lng,
                projected: first.projected,
                visits: group_visits,
            }
        })
        .collect()
}

fn optimize_transfer_stop_orders(
    stop_groups: &mut [StopGroup],
    routes: &[RouteSpec],
    visits: &[VisitSpec],
    parameters: CostParameters,
    points: &[Point],
) {
    for group in stop_groups {
        if group.visits.len() <= 2 {
            continue;
        }

        let center = transfer_order_center(&group.visits, points).unwrap_or(group.projected);
        group.visits =
            best_transfer_visit_order(&group.visits, center, routes, visits, parameters, points);
    }
}

fn transfer_order_center(order: &[usize], points: &[Point]) -> Option<Point> {
    if order.is_empty() {
        return None;
    }

    let mut x = 0.0;
    let mut y = 0.0;
    for &visit_index in order {
        let point = points[visit_index];
        x += point.x;
        y += point.y;
    }

    Some(Point {
        x: x / order.len() as f64,
        y: y / order.len() as f64,
    })
}

fn best_transfer_visit_order(
    base_order: &[usize],
    center: Point,
    routes: &[RouteSpec],
    visits: &[VisitSpec],
    parameters: CostParameters,
    points: &[Point],
) -> Vec<usize> {
    let mut best_order = base_order.to_vec();
    let mut best_score =
        transfer_order_score(&best_order, center, routes, visits, parameters, points);

    if base_order.len() <= parameters.transfer_order_search_limit {
        let mut current = best_order.clone();
        search_transfer_order_permutations(
            0,
            &mut current,
            center,
            routes,
            visits,
            parameters,
            points,
            &mut best_order,
            &mut best_score,
        );
    } else {
        best_order = improve_transfer_order_by_swaps(
            best_order,
            center,
            routes,
            visits,
            parameters,
            points,
            &mut best_score,
        );
    }

    best_order
}

fn search_transfer_order_permutations(
    start: usize,
    current: &mut [usize],
    center: Point,
    routes: &[RouteSpec],
    visits: &[VisitSpec],
    parameters: CostParameters,
    points: &[Point],
    best_order: &mut Vec<usize>,
    best_score: &mut f64,
) {
    if start == current.len() {
        let score = transfer_order_score(current, center, routes, visits, parameters, points);
        if score + 1e-6 < *best_score {
            *best_score = score;
            *best_order = current.to_vec();
        }
        return;
    }

    for index in start..current.len() {
        current.swap(start, index);
        search_transfer_order_permutations(
            start + 1,
            current,
            center,
            routes,
            visits,
            parameters,
            points,
            best_order,
            best_score,
        );
        current.swap(start, index);
    }
}

fn improve_transfer_order_by_swaps(
    mut order: Vec<usize>,
    center: Point,
    routes: &[RouteSpec],
    visits: &[VisitSpec],
    parameters: CostParameters,
    points: &[Point],
    best_score: &mut f64,
) -> Vec<usize> {
    let mut improved = true;
    while improved {
        improved = false;
        for index in 0..order.len().saturating_sub(1) {
            let mut candidate = order.clone();
            candidate.swap(index, index + 1);
            let score =
                transfer_order_score(&candidate, center, routes, visits, parameters, points);
            if score + 1e-6 < *best_score {
                *best_score = score;
                order = candidate;
                improved = true;
            }
        }
    }
    order
}

fn transfer_order_score(
    order: &[usize],
    center: Point,
    routes: &[RouteSpec],
    visits: &[VisitSpec],
    parameters: CostParameters,
    points: &[Point],
) -> f64 {
    TransferAxis::ALL
        .into_iter()
        .map(|axis| {
            transfer_order_axis_score(order, center, axis, routes, visits, parameters, points)
        })
        .fold(f64::INFINITY, f64::min)
}

fn transfer_order_axis_score(
    order: &[usize],
    center: Point,
    axis: TransferAxis,
    routes: &[RouteSpec],
    visits: &[VisitSpec],
    parameters: CostParameters,
    points: &[Point],
) -> f64 {
    let direction = axis.direction();
    let normal = Direction::new(-direction.y, direction.x);
    let middle = (order.len() as f64 - 1.0) / 2.0;
    let mut slot_points = HashMap::new();

    for (slot, &visit_index) in order.iter().enumerate() {
        let offset = (slot as f64 - middle) * parameters.shared_lane_gap;
        slot_points.insert(
            visit_index,
            Point {
                x: center.x + direction.x * offset,
                y: center.y + direction.y * offset,
            },
        );
    }

    let arms = transfer_order_arms(order, &slot_points, routes, visits, points);
    let mut score = transfer_order_arm_score(&arms, parameters);

    for arm in &arms {
        let slot_side = (arm.from.x - center.x) * normal.x + (arm.from.y - center.y) * normal.y;
        let target_side = (arm.to.x - center.x) * normal.x + (arm.to.y - center.y) * normal.y;
        if slot_side.abs() > f64::EPSILON
            && target_side.abs() > f64::EPSILON
            && slot_side.signum() != target_side.signum()
        {
            score += target_side.abs().min(80.0) * 3.0;
        }
    }

    score
}

fn transfer_order_arms(
    order: &[usize],
    slot_points: &HashMap<usize, Point>,
    routes: &[RouteSpec],
    visits: &[VisitSpec],
    points: &[Point],
) -> Vec<TransferArm> {
    let mut arms = Vec::new();

    for &visit_index in order {
        let Some(&from) = slot_points.get(&visit_index) else {
            continue;
        };
        let visit = &visits[visit_index];
        let route = &routes[visit.route_index];
        let Some(position) = route
            .visit_indices
            .iter()
            .position(|&candidate| candidate == visit_index)
        else {
            continue;
        };

        let mut neighbors = Vec::with_capacity(2);
        if position > 0 {
            neighbors.push(route.visit_indices[position - 1]);
        }
        if position + 1 < route.visit_indices.len() {
            neighbors.push(route.visit_indices[position + 1]);
        }

        for neighbor_index in neighbors {
            if visits[neighbor_index].stop_id == visit.stop_id {
                continue;
            }
            arms.push(TransferArm {
                route_index: visit.route_index,
                visit_index,
                from,
                to: points[neighbor_index],
            });
        }
    }

    arms
}

fn transfer_order_arm_score(arms: &[TransferArm], parameters: CostParameters) -> f64 {
    let mut score = 0.0;
    let clearance2 = parameters.overlap_clearance.powi(2);

    for i in 0..arms.len() {
        for j in (i + 1)..arms.len() {
            let a = arms[i];
            let b = arms[j];
            if a.route_index == b.route_index || a.visit_index == b.visit_index {
                continue;
            }

            if let Some((ta, tb)) = segment_intersection_params(a.from, a.to, b.from, b.to) {
                if (0.12..0.96).contains(&ta) && (0.12..0.96).contains(&tb) {
                    score += 25_000.0;
                }
            }

            for ta in OVERLAP_SAMPLES {
                for tb in OVERLAP_SAMPLES {
                    let pa = lerp_point(a.from, a.to, ta);
                    let pb = lerp_point(b.from, b.to, tb);
                    let dist2 = point_distance2(pa, pb);
                    if dist2 < clearance2 {
                        let gap = clearance2 - dist2;
                        score += gap * gap * 0.015;
                    }
                }
            }
        }
    }

    score
}

fn build_corridor_groups(
    segments: &[SegmentSpec],
    visits: &[VisitSpec],
    parameters: CostParameters,
) -> Vec<CorridorGroup> {
    let mut groups: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();

    for (segment_index, segment) in segments.iter().enumerate() {
        let a = &visits[segment.a_visit].stop_id;
        let b = &visits[segment.b_visit].stop_id;
        if a != b {
            groups
                .entry(ordered_corridor_key(a, b))
                .or_default()
                .push(segment_index);
        }
    }

    groups
        .into_iter()
        .filter_map(|((stop_a, _), mut segment_indices)| {
            let distinct_routes: BTreeSet<usize> = segment_indices
                .iter()
                .map(|&segment_index| segments[segment_index].route_index)
                .collect();
            if distinct_routes.len() <= 1 {
                return None;
            }

            segment_indices.sort_by_key(|&segment_index| {
                (
                    segments[segment_index].route_index,
                    segments[segment_index].segment_index,
                    segment_index,
                )
            });

            let middle = (segment_indices.len() as f64 - 1.0) / 2.0;
            let members = segment_indices
                .into_iter()
                .enumerate()
                .map(|(slot, segment_index)| {
                    let segment = &segments[segment_index];
                    CorridorMember {
                        segment_index,
                        key_forward: visits[segment.a_visit].stop_id == stop_a,
                        lane_offset: (slot as f64 - middle) * parameters.shared_corridor_gap,
                    }
                })
                .collect();

            Some(CorridorGroup { members })
        })
        .collect()
}

fn initial_coords(
    visits: &[VisitSpec],
    stop_groups: &[StopGroup],
    parameters: CostParameters,
) -> Vec<f64> {
    let mut coords = Vec::with_capacity(visits.len() * 2);
    for visit in visits {
        coords.push(visit.projected.x);
        coords.push(visit.projected.y);
    }

    for group in stop_groups {
        if group.visits.len() <= 1 {
            continue;
        }
        let middle = (group.visits.len() as f64 - 1.0) / 2.0;
        for (slot, &visit_index) in group.visits.iter().enumerate() {
            let offset = (slot as f64 - middle) * parameters.initial_shared_offset;
            coords[visit_index * 2] += offset;
        }
    }

    coords
}

fn point_from_coords(coords: &[f64], visit_index: usize) -> Point {
    Point {
        x: coords[visit_index * 2],
        y: coords[visit_index * 2 + 1],
    }
}

fn centroid_from_coords(coords: &[f64], visits: &[usize]) -> Point {
    let mut x = 0.0;
    let mut y = 0.0;
    for &visit_index in visits {
        let point = point_from_coords(coords, visit_index);
        x += point.x;
        y += point.y;
    }
    Point {
        x: x / visits.len() as f64,
        y: y / visits.len() as f64,
    }
}

fn visit_point(vars: &[Var], visit_index: usize) -> AdPoint {
    AdPoint {
        x: vars[visit_index * 2],
        y: vars[visit_index * 2 + 1],
    }
}

fn value_delta(a: AdPoint, b: AdPoint, tape: &Tape) -> (f64, f64) {
    (
        tape.value(b.x) - tape.value(a.x),
        tape.value(b.y) - tape.value(a.y),
    )
}

fn nearest_octilinear_direction(delta: (f64, f64)) -> Direction {
    let (dx, dy) = delta;
    let len = (dx * dx + dy * dy).sqrt();
    if len < f64::EPSILON {
        return OCTILINEAR_DIRECTIONS[0];
    }
    let ux = dx / len;
    let uy = dy / len;
    OCTILINEAR_DIRECTIONS
        .iter()
        .copied()
        .max_by(|a, b| {
            let dot_a = ux * a.x + uy * a.y;
            let dot_b = ux * b.x + uy * b.y;
            dot_a
                .partial_cmp(&dot_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(OCTILINEAR_DIRECTIONS[0])
}

fn directions_collinear(a: Direction, b: Direction) -> bool {
    (a.x * b.x + a.y * b.y).abs() > 0.98
}

fn squared_distance(a: AdPoint, b: AdPoint, tape: &mut Tape) -> Var {
    let dx = tape.sub(a.x, b.x);
    let dy = tape.sub(a.y, b.y);
    let dx2 = tape.sqr(dx);
    let dy2 = tape.sqr(dy);
    tape.add(dx2, dy2)
}

fn squared_distance_to_point(a: AdPoint, b: Point, tape: &mut Tape) -> Var {
    let bx = tape.constant(b.x);
    let by = tape.constant(b.y);
    squared_distance(a, AdPoint { x: bx, y: by }, tape)
}

fn segment_direction_cost(a: AdPoint, b: AdPoint, direction: Direction, tape: &mut Tape) -> Var {
    let dx = tape.sub(b.x, a.x);
    let dy = tape.sub(b.y, a.y);
    let left = tape.scale(dx, direction.y);
    let right = tape.scale(dy, direction.x);
    let cross = tape.sub(left, right);
    tape.sqr(cross)
}

fn point_to_line_cost(point: AdPoint, line_a: AdPoint, line_b: AdPoint, tape: &mut Tape) -> Var {
    let line_dx = tape.sub(line_b.x, line_a.x);
    let line_dy = tape.sub(line_b.y, line_a.y);
    let point_dx = tape.sub(point.x, line_a.x);
    let point_dy = tape.sub(point.y, line_a.y);
    let left = tape.mul(line_dx, point_dy);
    let right = tape.mul(line_dy, point_dx);
    let cross = tape.sub(left, right);
    let line_len2 = {
        let dx = tape.value(line_b.x) - tape.value(line_a.x);
        let dy = tape.value(line_b.y) - tape.value(line_a.y);
        (dx * dx + dy * dy).max(1.0)
    };
    let cross2 = tape.sqr(cross);
    tape.scale(cross2, 1.0 / line_len2)
}

fn segment_sample(segment: &SegmentSpec, t: f64, vars: &[Var], tape: &mut Tape) -> AdPoint {
    let a = visit_point(vars, segment.a_visit);
    let b = visit_point(vars, segment.b_visit);
    let ax = tape.scale(a.x, 1.0 - t);
    let bx = tape.scale(b.x, t);
    let ay = tape.scale(a.y, 1.0 - t);
    let by = tape.scale(b.y, t);
    AdPoint {
        x: tape.add(ax, bx),
        y: tape.add(ay, by),
    }
}

fn signed_side_separation_cost(
    line: &SegmentSpec,
    other: &SegmentSpec,
    clearance2: f64,
    vars: &[Var],
    tape: &mut Tape,
) -> Var {
    let side_a = signed_cross_to_line(line, other.a_visit, vars, tape);
    let side_b = signed_cross_to_line(line, other.b_visit, vars, tape);
    let product = tape.mul(side_a, side_b);
    let normalized = tape.scale(
        product,
        1.0 / segment_length2_value(line, vars, tape).max(1.0),
    );

    if tape.value(normalized) >= clearance2 {
        return tape.zero();
    }

    let limit = tape.constant(clearance2);
    let gap = tape.sub(limit, normalized);
    tape.sqr(gap)
}

fn signed_cross_to_line(
    line: &SegmentSpec,
    point_visit: usize,
    vars: &[Var],
    tape: &mut Tape,
) -> Var {
    let a = visit_point(vars, line.a_visit);
    let b = visit_point(vars, line.b_visit);
    let p = visit_point(vars, point_visit);
    let vx = tape.sub(b.x, a.x);
    let vy = tape.sub(b.y, a.y);
    let wx = tape.sub(p.x, a.x);
    let wy = tape.sub(p.y, a.y);
    let left = tape.mul(vx, wy);
    let right = tape.mul(vy, wx);
    tape.sub(left, right)
}

fn segment_length2_value(segment: &SegmentSpec, vars: &[Var], tape: &Tape) -> f64 {
    let a = visit_point(vars, segment.a_visit);
    let b = visit_point(vars, segment.b_visit);
    let dx = tape.value(b.x) - tape.value(a.x);
    let dy = tape.value(b.y) - tape.value(a.y);
    dx * dx + dy * dy
}

fn skip_segment_pair_for_spacing(a: &SegmentSpec, b: &SegmentSpec, visits: &[VisitSpec]) -> bool {
    same_or_adjacent_segment(a, b)
        || segments_share_stop_id(a, b, visits)
        || same_corridor(a, b, visits)
}

fn same_or_adjacent_segment(a: &SegmentSpec, b: &SegmentSpec) -> bool {
    a.a_visit == b.a_visit
        || a.a_visit == b.b_visit
        || a.b_visit == b.a_visit
        || a.b_visit == b.b_visit
        || (a.route_index == b.route_index && a.segment_index.abs_diff(b.segment_index) <= 1)
}

fn segments_share_stop_id(a: &SegmentSpec, b: &SegmentSpec, visits: &[VisitSpec]) -> bool {
    let a0 = &visits[a.a_visit].stop_id;
    let a1 = &visits[a.b_visit].stop_id;
    let b0 = &visits[b.a_visit].stop_id;
    let b1 = &visits[b.b_visit].stop_id;
    a0 == b0 || a0 == b1 || a1 == b0 || a1 == b1
}

fn same_corridor(a: &SegmentSpec, b: &SegmentSpec, visits: &[VisitSpec]) -> bool {
    let a0 = &visits[a.a_visit].stop_id;
    let a1 = &visits[a.b_visit].stop_id;
    let b0 = &visits[b.a_visit].stop_id;
    let b1 = &visits[b.b_visit].stop_id;
    (a0 == b0 && a1 == b1) || (a0 == b1 && a1 == b0)
}

fn segments_currently_cross(a: &SegmentSpec, b: &SegmentSpec, vars: &[Var], tape: &Tape) -> bool {
    let a0 = point_value_from_vars(vars, a.a_visit, tape);
    let a1 = point_value_from_vars(vars, a.b_visit, tape);
    let b0 = point_value_from_vars(vars, b.a_visit, tape);
    let b1 = point_value_from_vars(vars, b.b_visit, tape);

    let Some((ta, tb)) = segment_intersection_params(a0, a1, b0, b1) else {
        return false;
    };
    let margin = 0.04;
    ta > margin && ta < 1.0 - margin && tb > margin && tb < 1.0 - margin
}

fn point_value_from_vars(vars: &[Var], visit_index: usize, tape: &Tape) -> Point {
    let point = visit_point(vars, visit_index);
    Point {
        x: tape.value(point.x),
        y: tape.value(point.y),
    }
}

fn segment_intersection_params(a0: Point, a1: Point, b0: Point, b1: Point) -> Option<(f64, f64)> {
    let rx = a1.x - a0.x;
    let ry = a1.y - a0.y;
    let sx = b1.x - b0.x;
    let sy = b1.y - b0.y;
    let denom = cross_value(rx, ry, sx, sy);
    if denom.abs() < 1e-8 {
        return None;
    }

    let qpx = b0.x - a0.x;
    let qpy = b0.y - a0.y;
    Some((
        cross_value(qpx, qpy, sx, sy) / denom,
        cross_value(qpx, qpy, rx, ry) / denom,
    ))
}

fn best_transfer_axis(
    group: &StopGroup,
    vars: &[Var],
    tape: &Tape,
    shared_lane_gap: f64,
    hardness: f64,
) -> TransferAxis {
    TransferAxis::ALL
        .into_iter()
        .min_by(|&left, &right| {
            let left_score =
                transfer_axis_error(group, vars, tape, left, shared_lane_gap, hardness);
            let right_score =
                transfer_axis_error(group, vars, tape, right, shared_lane_gap, hardness);
            left_score
                .partial_cmp(&right_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(TransferAxis::Horizontal)
}

fn transfer_axis_error(
    group: &StopGroup,
    vars: &[Var],
    tape: &Tape,
    axis: TransferAxis,
    shared_lane_gap: f64,
    hardness: f64,
) -> f64 {
    let centroid = numeric_group_centroid(group, vars, tape);
    let direction = axis.direction();
    let middle = (group.visits.len() as f64 - 1.0) / 2.0;
    let gap2 = shared_lane_gap.powi(2).max(1.0);
    let mut error = 0.0;

    for (slot, &visit_index) in group.visits.iter().enumerate() {
        let point = point_value_from_vars(vars, visit_index, tape);
        let offset = (slot as f64 - middle) * shared_lane_gap;
        let target = Point {
            x: centroid.x + direction.x * offset,
            y: centroid.y + direction.y * offset,
        };
        let d2 = point_distance2(point, target);
        error += d2 + hardness * d2 * d2 / gap2;
    }

    error
}

fn numeric_group_centroid(group: &StopGroup, vars: &[Var], tape: &Tape) -> Point {
    let mut x = 0.0;
    let mut y = 0.0;
    for &visit_index in &group.visits {
        let point = point_value_from_vars(vars, visit_index, tape);
        x += point.x;
        y += point.y;
    }
    Point {
        x: x / group.visits.len() as f64,
        y: y / group.visits.len() as f64,
    }
}

fn corridor_member_points(
    member: &CorridorMember,
    segments: &[SegmentSpec],
    vars: &[Var],
) -> (AdPoint, AdPoint) {
    let segment = &segments[member.segment_index];
    if member.key_forward {
        (
            visit_point(vars, segment.a_visit),
            visit_point(vars, segment.b_visit),
        )
    } else {
        (
            visit_point(vars, segment.b_visit),
            visit_point(vars, segment.a_visit),
        )
    }
}

fn corridor_centroids(
    group: &CorridorGroup,
    segments: &[SegmentSpec],
    vars: &[Var],
    tape: &mut Tape,
) -> (AdPoint, AdPoint) {
    let mut ax = tape.zero();
    let mut ay = tape.zero();
    let mut bx = tape.zero();
    let mut by = tape.zero();

    for member in &group.members {
        let (point_a, point_b) = corridor_member_points(member, segments, vars);
        ax = tape.add(ax, point_a.x);
        ay = tape.add(ay, point_a.y);
        bx = tape.add(bx, point_b.x);
        by = tape.add(by, point_b.y);
    }

    let scale = 1.0 / group.members.len() as f64;
    (
        AdPoint {
            x: tape.scale(ax, scale),
            y: tape.scale(ay, scale),
        },
        AdPoint {
            x: tape.scale(bx, scale),
            y: tape.scale(by, scale),
        },
    )
}

fn lane_endpoint_cost(
    point: AdPoint,
    center: AdPoint,
    direction: Direction,
    normal: Direction,
    lane_offset: f64,
    tape: &mut Tape,
) -> Var {
    let dx = tape.sub(point.x, center.x);
    let dy = tape.sub(point.y, center.y);
    let offset_x = tape.scale(dx, normal.x);
    let offset_y = tape.scale(dy, normal.y);
    let signed_offset = tape.add(offset_x, offset_y);
    let lane_target = tape.constant(lane_offset);
    let lane_error = tape.sub(signed_offset, lane_target);
    let lane_cost = tape.sqr(lane_error);

    let along_x = tape.scale(dx, direction.x);
    let along_y = tape.scale(dy, direction.y);
    let along = tape.add(along_x, along_y);
    let along2 = tape.sqr(along);
    let along_cost = tape.scale(along2, 0.35);
    tape.add(lane_cost, along_cost)
}

fn signed_lane_gap_cost(
    left: AdPoint,
    right: AdPoint,
    center: AdPoint,
    normal: Direction,
    expected_gap: f64,
    tape: &mut Tape,
) -> Var {
    let left_dx = tape.sub(left.x, center.x);
    let left_dy = tape.sub(left.y, center.y);
    let right_dx = tape.sub(right.x, center.x);
    let right_dy = tape.sub(right.y, center.y);
    let left_offset_x = tape.scale(left_dx, normal.x);
    let left_offset_y = tape.scale(left_dy, normal.y);
    let left_offset = tape.add(left_offset_x, left_offset_y);
    let right_offset_x = tape.scale(right_dx, normal.x);
    let right_offset_y = tape.scale(right_dy, normal.y);
    let right_offset = tape.add(right_offset_x, right_offset_y);
    let gap = tape.sub(right_offset, left_offset);
    let expected_gap = tape.constant(expected_gap);
    let error = tape.sub(gap, expected_gap);
    tape.sqr(error)
}

fn label_box(center: AdPoint, label: &str, params: CostParameters, tape: &mut Tape) -> LabelBox {
    let offset_y = tape.constant(params.label_offset_y);
    let y = tape.add(center.y, offset_y);
    let char_count = label.chars().count() as f64;
    let width = (char_count * params.label_char_width).max(28.0);
    LabelBox {
        center: AdPoint { x: center.x, y },
        half_width: width / 2.0,
        half_height: params.label_height / 2.0,
    }
}

fn box_overlap_cost(a: LabelBox, b: LabelBox, clearance: f64, tape: &mut Tape) -> Option<Var> {
    let min_dx = a.half_width + b.half_width + clearance;
    let min_dy = a.half_height + b.half_height + clearance;
    let x_gap = axis_overlap_gap(a.center.x, b.center.x, min_dx, tape)?;
    let y_gap = axis_overlap_gap(a.center.y, b.center.y, min_dy, tape)?;
    let x_cost = tape.sqr(x_gap);
    let y_cost = tape.sqr(y_gap);
    Some(tape.add(x_cost, y_cost))
}

fn label_point_overlap_cost(
    label: LabelBox,
    point: AdPoint,
    radius: f64,
    tape: &mut Tape,
) -> Option<Var> {
    let min_dx = label.half_width + radius;
    let min_dy = label.half_height + radius;
    let x_gap = axis_overlap_gap(label.center.x, point.x, min_dx, tape)?;
    let y_gap = axis_overlap_gap(label.center.y, point.y, min_dy, tape)?;
    let x_cost = tape.sqr(x_gap);
    let y_cost = tape.sqr(y_gap);
    Some(tape.add(x_cost, y_cost))
}

fn axis_overlap_gap(a: Var, b: Var, min_gap: f64, tape: &mut Tape) -> Option<Var> {
    let delta = tape.sub(a, b);
    let current = tape.value(delta);
    let abs_delta = if current >= 0.0 {
        delta
    } else {
        tape.scale(delta, -1.0)
    };

    if current.abs() >= min_gap {
        return None;
    }

    let limit = tape.constant(min_gap);
    Some(tape.sub(limit, abs_delta))
}

fn outside_interval_cost(value: Var, min: f64, max: f64, tape: &mut Tape) -> Var {
    let current = tape.value(value);
    if current < min {
        let limit = tape.constant(min);
        let delta = tape.sub(limit, value);
        tape.sqr(delta)
    } else if current > max {
        let limit = tape.constant(max);
        let delta = tape.sub(value, limit);
        tape.sqr(delta)
    } else {
        tape.zero()
    }
}

fn route_path_points(route: &OptimizedRoute) -> Vec<Point> {
    if route.stops.is_empty() {
        return Vec::new();
    }

    let mut points = vec![route.stops[0].point];
    for pair in route.stops.windows(2) {
        for point in octilinear_connector(pair[0].point, pair[1].point)
            .into_iter()
            .skip(1)
        {
            push_point(&mut points, point);
        }
    }
    points
}

fn octilinear_connector(a: Point, b: Point) -> Vec<Point> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let tolerance = 2.0;

    if dx.abs() < tolerance || dy.abs() < tolerance || (dx.abs() - dy.abs()).abs() < tolerance {
        return vec![a, b];
    }

    minimum_one_bend_connector(a, b).unwrap_or_else(|| vec![a, b])
}

fn minimum_one_bend_connector(a: Point, b: Point) -> Option<Vec<Point>> {
    let mut best: Option<(Point, f64)> = None;

    for a_dir in OCTILINEAR_DIRECTIONS {
        for b_dir in OCTILINEAR_DIRECTIONS {
            if cross_value(a_dir.x, a_dir.y, b_dir.x, b_dir.y).abs() < 1e-8 {
                continue;
            }

            let Some(mid) = directed_line_intersection(a, a_dir, b, b_dir) else {
                continue;
            };
            if !mid.x.is_finite() || !mid.y.is_finite() || same_point(a, mid) || same_point(b, mid)
            {
                continue;
            }

            let length = point_distance(a, mid) + point_distance(mid, b);
            if best.is_none_or(|(_, best_length)| length < best_length) {
                best = Some((mid, length));
            }
        }
    }

    best.map(|(mid, _)| vec![a, mid, b])
}

fn directed_line_intersection(
    a: Point,
    a_dir: Direction,
    b: Point,
    b_dir: Direction,
) -> Option<Point> {
    let denom = cross_value(a_dir.x, a_dir.y, b_dir.x, b_dir.y);
    if denom.abs() < 1e-8 {
        return None;
    }
    let bx = b.x - a.x;
    let by = b.y - a.y;
    let t = cross_value(bx, by, b_dir.x, b_dir.y) / denom;
    Some(Point {
        x: a.x + a_dir.x * t,
        y: a.y + a_dir.y * t,
    })
}

fn push_point(points: &mut Vec<Point>, point: Point) {
    if !same_point(points.last().copied().unwrap_or(point), point) {
        points.push(point);
    }
}

fn place_labels(map: &OptimizedMap, route_paths: &[Vec<Point>]) -> Vec<PlacedLabel> {
    let mut line_segments = Vec::new();
    for path in route_paths {
        for segment in path.windows(2) {
            if !same_point(segment[0], segment[1]) {
                line_segments.push((segment[0], segment[1]));
            }
        }
    }

    let mut circles = Vec::new();
    for route in &map.routes {
        for stop in &route.stops {
            circles.push(RenderCircle {
                center: stop.point,
                radius: if stop.passthrough { 5.2 } else { 7.2 },
            });
        }
    }

    let mut stop_indices: Vec<usize> = (0..map.stops.len()).collect();
    stop_indices.sort_by(|&left, &right| {
        label_priority(&map.stops[right]).total_cmp(&label_priority(&map.stops[left]))
    });

    let mut placed = Vec::new();
    for stop_index in stop_indices {
        let stop = &map.stops[stop_index];
        let passthrough_only =
            !stop.visits.is_empty() && stop.visits.iter().all(|visit| visit.passthrough);
        let mut best: Option<(RenderLabelBox, f64)> = None;

        for (bbox, preference) in
            label_candidates(stop.optimized, &stop.name, map.optimization.parameters)
        {
            let score = score_label_box(
                bbox,
                stop.optimized,
                map.canvas,
                preference,
                &line_segments,
                &circles,
                &placed,
            );
            if best.is_none_or(|(_, best_score)| score < best_score) {
                best = Some((bbox, score));
            }
        }

        let bbox = best.map(|(bbox, _)| bbox).unwrap_or_else(|| {
            fallback_label_box(stop.optimized, &stop.name, map.optimization.parameters)
        });

        placed.push(PlacedLabel {
            name: stop.name.clone(),
            center: bbox.center,
            passthrough_only,
            bbox,
        });
    }

    placed
}

fn label_priority(stop: &OptimizedStop) -> f64 {
    stop.name.chars().count() as f64 * 4.0 + stop.visits.len() as f64 * 6.0
}

fn label_candidates(
    anchor: Point,
    name: &str,
    params: CostParameters,
) -> Vec<(RenderLabelBox, f64)> {
    let (half_width, half_height) = render_label_half_size(name, params);
    let mut candidates = Vec::new();
    candidates.push((
        RenderLabelBox {
            center: Point {
                x: anchor.x,
                y: anchor.y + params.label_offset_y,
            },
            half_width,
            half_height,
        },
        0.0,
    ));

    let circle_radius = 8.0;
    let gaps = [4.0, 12.0, 22.0, 36.0, 54.0, 74.0];
    let directions = [
        (0.0, -1.0),
        (1.0, 0.0),
        (0.0, 1.0),
        (-1.0, 0.0),
        (1.0, -1.0),
        (-1.0, -1.0),
        (1.0, 1.0),
        (-1.0, 1.0),
    ];

    for (gap_index, gap) in gaps.into_iter().enumerate() {
        for (direction_index, (dx, dy)) in directions.into_iter().enumerate() {
            let x_offset = if dx == 0.0 {
                0.0
            } else {
                dx * (circle_radius + gap + half_width)
            };
            let y_offset = if dy == 0.0 {
                0.0
            } else {
                dy * (circle_radius + gap + half_height)
            };
            candidates.push((
                RenderLabelBox {
                    center: Point {
                        x: anchor.x + x_offset,
                        y: anchor.y + y_offset,
                    },
                    half_width,
                    half_height,
                },
                gap_index as f64 * 22.0 + direction_index as f64 * 2.5,
            ));
        }
    }

    candidates
}

fn fallback_label_box(anchor: Point, name: &str, params: CostParameters) -> RenderLabelBox {
    let (half_width, half_height) = render_label_half_size(name, params);
    RenderLabelBox {
        center: Point {
            x: anchor.x,
            y: anchor.y + params.label_offset_y,
        },
        half_width,
        half_height,
    }
}

fn render_label_half_size(name: &str, params: CostParameters) -> (f64, f64) {
    let char_count = name.chars().count() as f64;
    let width = (char_count * params.label_char_width).max(28.0);
    (width / 2.0, params.label_height / 2.0)
}

fn score_label_box(
    bbox: RenderLabelBox,
    anchor: Point,
    canvas: Canvas,
    preference: f64,
    line_segments: &[(Point, Point)],
    circles: &[RenderCircle],
    placed: &[PlacedLabel],
) -> f64 {
    let mut score = preference + point_distance2(anchor, bbox.center) * 0.018;
    if let Some(amount) = outside_canvas_amount(bbox, canvas) {
        score += 240_000.0 + amount * amount * 90.0;
    }
    for circle in circles {
        if let Some(amount) = box_circle_overlap_amount(bbox, *circle, 2.5) {
            score += 220_000.0 + amount * amount * 70.0;
        }
    }
    for &(a, b) in line_segments {
        if segment_intersects_box(a, b, bbox, 3.0) {
            score += 190_000.0;
        } else if segment_intersects_box(a, b, bbox, 7.0) {
            score += 22_000.0;
        }
    }
    for label in placed {
        if let Some(amount) = box_box_overlap_amount(bbox, label.bbox, 3.0) {
            score += 260_000.0 + amount * 75.0;
        }
    }
    score
}

fn outside_canvas_amount(bbox: RenderLabelBox, canvas: Canvas) -> Option<f64> {
    let min_x = bbox.center.x - bbox.half_width;
    let max_x = bbox.center.x + bbox.half_width;
    let min_y = bbox.center.y - bbox.half_height;
    let max_y = bbox.center.y + bbox.half_height;
    let mut amount = 0.0_f64;
    if min_x < 2.0 {
        amount += 2.0 - min_x;
    }
    if max_x > canvas.width - 2.0 {
        amount += max_x - (canvas.width - 2.0);
    }
    if min_y < 2.0 {
        amount += 2.0 - min_y;
    }
    if max_y > canvas.height - 2.0 {
        amount += max_y - (canvas.height - 2.0);
    }
    (amount > 0.0).then_some(amount)
}

fn box_circle_overlap_amount(
    bbox: RenderLabelBox,
    circle: RenderCircle,
    clearance: f64,
) -> Option<f64> {
    let outside_x = ((circle.center.x - bbox.center.x).abs() - bbox.half_width).max(0.0);
    let outside_y = ((circle.center.y - bbox.center.y).abs() - bbox.half_height).max(0.0);
    let radius = circle.radius + clearance;
    let gap = radius * radius - outside_x * outside_x - outside_y * outside_y;
    (gap > 0.0).then_some(gap.sqrt())
}

fn box_box_overlap_amount(a: RenderLabelBox, b: RenderLabelBox, clearance: f64) -> Option<f64> {
    let x_overlap = a.half_width + b.half_width + clearance - (a.center.x - b.center.x).abs();
    let y_overlap = a.half_height + b.half_height + clearance - (a.center.y - b.center.y).abs();
    if x_overlap > 0.0 && y_overlap > 0.0 {
        Some(x_overlap * y_overlap)
    } else {
        None
    }
}

fn segment_intersects_box(a: Point, b: Point, bbox: RenderLabelBox, clearance: f64) -> bool {
    let min_x = bbox.center.x - bbox.half_width - clearance;
    let max_x = bbox.center.x + bbox.half_width + clearance;
    let min_y = bbox.center.y - bbox.half_height - clearance;
    let max_y = bbox.center.y + bbox.half_height + clearance;
    if point_inside_rect(a, min_x, max_x, min_y, max_y)
        || point_inside_rect(b, min_x, max_x, min_y, max_y)
    {
        return true;
    }
    let top_left = Point { x: min_x, y: min_y };
    let top_right = Point { x: max_x, y: min_y };
    let bottom_right = Point { x: max_x, y: max_y };
    let bottom_left = Point { x: min_x, y: max_y };
    segments_intersect_numeric(a, b, top_left, top_right)
        || segments_intersect_numeric(a, b, top_right, bottom_right)
        || segments_intersect_numeric(a, b, bottom_right, bottom_left)
        || segments_intersect_numeric(a, b, bottom_left, top_left)
}

fn point_inside_rect(point: Point, min_x: f64, max_x: f64, min_y: f64, max_y: f64) -> bool {
    point.x >= min_x && point.x <= max_x && point.y >= min_y && point.y <= max_y
}

fn segments_intersect_numeric(a: Point, b: Point, c: Point, d: Point) -> bool {
    let o1 = orientation_value(a, b, c);
    let o2 = orientation_value(a, b, d);
    let o3 = orientation_value(c, d, a);
    let o4 = orientation_value(c, d, b);
    let eps = 1e-8;
    if o1.abs() < eps && point_on_segment_numeric(c, a, b) {
        return true;
    }
    if o2.abs() < eps && point_on_segment_numeric(d, a, b) {
        return true;
    }
    if o3.abs() < eps && point_on_segment_numeric(a, c, d) {
        return true;
    }
    if o4.abs() < eps && point_on_segment_numeric(b, c, d) {
        return true;
    }
    (o1 > 0.0) != (o2 > 0.0) && (o3 > 0.0) != (o4 > 0.0)
}

fn orientation_value(a: Point, b: Point, c: Point) -> f64 {
    cross_value(b.x - a.x, b.y - a.y, c.x - a.x, c.y - a.y)
}

fn point_on_segment_numeric(point: Point, a: Point, b: Point) -> bool {
    let eps = 1e-8;
    point.x >= a.x.min(b.x) - eps
        && point.x <= a.x.max(b.x) + eps
        && point.y >= a.y.min(b.y) - eps
        && point.y <= a.y.max(b.y) + eps
}

fn svg_polyline(points: &[Point], color: &str, width: f64) -> String {
    let pts: Vec<String> = points
        .iter()
        .map(|point| format!("{:.2},{:.2}", point.x, point.y))
        .collect();
    format!(
        r#"  <polyline points="{}" fill="none" stroke="{}" stroke-width="{:.1}" stroke-linecap="round" stroke-linejoin="round"/>"#,
        pts.join(" "),
        color,
        width
    )
}

fn svg_stop_circle(point: Point, passthrough: bool, color: &str) -> String {
    if passthrough {
        format!(
            r#"  <circle cx="{:.2}" cy="{:.2}" r="3.8" fill="{}" fill-opacity="0.42" stroke="{}" stroke-width="1.4" stroke-opacity="0.75"/>"#,
            point.x, point.y, color, color
        )
    } else {
        format!(
            r#"  <circle cx="{:.2}" cy="{:.2}" r="5.6" fill="white" stroke="{}" stroke-width="2.4"/>"#,
            point.x, point.y, color
        )
    }
}

fn svg_label(point: Point, name: &str, passthrough_only: bool) -> String {
    let opacity = if passthrough_only { "0.55" } else { "1.0" };
    let weight = if passthrough_only { "normal" } else { "bold" };
    format!(
        r##"  <text x="{:.2}" y="{:.2}" text-anchor="middle" dominant-baseline="middle" font-size="11" font-weight="{}" fill="#222222" fill-opacity="{}" stroke="#f8f9fa" stroke-width="3" stroke-linejoin="round" paint-order="stroke">{}</text>"##,
        point.x,
        point.y,
        weight,
        opacity,
        escape_xml(name)
    )
}

fn ordered_corridor_key(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

fn cross_value(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    ax * by - ay * bx
}

fn point_distance(a: Point, b: Point) -> f64 {
    point_distance2(a, b).sqrt()
}

fn point_distance2(a: Point, b: Point) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

fn lerp_point(a: Point, b: Point, t: f64) -> Point {
    Point {
        x: a.x * (1.0 - t) + b.x * t,
        y: a.y * (1.0 - t) + b.y * t,
    }
}

fn same_point(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < 0.01 && (a.y - b.y).abs() < 0.01
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
