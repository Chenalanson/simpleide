//! Small, dependency-free Mermaid renderer for the framework panel and chat.
//!
//! The skill writes Mermaid flowcharts and sequence diagrams into
//! `framework.md`. Rendering the source block as text makes the architecture
//! panel much harder to read, so this module parses the subset used by the
//! skill and paints it with egui. Unsupported diagrams fall back to the normal
//! code block in `ai_ui`.

use std::collections::{HashMap, VecDeque};

use eframe::egui::{
    self, Align2, CornerRadius, FontId, Id, Pos2, Rect, RichText, ScrollArea, Sense, Shape, Stroke,
    StrokeKind, Ui, Vec2,
};

use crate::theme;

#[derive(Clone, Debug)]
pub enum Diagram {
    Flow(FlowChart),
    Sequence(SequenceDiagram),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlowDirection {
    #[default]
    TopDown,
    LeftRight,
}

#[derive(Clone, Debug, Default)]
pub struct FlowChart {
    pub direction: FlowDirection,
    pub nodes: Vec<FlowNode>,
    pub edges: Vec<FlowEdge>,
}

#[derive(Clone, Debug, Default)]
pub struct FlowNode {
    pub id: String,
    pub label: String,
    pub shape: NodeShape,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NodeShape {
    #[default]
    Rect,
    Round,
    Stadium,
    Diamond,
    Circle,
    Asymmetric,
}

#[derive(Clone, Debug, Default)]
pub struct FlowEdge {
    pub from: String,
    pub to: String,
    pub label: String,
    pub dotted: bool,
    pub thick: bool,
    pub cross: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SequenceDiagram {
    pub participants: Vec<SeqParticipant>,
    pub events: Vec<SeqEvent>,
}

#[derive(Clone, Debug, Default)]
pub struct SeqParticipant {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Default)]
pub enum SeqEvent {
    #[default]
    Empty,
    Message {
        from: String,
        to: String,
        text: String,
        dotted: bool,
        cross: bool,
    },
    Note {
        over: Vec<String>,
        text: String,
    },
    Label {
        text: String,
    },
}

pub fn render(ui: &mut Ui, source: &str, id: Id) -> bool {
    match parse(source) {
        Some(Diagram::Flow(flow)) if !flow.nodes.is_empty() => {
            render_flow(ui, &flow, id);
            true
        }
        Some(Diagram::Sequence(sequence)) if !sequence.participants.is_empty() => {
            render_sequence(ui, &sequence, id);
            true
        }
        _ => false,
    }
}

pub fn parse(source: &str) -> Option<Diagram> {
    let lines = source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let first = lines.first()?.trim_start_matches('\u{feff}');
    let lower = first.to_ascii_lowercase();
    if lower.starts_with("flowchart") || lower.starts_with("graph") {
        return parse_flow(first, &lines[1..]).map(Diagram::Flow);
    }
    if lower.starts_with("sequencediagram") {
        return parse_sequence(&lines[1..]).map(Diagram::Sequence);
    }
    None
}

// ---------------------------------------------------------------------------
// Mermaid source parsing
// ---------------------------------------------------------------------------

#[derive(Default)]
struct FlowParser {
    direction: FlowDirection,
    nodes: Vec<FlowNode>,
    index: HashMap<String, usize>,
    edges: Vec<FlowEdge>,
}

impl FlowParser {
    fn add_node(&mut self, id: &str, label: &str, shape: NodeShape) {
        if id.is_empty() {
            return;
        }
        if let Some(&index) = self.index.get(id) {
            let node = &mut self.nodes[index];
            if !label.is_empty() && (node.label.is_empty() || node.label == node.id) {
                node.label = label.to_owned();
            }
            if shape != NodeShape::Rect {
                node.shape = shape;
            }
            return;
        }
        let index = self.nodes.len();
        self.index.insert(id.to_owned(), index);
        self.nodes.push(FlowNode {
            id: id.to_owned(),
            label: if label.is_empty() {
                id.to_owned()
            } else {
                label.to_owned()
            },
            shape,
        });
    }

    fn add_edge(&mut self, from: String, to: String, label: String, op: &EdgeOp) {
        if from.is_empty() || to.is_empty() {
            return;
        }
        self.edges.push(FlowEdge {
            from,
            to,
            label,
            dotted: op.dotted,
            thick: op.thick,
            cross: op.cross,
        });
    }
}

fn parse_flow(header: &str, lines: &[&str]) -> Option<FlowChart> {
    let direction = header
        .split_whitespace()
        .nth(1)
        .map(|value| match value.to_ascii_uppercase().as_str() {
            "LR" | "RL" => FlowDirection::LeftRight,
            _ => FlowDirection::TopDown,
        })
        .unwrap_or_default();
    let mut parser = FlowParser {
        direction,
        ..FlowParser::default()
    };
    for line in lines {
        let line = line.trim().trim_end_matches(';').trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("classdef")
            || lower.starts_with("class ")
            || lower.starts_with("style ")
            || lower.starts_with("linkstyle")
            || lower.starts_with("click ")
            || lower == "end"
            || lower.starts_with("subgraph")
            || lower.starts_with("direction ")
            || lower.starts_with("accTitle")
            || lower.starts_with("accDescr")
        {
            continue;
        }
        parse_flow_statement(line, &mut parser);
    }
    if parser.nodes.is_empty() {
        return None;
    }
    Some(FlowChart {
        direction: parser.direction,
        nodes: parser.nodes,
        edges: parser.edges,
    })
}

fn parse_flow_statement(line: &str, parser: &mut FlowParser) {
    let Some((mut current, consumed)) = parse_node_token(line) else {
        return;
    };
    parser.add_node(&current.id, &current.label, current.shape);
    let mut rest = &line[consumed..];
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let Some((op, consumed)) = parse_edge_op(rest) else {
            break;
        };
        rest = rest[consumed..].trim_start();
        let mut label = op.label.clone().unwrap_or_default();
        if let Some(after) = rest.strip_prefix('|') {
            if let Some(end) = after.find('|') {
                label = clean_label(&after[..end]);
                rest = after[end + 1..].trim_start();
            }
        }
        let Some((next, consumed)) = parse_node_token(rest) else {
            break;
        };
        parser.add_node(&next.id, &next.label, next.shape);
        parser.add_edge(current.id.clone(), next.id.clone(), label, &op);
        current = next;
        rest = &rest[consumed..];
    }
}

fn parse_node_token(source: &str) -> Option<(FlowNode, usize)> {
    let source = source.trim_start();
    let mut id_end = 0usize;
    for (index, ch) in source.char_indices() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':') {
            id_end = index + ch.len_utf8();
        } else {
            break;
        }
    }
    if id_end == 0 {
        return None;
    }
    let id = source[..id_end].to_owned();
    let after_id = &source[id_end..];
    let trimmed = after_id.trim_start();
    let shape_start = id_end + (after_id.len() - trimmed.len());
    if let Some((shape, label, consumed)) = parse_shape(trimmed) {
        return Some((FlowNode { id, label, shape }, shape_start + consumed));
    }
    Some((
        FlowNode {
            label: id.clone(),
            id,
            shape: NodeShape::Rect,
        },
        id_end,
    ))
}

fn parse_shape(source: &str) -> Option<(NodeShape, String, usize)> {
    let quoted = |rest: &str, open: &str, close: &str, shape: NodeShape| {
        let mut consumed = open.len();
        let body = &rest[open.len()..];
        if let Some(end) = body.find(close) {
            let label = clean_label(&body[..end]);
            consumed += end + close.len();
            Some((shape, label, consumed))
        } else {
            None
        }
    };
    if source.starts_with("[\"") {
        return quoted(source, "[\"", "\"]", NodeShape::Rect);
    }
    if source.starts_with("(\"") {
        return quoted(source, "(\"", "\")", NodeShape::Round);
    }
    if source.starts_with("{\"") {
        return quoted(source, "{\"", "\"}", NodeShape::Diamond);
    }
    if source.starts_with("[(") {
        return quoted(source, "[(", ")]", NodeShape::Stadium);
    }
    if source.starts_with("((") {
        return quoted(source, "((", "))", NodeShape::Circle);
    }
    if source.starts_with('>') {
        return quoted(source, ">", "]", NodeShape::Asymmetric);
    }
    if source.starts_with('[') {
        return quoted(source, "[", "]", NodeShape::Rect);
    }
    if source.starts_with('(') {
        return quoted(source, "(", ")", NodeShape::Round);
    }
    if source.starts_with('{') {
        return quoted(source, "{", "}", NodeShape::Diamond);
    }
    None
}

#[derive(Clone, Debug, Default)]
struct EdgeOp {
    dotted: bool,
    thick: bool,
    cross: bool,
    label: Option<String>,
}

fn parse_edge_op(source: &str) -> Option<(EdgeOp, usize)> {
    let source = source.trim_start();
    let simple = |dotted: bool, thick: bool, cross: bool, len: usize| {
        Some((
            EdgeOp {
                dotted,
                thick,
                cross,
                label: None,
            },
            len,
        ))
    };
    if source.starts_with("-.->") {
        return simple(true, false, false, 4);
    }
    if source.starts_with("-.-x") {
        return simple(true, false, true, 4);
    }
    if source.starts_with(".-x") {
        return simple(true, false, true, 3);
    }
    if source.starts_with("-.-") {
        return simple(true, false, false, 3);
    }
    if source.starts_with("<-->") {
        return simple(false, false, false, 4);
    }
    if source.starts_with("-->") {
        return simple(false, false, false, 3);
    }
    if source.starts_with("---") {
        return simple(false, false, false, 3);
    }
    if source.starts_with("==>") {
        return simple(false, true, false, 3);
    }
    if source.starts_with("===") {
        return simple(false, true, false, 3);
    }
    if source.starts_with("--x") {
        return simple(false, false, true, 3);
    }
    if source.starts_with("--o") {
        return simple(false, false, false, 3);
    }
    if source.starts_with("--") {
        for (suffix, dotted, thick, cross) in [
            ("-->", false, false, false),
            ("---", false, false, false),
            ("--x", false, false, true),
            ("--o", false, false, false),
        ] {
            if let Some(end) = source.find(suffix) {
                if end > 2 {
                    return Some((
                        EdgeOp {
                            dotted,
                            thick,
                            cross,
                            label: Some(clean_label(&source[2..end])),
                        },
                        end + suffix.len(),
                    ));
                }
            }
        }
    }
    if source.starts_with("==") {
        if let Some(end) = source.find("==>") {
            if end > 2 {
                return Some((
                    EdgeOp {
                        dotted: false,
                        thick: true,
                        cross: false,
                        label: Some(clean_label(&source[2..end])),
                    },
                    end + 3,
                ));
            }
        }
    }
    if source.starts_with("-.") {
        for (suffix, cross) in [(".->", false), (".-x", true), (".-", false)] {
            if let Some(end) = source.find(suffix) {
                if end >= 2 {
                    return Some((
                        EdgeOp {
                            dotted: true,
                            thick: false,
                            cross,
                            label: Some(clean_label(&source[2..end])),
                        },
                        end + suffix.len(),
                    ));
                }
            }
        }
    }
    None
}

fn parse_sequence(lines: &[&str]) -> Option<SequenceDiagram> {
    let mut participants = Vec::new();
    let mut index = HashMap::new();
    let mut events = Vec::new();
    for line in lines {
        let line = line.trim().trim_end_matches(';').trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("participant ") || lower.starts_with("actor ") {
            let rest = line
                .split_once(' ')
                .map(|(_, rest)| rest.trim())
                .unwrap_or("");
            let (id, label) = rest
                .split_once(" as ")
                .map(|(id, label)| (id.trim(), label.trim()))
                .unwrap_or((rest, rest));
            add_participant(&mut participants, &mut index, id, label);
            continue;
        }
        if lower.starts_with("note ") {
            if let Some((before, text)) = line.split_once(':') {
                let text = clean_label(text);
                let lower_before = before.to_ascii_lowercase();
                let over = if let Some(pos) = lower_before.find(" over ") {
                    before[pos + 6..]
                        .split(',')
                        .map(|name| name.trim().to_owned())
                        .filter(|name| !name.is_empty())
                        .collect::<Vec<_>>()
                } else if let Some(pos) = lower_before.find(" of ") {
                    before[pos + 4..]
                        .split(',')
                        .map(|name| name.trim().to_owned())
                        .filter(|name| !name.is_empty())
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                for name in &over {
                    add_participant(&mut participants, &mut index, name, name);
                }
                events.push(SeqEvent::Note { over, text });
            }
            continue;
        }
        if let Some(rest) = lower.strip_prefix("alt ") {
            events.push(SeqEvent::Label {
                text: format!("alt: {rest}"),
            });
            continue;
        }
        if let Some(rest) = lower.strip_prefix("else ") {
            events.push(SeqEvent::Label {
                text: format!("else: {rest}"),
            });
            continue;
        }
        if let Some(rest) = lower.strip_prefix("loop ") {
            events.push(SeqEvent::Label {
                text: format!("loop: {rest}"),
            });
            continue;
        }
        if let Some(rest) = lower.strip_prefix("opt ") {
            events.push(SeqEvent::Label {
                text: format!("opt: {rest}"),
            });
            continue;
        }
        if let Some(rest) = lower.strip_prefix("par ") {
            events.push(SeqEvent::Label {
                text: format!("par: {rest}"),
            });
            continue;
        }
        if lower == "end" {
            events.push(SeqEvent::Label {
                text: "end".to_owned(),
            });
            continue;
        }
        if lower.starts_with("activate ")
            || lower.starts_with("deactivate ")
            || lower.starts_with("autonumber")
            || lower.starts_with("title ")
        {
            continue;
        }
        let Some((arrow_start, arrow_len, dotted, cross)) = find_sequence_arrow(line) else {
            continue;
        };
        let after_arrow = &line[arrow_start + arrow_len..];
        let (to, text) = after_arrow.split_once(':').unwrap_or((after_arrow, ""));
        let from = line[..arrow_start].trim();
        let to = to.trim();
        if from.is_empty() || to.is_empty() {
            continue;
        }
        add_participant(&mut participants, &mut index, from, from);
        add_participant(&mut participants, &mut index, to, to);
        events.push(SeqEvent::Message {
            from: from.to_owned(),
            to: to.to_owned(),
            text: clean_label(text),
            dotted,
            cross,
        });
    }
    if participants.is_empty() {
        return None;
    }
    Some(SequenceDiagram {
        participants,
        events,
    })
}

fn add_participant(
    participants: &mut Vec<SeqParticipant>,
    index: &mut HashMap<String, usize>,
    id: &str,
    label: &str,
) {
    if id.is_empty() || index.contains_key(id) {
        return;
    }
    let index_value = participants.len();
    index.insert(id.to_owned(), index_value);
    participants.push(SeqParticipant {
        id: id.to_owned(),
        label: clean_label(label),
    });
}

fn find_sequence_arrow(source: &str) -> Option<(usize, usize, bool, bool)> {
    let patterns = [
        ("-->>", false, false),
        ("->>", false, false),
        ("--x", true, true),
        ("-->", true, false),
        ("-x", false, true),
        ("->", false, false),
        ("--)", true, false),
        ("-)", false, false),
    ];
    let mut best: Option<(usize, usize, bool, bool)> = None;
    for (pattern, dotted, cross) in patterns {
        if let Some(start) = source.find(pattern) {
            let candidate = (start, pattern.len(), dotted, cross);
            if best.is_none_or(|current| {
                candidate.0 < current.0 || (candidate.0 == current.0 && candidate.1 > current.1)
            }) {
                best = Some(candidate);
            }
        }
    }
    best
}

fn clean_label(raw: &str) -> String {
    let mut text = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<br>", "\n")
        .replace("**", "");
    let mut output = String::new();
    let mut in_tag = false;
    for ch in text.drain(..) {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    output
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .trim()
        .to_owned()
}

fn wrap_label(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for raw in text.split('\n') {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let mut current = String::new();
        let mut count = 0usize;
        for ch in raw.chars() {
            if count >= max_chars {
                lines.push(current.trim_end().to_owned());
                current.clear();
                count = 0;
            }
            current.push(ch);
            count += 1;
        }
        if !current.trim().is_empty() {
            lines.push(current.trim_end().to_owned());
        }
    }
    if lines.is_empty() {
        lines.push(text.trim().to_owned());
    }
    if lines.len() > 8 {
        lines.truncate(8);
        if let Some(last) = lines.last_mut() {
            last.push('…');
        }
    }
    lines
}

/// Approximate rendered width: CJK glyphs are about one font-size wide, ASCII
/// about 0.58. This keeps node boxes from clipping Chinese labels in the narrow
/// architecture panel.
fn text_width(text: &str, font_size: f32) -> f32 {
    text.chars()
        .map(|ch| {
            if ch.is_ascii() {
                font_size * 0.58
            } else {
                font_size
            }
        })
        .sum()
}

// ---------------------------------------------------------------------------
// Flowchart layout and painting
// ---------------------------------------------------------------------------

struct FlowNodeLayout {
    rect: Rect,
    lines: Vec<String>,
    shape: NodeShape,
}

struct FlowEdgeLayout {
    points: Vec<Pos2>,
    label: String,
    dotted: bool,
    thick: bool,
    cross: bool,
}

fn render_flow(ui: &mut Ui, flow: &FlowChart, id: Id) {
    let (nodes, edges, size) = layout_flow(flow);
    let available = ui.available_width();
    if size.x <= available + 4.0 {
        paint_flow(ui, &nodes, &edges, size);
    } else {
        ScrollArea::horizontal()
            .id_salt(("mermaid_flow", id))
            .scroll_source(egui::scroll_area::ScrollSource::SCROLL_BAR)
            .auto_shrink([false, true])
            .show(ui, |ui| paint_flow(ui, &nodes, &edges, size));
        ui.label(
            RichText::new("横向拖动滚动条查看完整图")
                .size(8.0)
                .color(theme::text_faint()),
        );
    }
    ui.add_space(4.0);
}

fn paint_flow(ui: &mut Ui, nodes: &[FlowNodeLayout], edges: &[FlowEdgeLayout], size: Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        CornerRadius::same(8),
        theme::with_alpha(theme::editor_bg(), 180),
    );
    for edge in edges {
        draw_flow_edge(painter, edge, rect.min);
    }
    for node in nodes {
        draw_flow_node(painter, node, rect.min);
    }
}

fn layout_flow(flow: &FlowChart) -> (Vec<FlowNodeLayout>, Vec<FlowEdgeLayout>, Vec2) {
    let count = flow.nodes.len();
    let mut index = HashMap::new();
    for (position, node) in flow.nodes.iter().enumerate() {
        index.insert(node.id.as_str(), position);
    }
    let mut adjacency = vec![Vec::new(); count];
    let mut indegree = vec![0usize; count];
    for edge in &flow.edges {
        if let (Some(&from), Some(&to)) =
            (index.get(edge.from.as_str()), index.get(edge.to.as_str()))
        {
            if from != to {
                adjacency[from].push(to);
                indegree[to] += 1;
            }
        }
    }
    let mut layer = vec![0usize; count];
    let mut queue = (0..count)
        .filter(|&node| indegree[node] == 0)
        .collect::<VecDeque<_>>();
    let mut seen = vec![false; count];
    while let Some(node) = queue.pop_front() {
        if seen[node] {
            continue;
        }
        seen[node] = true;
        for &next in &adjacency[node] {
            layer[next] = layer[next].max(layer[node] + 1);
            indegree[next] -= 1;
            if indegree[next] == 0 {
                queue.push_back(next);
            }
        }
    }
    // Cycles cannot be layered by Kahn; assign them from known predecessors
    // and stop after a bounded number of passes.
    for _ in 0..count {
        let mut changed = false;
        for node in 0..count {
            if seen[node] {
                continue;
            }
            let mut best = layer[node];
            for edge in &flow.edges {
                if edge.to == flow.nodes[node].id {
                    if let Some(&from) = index.get(edge.from.as_str()) {
                        if seen[from] {
                            best = best.max(layer[from] + 1);
                        }
                    }
                }
            }
            if best != layer[node] {
                layer[node] = best;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let max_layer = layer.iter().copied().max().unwrap_or(0);
    let mut by_layer = vec![Vec::new(); max_layer + 1];
    for (node, &value) in layer.iter().enumerate() {
        by_layer[value].push(node);
    }

    let mut sizes = Vec::with_capacity(count);
    for node in &flow.nodes {
        let lines = wrap_label(&node.label, 10);
        let max_width = lines
            .iter()
            .map(|line| text_width(line, 8.5))
            .fold(0.0_f32, f32::max);
        let width = (max_width + 16.0).clamp(70.0, 200.0);
        let height = (lines.len() as f32 * 13.0 + 14.0).clamp(28.0, 130.0);
        sizes.push((width, height, lines));
    }

    let pad = 12.0;
    let gap_x = 14.0;
    let gap_y = 30.0;
    let mut rects = vec![Rect::ZERO; count];
    let total = if flow.direction == FlowDirection::TopDown {
        let layer_widths = by_layer
            .iter()
            .map(|nodes| {
                nodes.iter().map(|&node| sizes[node].0).sum::<f32>()
                    + gap_x * nodes.len().saturating_sub(1) as f32
            })
            .collect::<Vec<_>>();
        let layer_heights = by_layer
            .iter()
            .map(|nodes| {
                nodes
                    .iter()
                    .map(|&node| sizes[node].1)
                    .fold(0.0_f32, f32::max)
            })
            .collect::<Vec<_>>();
        let max_width = layer_widths.iter().copied().fold(0.0_f32, f32::max);
        let mut y = pad;
        for (layer_index, nodes) in by_layer.iter().enumerate() {
            let layer_width = layer_widths[layer_index];
            let layer_height = layer_heights[layer_index];
            let mut x = pad + (max_width - layer_width) / 2.0;
            for &node in nodes {
                let (width, height, _) = &sizes[node];
                rects[node] = Rect::from_min_size(
                    egui::pos2(x, y + (layer_height - height) / 2.0),
                    egui::vec2(*width, *height),
                );
                x += width + gap_x;
            }
            y += layer_height + gap_y;
        }
        egui::vec2(max_width + pad * 2.0, y - gap_y + pad)
    } else {
        let layer_heights = by_layer
            .iter()
            .map(|nodes| {
                nodes.iter().map(|&node| sizes[node].1).sum::<f32>()
                    + gap_y * nodes.len().saturating_sub(1) as f32
            })
            .collect::<Vec<_>>();
        let layer_widths = by_layer
            .iter()
            .map(|nodes| {
                nodes
                    .iter()
                    .map(|&node| sizes[node].0)
                    .fold(0.0_f32, f32::max)
            })
            .collect::<Vec<_>>();
        let max_height = layer_heights.iter().copied().fold(0.0_f32, f32::max);
        let mut x = pad;
        for (layer_index, nodes) in by_layer.iter().enumerate() {
            let layer_width = layer_widths[layer_index];
            let layer_height = layer_heights[layer_index];
            let mut y = pad + (max_height - layer_height) / 2.0;
            for &node in nodes {
                let (width, height, _) = &sizes[node];
                rects[node] = Rect::from_min_size(
                    egui::pos2(x + (layer_width - width) / 2.0, y),
                    egui::vec2(*width, *height),
                );
                y += height + gap_y;
            }
            x += layer_width + 46.0;
        }
        egui::vec2(x - 46.0 + pad, max_height + pad * 2.0)
    };

    let mut edge_layouts = Vec::new();
    for edge in &flow.edges {
        let (Some(&from), Some(&to)) = (index.get(edge.from.as_str()), index.get(edge.to.as_str()))
        else {
            continue;
        };
        let from_rect = rects[from];
        let to_rect = rects[to];
        let points = if flow.direction == FlowDirection::TopDown {
            if to_rect.top() > from_rect.bottom() + 4.0 {
                let start = egui::pos2(from_rect.center().x, from_rect.bottom());
                let end = egui::pos2(to_rect.center().x, to_rect.top());
                let mid = (start.y + end.y) / 2.0;
                vec![start, egui::pos2(start.x, mid), egui::pos2(end.x, mid), end]
            } else {
                let bend = from_rect.right().max(to_rect.right()) + 18.0;
                vec![
                    egui::pos2(from_rect.right(), from_rect.center().y),
                    egui::pos2(bend, from_rect.center().y),
                    egui::pos2(bend, to_rect.center().y),
                    egui::pos2(to_rect.right(), to_rect.center().y),
                ]
            }
        } else if to_rect.left() > from_rect.right() + 4.0 {
            let start = egui::pos2(from_rect.right(), from_rect.center().y);
            let end = egui::pos2(to_rect.left(), to_rect.center().y);
            let mid = (start.x + end.x) / 2.0;
            vec![start, egui::pos2(mid, start.y), egui::pos2(mid, end.y), end]
        } else {
            let bend = from_rect.bottom().max(to_rect.bottom()) + 18.0;
            vec![
                egui::pos2(from_rect.center().x, from_rect.bottom()),
                egui::pos2(from_rect.center().x, bend),
                egui::pos2(to_rect.center().x, bend),
                egui::pos2(to_rect.center().x, to_rect.bottom()),
            ]
        };
        edge_layouts.push(FlowEdgeLayout {
            points,
            label: edge.label.clone(),
            dotted: edge.dotted,
            thick: edge.thick,
            cross: edge.cross,
        });
    }
    let nodes = flow
        .nodes
        .iter()
        .enumerate()
        .map(|(position, _)| FlowNodeLayout {
            rect: rects[position],
            lines: sizes[position].2.clone(),
            shape: flow.nodes[position].shape,
        })
        .collect::<Vec<_>>();
    (nodes, edge_layouts, total)
}

fn draw_flow_node(painter: &egui::Painter, node: &FlowNodeLayout, offset: Pos2) {
    let rect = node.rect.translate(offset.to_vec2());
    let fill = theme::with_alpha(theme::surface(), 235);
    let stroke = Stroke::new(1.1_f32, theme::accent());
    match node.shape {
        NodeShape::Diamond => {
            let center = rect.center();
            let points = vec![
                egui::pos2(center.x, rect.top()),
                egui::pos2(rect.right(), center.y),
                egui::pos2(center.x, rect.bottom()),
                egui::pos2(rect.left(), center.y),
            ];
            painter.add(Shape::convex_polygon(points, fill, stroke));
        }
        NodeShape::Circle => {
            let radius = rect.width().min(rect.height()) / 2.0;
            painter.circle_filled(rect.center(), radius, fill);
            painter.circle_stroke(rect.center(), radius, stroke);
        }
        NodeShape::Asymmetric => {
            let points = vec![
                rect.left_top(),
                egui::pos2(rect.right() - 8.0, rect.top()),
                egui::pos2(rect.right(), rect.center().y),
                egui::pos2(rect.right() - 8.0, rect.bottom()),
                rect.left_bottom(),
            ];
            painter.add(Shape::convex_polygon(points, fill, stroke));
        }
        NodeShape::Round | NodeShape::Stadium => {
            let radius = rect.height() / 2.0;
            painter.rect_filled(rect, CornerRadius::same(radius as u8), fill);
            painter.rect_stroke(
                rect,
                CornerRadius::same(radius as u8),
                stroke,
                StrokeKind::Inside,
            );
        }
        NodeShape::Rect => {
            painter.rect_filled(rect, CornerRadius::same(6), fill);
            painter.rect_stroke(rect, CornerRadius::same(6), stroke, StrokeKind::Inside);
        }
    }
    let font = FontId::proportional(8.5);
    let line_height = 12.5;
    let start_y = rect.center().y - (node.lines.len() as f32 - 1.0) * line_height / 2.0;
    for (line_index, line) in node.lines.iter().enumerate() {
        painter.text(
            egui::pos2(rect.center().x, start_y + line_index as f32 * line_height),
            Align2::CENTER_CENTER,
            line,
            font.clone(),
            theme::text(),
        );
    }
}

fn draw_flow_edge(painter: &egui::Painter, edge: &FlowEdgeLayout, offset: Pos2) {
    let points = edge
        .points
        .iter()
        .map(|point| *point + offset.to_vec2())
        .collect::<Vec<_>>();
    if points.len() < 2 {
        return;
    }
    let color = theme::text_faint();
    let stroke = Stroke::new(if edge.thick { 2.0 } else { 1.1_f32 }, color);
    for pair in points.windows(2) {
        if edge.dotted {
            draw_dashed_line(painter, pair[0], pair[1], stroke);
        } else {
            painter.line_segment([pair[0], pair[1]], stroke);
        }
    }
    let tip = *points.last().unwrap();
    let previous = points[points.len() - 2];
    if edge.cross {
        let direction = (tip - previous).normalized();
        let perpendicular = egui::vec2(-direction.y, direction.x);
        painter.line_segment(
            [tip - perpendicular * 5.0, tip + perpendicular * 5.0],
            stroke,
        );
        painter.line_segment(
            [
                tip - direction * 5.0 + perpendicular * 5.0,
                tip + direction * 5.0 - perpendicular * 5.0,
            ],
            stroke,
        );
    } else {
        draw_arrow_head(painter, tip, previous, stroke);
    }
    if !edge.label.is_empty() {
        let lines = wrap_label(&edge.label, 18);
        let mid = points[1] + (points[2] - points[1]) * 0.5;
        let width = lines
            .iter()
            .map(|line| text_width(line, 7.5))
            .fold(0.0_f32, f32::max)
            + 8.0;
        let height = lines.len() as f32 * 11.0 + 4.0;
        let rect = Rect::from_center_size(mid, egui::vec2(width, height));
        painter.rect_filled(rect, CornerRadius::same(3), theme::panel());
        painter.rect_stroke(
            rect,
            CornerRadius::same(3),
            Stroke::new(0.7_f32, theme::border_soft()),
            StrokeKind::Inside,
        );
        for (line_index, line) in lines.iter().enumerate() {
            painter.text(
                egui::pos2(mid.x, rect.top() + 2.0 + line_index as f32 * 11.0 + 5.5),
                Align2::CENTER_CENTER,
                line,
                FontId::proportional(7.5),
                theme::text_dim(),
            );
        }
    }
}

fn draw_arrow_head(painter: &egui::Painter, tip: Pos2, previous: Pos2, stroke: Stroke) {
    let direction = (tip - previous).normalized();
    let perpendicular = egui::vec2(-direction.y, direction.x);
    let base = tip - direction * 8.0;
    painter.line_segment([tip, base + perpendicular * 4.0], stroke);
    painter.line_segment([tip, base - perpendicular * 4.0], stroke);
}

fn draw_dashed_line(painter: &egui::Painter, from: Pos2, to: Pos2, stroke: Stroke) {
    let distance = from.distance(to);
    if distance <= 0.5 {
        return;
    }
    let direction = (to - from) / distance;
    let mut position = 0.0;
    while position < distance {
        let end = (position + 4.0).min(distance);
        painter.line_segment(
            [from + direction * position, from + direction * end],
            stroke,
        );
        position += 7.0;
    }
}

// ---------------------------------------------------------------------------
// Sequence diagram layout and painting
// ---------------------------------------------------------------------------

struct SeqParticipantLayout {
    rect: Rect,
    center_x: f32,
    lines: Vec<String>,
}

enum SeqItem {
    Message {
        from_x: f32,
        to_x: f32,
        top: f32,
        arrow_y: f32,
        lines: Vec<String>,
        dotted: bool,
        cross: bool,
    },
    Note {
        left: f32,
        right: f32,
        top: f32,
        height: f32,
        lines: Vec<String>,
    },
    Label {
        top: f32,
        text: String,
    },
}

fn render_sequence(ui: &mut Ui, sequence: &SequenceDiagram, id: Id) {
    let available = ui.available_width();
    let (participants, items, size) = layout_sequence(sequence, available);
    ScrollArea::horizontal()
        .id_salt(("mermaid_sequence", id))
        .scroll_source(egui::scroll_area::ScrollSource::SCROLL_BAR)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(
                rect,
                CornerRadius::same(8),
                theme::with_alpha(theme::editor_bg(), 180),
            );
            let line_color = theme::border_soft();
            let origin = rect.min;
            for participant in &participants {
                let participant_rect = participant.rect.translate(origin.to_vec2());
                painter.line_segment(
                    [
                        egui::pos2(participant.center_x + origin.x, participant_rect.bottom()),
                        egui::pos2(participant.center_x + origin.x, rect.bottom() - 8.0),
                    ],
                    Stroke::new(0.8_f32, line_color),
                );
            }
            for item in &items {
                match item {
                    SeqItem::Label { top, text } => {
                        painter.text(
                            egui::pos2(rect.left() + 12.0, rect.top() + *top),
                            Align2::LEFT_TOP,
                            text,
                            FontId::proportional(9.0),
                            theme::warning(),
                        );
                    }
                    SeqItem::Message {
                        from_x,
                        to_x,
                        top,
                        arrow_y,
                        lines,
                        dotted,
                        cross,
                    } => {
                        draw_message_text(painter, rect.left() + 8.0, rect.top() + *top, lines);
                        let from = egui::pos2(rect.left() + *from_x, rect.top() + *arrow_y);
                        let to = egui::pos2(rect.left() + *to_x, rect.top() + *arrow_y);
                        let stroke = Stroke::new(1.2_f32, theme::text_faint());
                        if *dotted {
                            draw_dashed_line(painter, from, to, stroke);
                        } else {
                            painter.line_segment([from, to], stroke);
                        }
                        if *cross {
                            let perpendicular = egui::vec2(0.0, 4.0);
                            painter.line_segment([to - perpendicular, to + perpendicular], stroke);
                            painter.line_segment(
                                [to + egui::vec2(-4.0, 0.0), to + egui::vec2(4.0, 0.0)],
                                stroke,
                            );
                        } else {
                            let direction = if (to.x - from.x).abs() < 0.5 {
                                egui::vec2(1.0, 0.0)
                            } else {
                                egui::vec2((to.x - from.x).signum(), 0.0)
                            };
                            let base = to - direction * 8.0;
                            painter.line_segment([to, base + egui::vec2(0.0, 4.0)], stroke);
                            painter.line_segment([to, base - egui::vec2(0.0, 4.0)], stroke);
                        }
                    }
                    SeqItem::Note {
                        left,
                        right,
                        top,
                        height,
                        lines,
                    } => {
                        let note = Rect::from_min_size(
                            egui::pos2(rect.left() + *left, rect.top() + *top),
                            egui::vec2((right - left).max(120.0), *height),
                        );
                        painter.rect_filled(
                            note,
                            CornerRadius::same(5),
                            theme::with_alpha(theme::info(), 26),
                        );
                        painter.rect_stroke(
                            note,
                            CornerRadius::same(5),
                            Stroke::new(0.9_f32, theme::with_alpha(theme::info(), 150)),
                            StrokeKind::Inside,
                        );
                        for (index, line) in lines.iter().enumerate() {
                            painter.text(
                                egui::pos2(
                                    note.center().x,
                                    note.top() + 4.0 + index as f32 * 12.0 + 6.0,
                                ),
                                Align2::CENTER_CENTER,
                                line,
                                FontId::proportional(8.5),
                                theme::text(),
                            );
                        }
                    }
                }
            }
            for participant in &participants {
                let participant_rect = participant.rect.translate(origin.to_vec2());
                painter.rect_filled(
                    participant_rect,
                    CornerRadius::same(5),
                    theme::with_alpha(theme::accent(), 28),
                );
                painter.rect_stroke(
                    participant_rect,
                    CornerRadius::same(5),
                    Stroke::new(1.0_f32, theme::with_alpha(theme::accent(), 170)),
                    StrokeKind::Inside,
                );
                for (index, line) in participant.lines.iter().enumerate() {
                    painter.text(
                        egui::pos2(
                            participant_rect.center().x,
                            participant_rect.top() + 4.0 + index as f32 * 12.0 + 6.0,
                        ),
                        Align2::CENTER_CENTER,
                        line,
                        FontId::proportional(8.5),
                        theme::text(),
                    );
                }
            }
        });
    if size.x > available + 4.0 {
        ui.label(
            RichText::new("横向拖动滚动条查看完整图")
                .size(8.0)
                .color(theme::text_faint()),
        );
    }
    ui.add_space(4.0);
}

fn layout_sequence(
    sequence: &SequenceDiagram,
    available: f32,
) -> (Vec<SeqParticipantLayout>, Vec<SeqItem>, Vec2) {
    let count = sequence.participants.len().max(1);
    let gap = 10.0;
    let pad = 10.0;
    let mut widths = Vec::new();
    for participant in &sequence.participants {
        let natural = text_width(&participant.label, 8.5) + 20.0;
        widths.push(natural.clamp(70.0, 160.0));
    }
    let sum_widths = widths.iter().sum::<f32>();
    let usable = (available - pad * 2.0 - gap * (count - 1) as f32).max(40.0);
    let scale = (usable / sum_widths.max(1.0)).min(1.0);
    for width in &mut widths {
        *width = (*width * scale).max(34.0);
    }
    let mut labels = Vec::new();
    for (index, participant) in sequence.participants.iter().enumerate() {
        let max_chars = ((widths[index] - 8.0) / 5.2).max(3.0) as usize;
        let header = if participant.label.chars().count() > 8 {
            participant.id.clone()
        } else {
            participant.label.clone()
        };
        labels.push(wrap_label(&header, max_chars));
    }
    let mut x = pad;
    let mut center = HashMap::new();
    let mut layouts = Vec::new();
    for (index, participant) in sequence.participants.iter().enumerate() {
        let width = widths[index];
        let height = (labels[index].len() as f32 * 11.0 + 10.0).clamp(24.0, 80.0);
        let rect = Rect::from_min_size(egui::pos2(x, 8.0), egui::vec2(width, height));
        center.insert(participant.id.as_str(), rect.center().x);
        layouts.push(SeqParticipantLayout {
            rect,
            center_x: rect.center().x,
            lines: labels[index].clone(),
        });
        x += width + gap;
    }
    let total_width = x - gap + pad;
    let max_header = labels
        .iter()
        .map(|lines| (lines.len() as f32 * 11.0 + 10.0).clamp(24.0, 80.0))
        .fold(0.0_f32, f32::max);
    let mut top = 8.0 + max_header + 18.0;
    let mut items = Vec::new();
    for event in &sequence.events {
        match event {
            SeqEvent::Message {
                from,
                to,
                text,
                dotted,
                cross,
            } => {
                let from_label = sequence
                    .participants
                    .iter()
                    .find(|participant| participant.id == *from)
                    .map(|participant| participant.label.clone())
                    .unwrap_or_else(|| from.clone());
                let to_label = sequence
                    .participants
                    .iter()
                    .find(|participant| participant.id == *to)
                    .map(|participant| participant.label.clone())
                    .unwrap_or_else(|| to.clone());
                let combined = if text.trim().is_empty() {
                    format!("{from_label} → {to_label}")
                } else {
                    format!("{from_label} → {to_label}：{text}")
                };
                let lines = wrap_label(&combined, 40);
                let height = lines.len() as f32 * 11.0 + 14.0;
                let from_x = center.get(from.as_str()).copied().unwrap_or(pad);
                let to_x = center.get(to.as_str()).copied().unwrap_or(pad);
                items.push(SeqItem::Message {
                    from_x,
                    to_x,
                    top,
                    arrow_y: top + lines.len() as f32 * 11.0 + 5.0,
                    lines,
                    dotted: *dotted,
                    cross: *cross,
                });
                top += height;
            }
            SeqEvent::Note { over, text } => {
                let lines = wrap_label(text, 34);
                let height = lines.len() as f32 * 12.0 + 14.0;
                let mut xs = over
                    .iter()
                    .filter_map(|name| center.get(name.as_str()).copied())
                    .collect::<Vec<_>>();
                if xs.is_empty() {
                    xs.push(pad + 60.0);
                }
                let left = xs.iter().copied().fold(f32::INFINITY, f32::min) - 36.0;
                let right = xs.iter().copied().fold(0.0_f32, f32::max) + 36.0;
                items.push(SeqItem::Note {
                    left,
                    right,
                    top,
                    height,
                    lines,
                });
                top += height + 8.0;
            }
            SeqEvent::Label { text } => {
                items.push(SeqItem::Label {
                    top,
                    text: text.clone(),
                });
                top += 18.0;
            }
            SeqEvent::Empty => {}
        }
    }
    let size = egui::vec2(total_width, (top + 16.0).max(120.0));
    (layouts, items, size)
}

fn draw_message_text(painter: &egui::Painter, left: f32, top: f32, lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    let width = lines
        .iter()
        .map(|line| text_width(line, 8.0))
        .fold(0.0_f32, f32::max)
        + 10.0;
    let height = lines.len() as f32 * 11.0 + 4.0;
    let rect = Rect::from_min_size(egui::pos2(left, top), egui::vec2(width, height));
    painter.rect_filled(rect, CornerRadius::same(3), theme::panel());
    for (index, line) in lines.iter().enumerate() {
        painter.text(
            egui::pos2(left + 4.0, top + 2.0 + index as f32 * 11.0),
            Align2::LEFT_TOP,
            line,
            FontId::proportional(8.0),
            theme::text(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flowchart_nodes_and_edges() {
        let source = r#"flowchart TD
    APP["APP/app.c（活跃）<br/>app_init · app_loop_thread"]
    LED["BSP/MODULES/bsp_led.c"]
    PER["BSP/PERIPHERAL/bsp_usart_echo.c"]
    APP --> LED
    APP -. "运行期无调用" .-> PER
    PER -. "禁止反向依赖" .-x APP
"#;
        let Some(Diagram::Flow(flow)) = parse(source) else {
            panic!("flowchart should parse");
        };
        assert_eq!(flow.nodes.len(), 3);
        assert!(flow.nodes[0].label.contains("app_init"));
        assert_eq!(flow.edges.len(), 3);
        assert!(flow.edges[1].dotted);
        assert_eq!(flow.edges[1].label, "运行期无调用");
        assert!(flow.edges[2].cross);
    }

    #[test]
    fn parses_sequence_messages_notes_and_labels() {
        let source = r#"sequenceDiagram
    participant M as main()
    participant H as HAL
    M->>H: HAL_Init()
    H-->>M: ready
    M->>H: "A:B"
    Note over M: startup
    alt error
        H->>M: fail
    else ok
        H-->>M: ok
    end
"#;
        let Some(Diagram::Sequence(sequence)) = parse(source) else {
            panic!("sequence should parse");
        };
        assert_eq!(sequence.participants.len(), 2);
        assert!(sequence.events.iter().any(|event| matches!(
            event,
            SeqEvent::Message { text, .. } if text == "HAL_Init()"
        )));
        assert!(sequence.events.iter().any(|event| matches!(
            event,
            SeqEvent::Message { text, .. } if text == "A:B"
        )));
        assert!(sequence
            .events
            .iter()
            .any(|event| matches!(event, SeqEvent::Note { .. })));
        assert!(sequence
            .events
            .iter()
            .any(|event| matches!(event, SeqEvent::Label { text } if text.starts_with("alt:"))));
    }

    #[test]
    fn parses_the_architecture_flowchart() {
        let source = r##"flowchart TD
    APP["APP/app.c（活跃）<br/>app_init · app_sched_thread · app_loop_thread"]
    MID_S["MID/service_cmd.c（已编译未接入）<br/>service_cmd_init · service_cmd_poll"]
    MID_T["MID/task_sched.c（活跃·空表）<br/>task_sched_register · task_sched_poll"]
    MID_E["MID/err_code.h<br/>err_code_t"]
    MOD_LED["BSP/MODULES/bsp_led.c（活跃）<br/>bsp_led_init/on/off/blink_start/poll"]
    PER_U["BSP/PERIPHERAL/bsp_usart_echo.c（已编译未接入）<br/>init/poll/write/set_rx_handler"]
    CORE_U["Core/Src/usart.c<br/>MX_USART1_UART_Init · MspInit · usart1_idle_dispatch"]
    CORE_IT["Core/Src/stm32f1xx_it.c<br/>USART1_IRQHandler · DMA1_CH4/CH5_IRQHandler"]
    HAL["HAL / CMSIS 库"]
    REG["硬件寄存器<br/>USART1 · DMA1 · GPIOA/C · SysTick"]

    APP --> MID_T
    APP --> MOD_LED
    APP -. "编译期存在，运行期无调用" .-> MID_S
    APP -. "编译期存在，运行期无调用" .-> PER_U
    MID_S -. "接口在位·链路未激活" .-> MOD_LED
    MID_S -. "接口在位·链路未激活" .-> PER_U
    MID_S --> MID_E
    MID_T --> MID_E
    PER_U --> CORE_U
    CORE_IT --> CORE_U
    CORE_IT --> HAL
    CORE_U --> HAL
    MOD_LED --> HAL
    PER_U --> HAL
    HAL --> REG
    CORE_IT -. "虚线：禁止的反向依赖（当前不存在）" .-x MID_S
"##;
        let Some(Diagram::Flow(flow)) = parse(source) else {
            panic!("flowchart should parse");
        };
        assert_eq!(flow.nodes.len(), 10);
        assert_eq!(flow.edges.len(), 16);
        let (nodes, edges, size) = layout_flow(&flow);
        assert_eq!(nodes.len(), 10);
        assert_eq!(edges.len(), 16);
        assert!(size.x > 200.0);
        assert!(size.y > 200.0);
        assert!(nodes.iter().all(|node| node.rect.left() >= 0.0));
    }

    #[test]
    fn wraps_long_labels_without_losing_lines() {
        let lines = wrap_label("第一行\n第二行很长很长很长很长很长很长很长很长很长", 8);
        assert!(lines.len() >= 3);
        assert_eq!(lines[0], "第一行");
    }
}
