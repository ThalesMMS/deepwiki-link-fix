use std::collections::{HashMap, HashSet};

use once_cell::sync::Lazy;
use regex::Regex;

static MERMAID_LIST_ITEM_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*(?:\d+\.\s+|[-*+]\s+)").unwrap());
static MERMAID_BR_SPLIT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"<br\s*/?>").unwrap());
static MERMAID_MD_LINK_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[[^\]]+\]\([^)]+\)").unwrap());
static MERMAID_URL_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"https?://\S+").unwrap());
static MERMAID_PARTICIPANT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^\s*(?P<keyword>participant|actor)\s+(?P<id>.+?)(?:\s+(?P<as>as)\s+(?P<display>.+))?\s*$",
    )
    .unwrap()
});
static MERMAID_BROKEN_CONTENT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(\w+)\[broken-content\]\"\]"#).unwrap());
static MERMAID_EDGE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"^(?P<indent>\s*)(?P<src>[A-Za-z0-9_]+)\s*(?P<arrow>[-.=]+>)\s*(?:\|(?:"(?P<quoted_label>[^"]*)"|(?P<unquoted_label>[^|"\s][^|]*))\|\s*)?(?P<dst>[A-Za-z0-9_]+)\s*$"#,
    )
    .unwrap()
});
static MERMAID_NODE_LABEL_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"\["(.*?)"\]"#).unwrap());

const BRANCH_LABELS: &[&str] = &["yes", "no", "true", "false"];
const POSITIVE_HINTS: &[&str] = &[
    "add",
    "use",
    "enable",
    "create",
    "remove",
    "success",
    "ready",
    "connected",
    "established",
    "proceed",
    "continue",
];
const NEGATIVE_HINTS: &[&str] = &[
    "fail", "error", "invalid", "reject", "timeout", "blocked", "missing", "not", "false", "empty",
    "return", "skip", "default",
];

#[derive(Clone)]
struct Edge {
    line_idx: usize,
    indent: String,
    src: String,
    arrow: String,
    label: Option<String>,
    dst: String,
}

#[derive(Debug)]
struct ParticipantDecl {
    keyword: String,
    id: String,
    display: Option<String>,
}

fn parse_participant(line: &str) -> Option<ParticipantDecl> {
    let caps = MERMAID_PARTICIPANT_RE.captures(line)?;
    Some(ParticipantDecl {
        keyword: caps.name("keyword")?.as_str().to_string(),
        id: caps.name("id")?.as_str().trim().to_string(),
        display: caps.name("display").map(|m| m.as_str().trim().to_string()),
    })
}

/// Determines whether the given label contains a Markdown-style list item in any HTML `<br/>`-separated segment.
///
/// The label is split on HTML `<br...>` breaks and each trimmed segment is inspected for common list-item prefixes
/// (e.g., numbered lists like `1.`, or bullet markers `-`, `*`, `+`).
///
/// # Returns
///
/// `true` if any `<br/>`-separated segment begins with a Markdown list-item marker, `false` otherwise.
///
/// # Examples
///
/// ```
/// assert!(contains_list_marker("Line one<br/>- item two"));
/// assert!(contains_list_marker("First<br/>1. Second"));
/// assert!(!contains_list_marker("Just a single line without list markers"));
/// ```
fn contains_list_marker(label: &str) -> bool {
    MERMAID_BR_SPLIT_RE
        .split(label)
        .any(|part| MERMAID_LIST_ITEM_RE.is_match(part.trim()))
}

/// Sanitizes a Mermaid node label by replacing unsupported Markdown constructs with placeholders.
///
/// If the label contains a list marker (e.g., `- item`, `1. item`, or HTML `<br/>` separated list segments),
/// the function returns exactly `"Unsupported markdown: list"`. Otherwise, any Markdown links (`[text](url)`)
/// or raw `http`/`https` URLs inside the label are replaced with `"Unsupported markdown: link"`.
///
/// # Examples
///
/// ```
/// let s = sanitize_label("See [docs](https://example.com) for details");
/// assert_eq!(s, "Unsupported markdown: link");
///
/// let s2 = sanitize_label("- item 1<br/>- item 2");
/// assert_eq!(s2, "Unsupported markdown: list");
///
/// let s3 = sanitize_label("Plain label");
/// assert_eq!(s3, "Plain label");
/// ```
fn sanitize_label(label: &str) -> String {
    if contains_list_marker(label) {
        return "Unsupported markdown: list".to_string();
    }
    let result = MERMAID_MD_LINK_RE.replace_all(label, "Unsupported markdown: link");
    let result = MERMAID_URL_RE.replace_all(&result, "Unsupported markdown: link");
    result.to_string()
}

/// Rewrites sequenceDiagram participant declarations with spaces or hyphens into safe aliases and updates actor references.
///
/// When a `participant <name>` contains a space or `-`, an underscore-based alias is created; the declaration is rewritten as
/// `participant <alias> as <original>`, and occurrences of the original name in actor-head positions (the part of a line before `:` that denotes actors/interactions) are replaced with the alias.
///
/// # Examples
///
/// ```
/// let lines = vec![
///     "participant Alice Smith".to_string(),
///     "Alice Smith -> Bob: hello".to_string(),
///     "Note over Alice Smith: note".to_string(),
/// ];
/// let out = fix_sequence_diagram_participants(&lines);
/// assert_eq!(out[0], "  participant Alice_Smith as Alice Smith");
/// assert_eq!(out[1], "Alice_Smith -> Bob: hello");
/// assert_eq!(out[2], "Note over Alice_Smith: note");
/// ```
fn fix_sequence_diagram_participants(lines: &[String]) -> Vec<String> {
    let mut participant_aliases: HashMap<String, String> = HashMap::new();
    let mut result: Vec<String> = Vec::new();

    // First pass: identify participants with spaces and create aliases
    for line in lines {
        if let Some(decl) = parse_participant(line) {
            if decl.id.contains("@{") {
                continue;
            }
            if decl.id.contains(' ') || decl.id.contains('-') {
                // Create safe alias
                let safe_alias = decl.id.replace(' ', "_").replace('-', "_");
                participant_aliases.insert(decl.id, safe_alias);
            }
        }
    }

    // Second pass: replace in all lines
    for line in lines {
        let mut new_line = line.clone();

        // Replace participant declarations
        if let Some(decl) = parse_participant(&new_line) {
            if decl.id.contains("@{") {
                result.push(new_line);
                continue;
            }
            if let Some(safe_alias) = participant_aliases.get(&decl.id) {
                new_line = match decl.display {
                    Some(display) => {
                        format!("  {} {} as {}", decl.keyword, safe_alias, display)
                    }
                    None => format!("  {} {} as {}", decl.keyword, safe_alias, decl.id),
                };
            }
        } else {
            new_line = replace_sequence_actor_tokens(&new_line, &participant_aliases);
        }

        result.push(new_line);
    }

    result
}

/// Replace occurrences of participant names in `text` with their corresponding safe aliases.
///
/// Longer original names are replaced before shorter ones to avoid partial matches (e.g., replace
/// "Alice Bob" before "Alice").
///
/// # Parameters
///
/// - `text`: input string in which participant names should be replaced.
/// - `participant_aliases`: mapping from original participant name to its safe alias.
///
/// # Returns
///
/// A new `String` with all occurrences of each original participant name replaced by its alias.
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
///
/// let mut aliases = HashMap::new();
/// aliases.insert("Alice Bob".to_string(), "Alice_Bob".to_string());
/// aliases.insert("Alice".to_string(), "AliceX".to_string());
///
/// let src = "Alice Bob->Alice: hello";
/// let out = replace_aliases(src, &aliases);
/// assert_eq!(out, "Alice_Bob->AliceX: hello");
/// ```
fn replace_aliases(text: &str, participant_aliases: &HashMap<String, String>) -> String {
    let mut replacements: Vec<(&String, &String)> = participant_aliases.iter().collect();
    replacements.sort_by(|(left, _), (right, _)| right.len().cmp(&left.len()));

    let mut result = text.to_string();
    for (original, safe) in replacements {
        result = result.replace(original, safe);
    }
    result
}

/// Determines whether the given leading substring looks like a Mermaid sequence-diagram actor or interaction head.
///
/// Returns `true` when `head` (after trimming leading whitespace) contains common sequence diagram arrow or termination tokens
/// or begins with sequence-specific directives such as `Note over`, `Note right of`, `Note left of`, `activate`, `deactivate`, or `destroy`.
///
/// # Examples
///
/// ```
/// assert!(is_sequence_actor_head("Alice -> Bob: hello"));
/// assert!(is_sequence_actor_head("  Note over Alice: reminder"));
/// assert!(!is_sequence_actor_head("Some general text: not an actor"));
/// ```
fn is_sequence_actor_head(head: &str) -> bool {
    let trimmed = head.trim_start();
    trimmed.contains("->")
        || trimmed.contains("-)")
        || trimmed.contains("-x")
        || trimmed.starts_with("Note over ")
        || trimmed.starts_with("Note right of ")
        || trimmed.starts_with("Note left of ")
        || trimmed.starts_with("activate ")
        || trimmed.starts_with("create ")
        || trimmed.starts_with("deactivate ")
        || trimmed.starts_with("destroy ")
}

/// Replace participant names with their aliases in the actor portion of a sequence-diagram line.
///
/// The function examines the substring before the first `:` (the "head") and, only if that
/// head looks like a sequence-diagram actor/interaction (as determined by `is_sequence_actor_head`),
/// replaces occurrences of original participant names with their aliases from `participant_aliases`.
/// The remainder of the line after the first `:` is preserved unchanged. If the head does not look
/// like a sequence actor, the original line is returned unchanged.
///
/// # Parameters
///
/// - `line`: The sequence-diagram line to transform.
/// - `participant_aliases`: A map from original participant names to their alias replacements.
///
/// # Returns
///
/// The input line with participant names replaced by aliases in the head portion only.
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
///
/// let mut aliases = HashMap::new();
/// aliases.insert("Alice Bob".to_string(), "Alice_Bob".to_string());
///
/// let line = "Alice Bob->Server: request";
/// let result = replace_sequence_actor_tokens(line, &aliases);
/// assert_eq!(result, "Alice_Bob->Server: request");
/// ```
fn replace_sequence_actor_tokens(
    line: &str,
    participant_aliases: &HashMap<String, String>,
) -> String {
    let (head, tail) = line.split_once(':').unwrap_or((line, ""));
    if !is_sequence_actor_head(head) {
        return line.to_string();
    }

    let replaced_head = replace_aliases(head, participant_aliases);
    if line.contains(':') {
        format!("{}:{}", replaced_head, tail)
    } else {
        replaced_head
    }
}

/// Rewrites specific malformed node fragments produced by broken DeepWiki exports into valid Mermaid node labels.
///
/// Scans each input line and replaces occurrences matching the pattern `(\w+)[broken-content]"\]"`
/// with a node declaration that uses the captured identifier as both node id and label, e.g.:
/// `NODE["NODE"]`.
///
/// # Examples
///
/// ```
/// let lines = vec![
///     r#"INPUTENC[broken-content]"]"#.to_string(),
///     r#"normal line"#.to_string(),
/// ];
/// let fixed = fix_malformed_nodes(&lines);
/// assert_eq!(fixed[0], r#"INPUTENC["INPUTENC"]"#);
/// assert_eq!(fixed[1], "normal line");
/// ```
fn fix_malformed_nodes(lines: &[String]) -> Vec<String> {
    // Fix patterns like INPUTENC[broken-content]"] - specific to broken DeepWiki exports
    lines
        .iter()
        .map(|line| {
            MERMAID_BROKEN_CONTENT_RE
                .replace_all(line, |caps: &regex::Captures| {
                    let node_id = &caps[1];
                    // Replace with node ID as the label
                    format!(r#"{}["{}"]"#, node_id, node_id)
                })
                .to_string()
        })
        .collect()
}

/// Sanitizes Mermaid node labels in the provided lines by replacing each `["..."]` label's inner text with the sanitized form.
///
/// Each occurrence matching the Mermaid node label pattern `["..."]` will be rewritten as `["<sanitized>"]` where `<sanitized>` is produced by `sanitize_label`.
///
/// # Examples
///
/// ```
/// let lines = vec![
///     "A[\"clickable link: http://example.com\"]".to_string(),
///     "B[\"- list item\"]".to_string(),
///     "C[\"normal label\"]".to_string(),
/// ];
/// let out = sanitize_node_labels(&lines);
/// assert_eq!(out[0], "A[\"Unsupported markdown: link\"]");
/// assert_eq!(out[1], "B[\"Unsupported markdown: list\"]");
/// assert_eq!(out[2], "C[\"normal label\"]");
/// ```
fn sanitize_node_labels(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            MERMAID_NODE_LABEL_RE
                .replace_all(line, |caps: &regex::Captures| {
                    let label = &caps[1];
                    format!("[\"{}\"]", sanitize_label(label))
                })
                .to_string()
        })
        .collect()
}

/// Determines whether an optional edge label represents a branch token (`yes`, `no`, `true`, or `false`).
///
/// # Parameters
///
/// - `label`: Optional label string to test; leading/trailing whitespace and case are ignored.
///
/// # Returns
///
/// `true` if `label` is present and equals `yes`, `no`, `true`, or `false` (case-insensitive), `false` otherwise.
///
/// # Examples
///
/// ```
/// assert!(is_branch_label(&Some(" yes ".to_string())));
/// assert!(!is_branch_label(&Some("maybe".to_string())));
/// assert!(!is_branch_label(&None));
/// ```
fn is_branch_label(label: &Option<String>) -> bool {
    match label {
        None => false,
        Some(l) => BRANCH_LABELS.contains(&l.trim().to_lowercase().as_str()),
    }
}

/// Classifies a branch label string as positive, negative, or unknown.
///
/// Matches the trimmed, case-insensitive label against known boolean-like tokens.
///
/// # Returns
///
/// `Some("positive")` for labels `"yes"` or `"true"`, `Some("negative")` for labels `"no"` or `"false"`, `None` otherwise.
///
/// # Examples
///
/// ```
/// assert_eq!(label_polarity("yes"), Some("positive"));
/// assert_eq!(label_polarity(" False "), Some("negative"));
/// assert_eq!(label_polarity("maybe"), None);
/// ```
fn label_polarity(label: &str) -> Option<&'static str> {
    let lowered = label.trim().to_lowercase();
    if lowered == "yes" || lowered == "true" {
        Some("positive")
    } else if lowered == "no" || lowered == "false" {
        Some("negative")
    } else {
        None
    }
}

/// Computes a score for `text` based on the provided `polarity` using
/// the module's positive and negative hint lists.
///
/// The score is the count of matching hints for the requested polarity
/// minus the count of matching hints for the opposite polarity. If
/// `polarity` is `None`, the score is `0`.
///
/// # Parameters
///
/// - `polarity`: expected polarity to score for; accepted values are
///   `"positive"` or any other string treated as the opposite polarity.
///   Use `None` to indicate no polarity preference.
///
/// # Returns
///
/// An `i32` where a positive value means `text` contains more hints
/// aligned with `polarity`, a negative value means it contains more
/// hints aligned with the opposite polarity, and zero means neutral
/// or `polarity` was `None`.
///
/// # Examples
///
/// ```
/// // positive hint ("success") present -> positive score
/// assert!(edge_score("Deployment success", Some("positive")) > 0);
///
/// // negative hint ("fail") present -> negative score when polarity is positive
/// assert!(edge_score("Deployment fail", Some("positive")) < 0);
///
/// // no polarity -> zero
/// assert_eq!(edge_score("anything", None), 0);
/// ```
fn edge_score(text: &str, polarity: Option<&str>) -> i32 {
    match polarity {
        None => 0,
        Some(pol) => {
            let text_lower = text.to_lowercase();
            let pos_hits: i32 = POSITIVE_HINTS
                .iter()
                .filter(|hint| text_lower.contains(*hint))
                .count() as i32;
            let neg_hits: i32 = NEGATIVE_HINTS
                .iter()
                .filter(|hint| text_lower.contains(*hint))
                .count() as i32;
            if pol == "positive" {
                pos_hits - neg_hits
            } else {
                neg_hits - pos_hits
            }
        }
    }
}

/// Selects a single candidate edge index to receive a moved branch label, or `None` if no
/// unambiguous choice can be made.
///
/// The function applies these rules:
/// - If `candidates` is empty, returns `None`. If it contains exactly one index, returns it.
/// - If `polarity` is `None`, prefers the sole candidate whose edge has no label; returns that
///   index only when exactly one unlabeled candidate exists, otherwise `None`.
/// - If `polarity` is `Some("positive")` or `Some("negative")`, scores each candidate by
///   evaluating the target node's label (from `node_labels` or the target id) with `edge_score`.
///   If the maximum score is greater than zero and unique, returns the corresponding index.
///   If the maximum score is not positive or not unique, falls back to the unlabeled-single rule
///   above and returns the index only when exactly one unlabeled candidate exists.
///
/// # Returns
///
/// `Some(index)` when a single candidate is selected according to the rules above, `None` otherwise.
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
///
/// // Minimal Edge construction matching the module's Edge layout
/// let edges = vec![
///     super::Edge { line_idx: 0, indent: String::new(), src: "A".into(), arrow: "-->".into(), label: None, dst: "B".into() },
///     super::Edge { line_idx: 1, indent: String::new(), src: "A".into(), arrow: "-->".into(), label: Some("yes".into()), dst: "C".into() },
///     super::Edge { line_idx: 2, indent: String::new(), src: "B".into(), arrow: "-->".into(), label: None, dst: "D".into() },
/// ];
///
/// let mut node_labels = HashMap::new();
/// node_labels.insert("D".into(), "success".into());
///
/// // Choose among candidates 0 and 2 with positive polarity.
/// let choice = super::choose_edge(&edges, &node_labels, &[0, 2], Some("positive"));
/// // Depending on scoring, a unique positive winner may be returned; otherwise None.
/// let _ = choice;
/// ```
fn choose_edge(
    edges: &[Edge],
    node_labels: &HashMap<String, String>,
    candidates: &[usize],
    polarity: Option<&str>,
) -> Option<usize> {
    if candidates.is_empty() {
        return None;
    }
    if candidates.len() == 1 {
        return Some(candidates[0]);
    }
    if polarity.is_none() {
        let unlabeled: Vec<usize> = candidates
            .iter()
            .copied()
            .filter(|&idx| edges[idx].label.is_none())
            .collect();
        return if unlabeled.len() == 1 {
            Some(unlabeled[0])
        } else {
            None
        };
    }

    let scored: Vec<(i32, usize)> = candidates
        .iter()
        .map(|&idx| {
            let target_label = node_labels
                .get(&edges[idx].dst)
                .cloned()
                .unwrap_or_else(|| edges[idx].dst.clone());
            (edge_score(&target_label, polarity), idx)
        })
        .collect();

    let max_score = scored.iter().map(|(s, _)| *s).max().unwrap_or(0);
    if max_score <= 0 {
        let unlabeled: Vec<usize> = candidates
            .iter()
            .copied()
            .filter(|&idx| edges[idx].label.is_none())
            .collect();
        return if unlabeled.len() == 1 {
            Some(unlabeled[0])
        } else {
            None
        };
    }

    let winners: Vec<usize> = scored
        .iter()
        .filter(|(s, _)| *s == max_score)
        .map(|(_, idx)| *idx)
        .collect();
    if winners.len() == 1 {
        Some(winners[0])
    } else {
        None
    }
}

/// Moves branch labels (e.g., `yes`, `no`, `true`, `false`) from edges that originate a branch
/// onto an appropriate unlabeled outgoing edge from that branch's destination, then rewrites the
/// corresponding lines in-place.
///
/// The function:
/// - Parses node labels and edges from the provided lines.
/// - Identifies edges whose label is a branch label and whose source has exactly one outgoing edge.
/// - For each such edge, considers the unlabeled outgoing edges from the branch destination and
///   selects a single recipient using label polarity and text-based scoring heuristics.
/// - Transfers the branch label from the originating edge to the chosen target edge and updates
///   the original `lines` vector with the reconstructed edge lines.
///
/// # Examples
///
/// ```
/// let mut lines = vec![
///     "A -->|yes| B".to_string(),
///     "B --> C".to_string(),
///     "B --> D".to_string(),
/// ];
/// move_branch_labels(&mut lines);
/// // label removed from the original branching edge
/// assert!(!lines[0].contains("|\"yes\"|"));
/// // and moved to one of the unlabeled outgoing edges
/// assert!(lines[1].contains("|\"yes\"|") || lines[2].contains("|\"yes\"|"));
/// ```
fn move_branch_labels(lines: &mut Vec<String>) {
    let mut edges: Vec<Edge> = Vec::new();
    let mut node_labels: HashMap<String, String> = HashMap::new();

    for (idx, line) in lines.iter().enumerate() {
        if let Some(label_match) = MERMAID_NODE_LABEL_RE.captures(line) {
            let label = label_match.get(1).unwrap().as_str();
            let prefix = line.split("[\"").next().unwrap_or("").trim();
            if !prefix.is_empty() {
                node_labels.insert(prefix.to_string(), label.to_string());
            }
        }

        if let Some(caps) = MERMAID_EDGE_RE.captures(line) {
            let label = caps
                .name("quoted_label")
                .or_else(|| caps.name("unquoted_label"))
                .map(|m| m.as_str().trim().to_string());
            edges.push(Edge {
                line_idx: idx,
                indent: caps.name("indent").map_or("", |m| m.as_str()).to_string(),
                src: caps.name("src").unwrap().as_str().to_string(),
                arrow: caps.name("arrow").unwrap().as_str().to_string(),
                label,
                dst: caps.name("dst").unwrap().as_str().to_string(),
            });
        }
    }

    let mut outgoing: HashMap<String, Vec<usize>> = HashMap::new();
    for (edge_idx, edge) in edges.iter().enumerate() {
        outgoing.entry(edge.src.clone()).or_default().push(edge_idx);
    }

    let mut moved_targets: HashSet<usize> = HashSet::new();
    let mut label_moves: Vec<(usize, usize)> = Vec::new(); // (from_edge_idx, to_edge_idx)

    for edge_idx in 0..edges.len() {
        if moved_targets.contains(&edge_idx) {
            continue;
        }
        if !is_branch_label(&edges[edge_idx].label) {
            continue;
        }
        if outgoing.get(&edges[edge_idx].src).map_or(0, |v| v.len()) != 1 {
            continue;
        }

        let candidates_all = outgoing
            .get(&edges[edge_idx].dst)
            .cloned()
            .unwrap_or_default();
        if candidates_all.len() < 2 {
            continue;
        }

        let candidates: Vec<usize> = candidates_all
            .into_iter()
            .filter(|&idx| edges[idx].label.is_none())
            .collect();
        if candidates.is_empty() {
            continue;
        }

        let polarity = edges[edge_idx]
            .label
            .as_ref()
            .and_then(|l| label_polarity(l));
        if let Some(target_idx) = choose_edge(&edges, &node_labels, &candidates, polarity) {
            label_moves.push((edge_idx, target_idx));
            moved_targets.insert(target_idx);
        }
    }

    let mut changed_lines = HashSet::new();

    // Apply the label moves
    for (from_idx, to_idx) in label_moves {
        let label = edges[from_idx].label.take();
        edges[to_idx].label = label;
        changed_lines.insert(edges[from_idx].line_idx);
        changed_lines.insert(edges[to_idx].line_idx);
    }

    // Update lines
    for edge in &edges {
        if !changed_lines.contains(&edge.line_idx) {
            continue;
        }
        let new_line = if let Some(ref label) = edge.label {
            format!(
                "{}{} {}|\"{}\"| {}",
                edge.indent, edge.src, edge.arrow, label, edge.dst
            )
        } else {
            format!("{}{} {} {}", edge.indent, edge.src, edge.arrow, edge.dst)
        };
        lines[edge.line_idx] = new_line;
    }
}

/// Sanitizes a Mermaid diagram block represented as a sequence of lines.
///
/// The function normalizes node labels, repairs a specific malformed node pattern,
/// detects whether the block is a flowchart/graph or a sequence diagram, and
/// applies block-specific fixes (branch-label movement for flowcharts, participant
/// aliasing for sequence diagrams).
///
/// # Parameters
///
/// - `lines`: lines comprising a fenced Mermaid block (exclude the opening/closing fence).
///
/// # Returns
///
/// A new `Vec<String>` containing the sanitized lines for the block.
///
/// # Examples
///
/// ```
/// let input = vec![
///     "graph TD".into(),
///     "A[\"yes\"] --> B".into(),
/// ];
/// let out = sanitize_mermaid_block(&input);
/// assert!(!out.is_empty());
/// ```
pub(crate) fn sanitize_mermaid_block(lines: &[String]) -> Vec<String> {
    let mut lines = sanitize_node_labels(lines);
    lines = fix_malformed_nodes(&lines);

    let mut block_type = None;
    for line in &lines {
        let stripped = line.trim();
        if stripped.is_empty() {
            continue;
        }
        if stripped.starts_with("flowchart") || stripped.starts_with("graph") {
            block_type = Some("flowchart");
            break;
        } else if stripped.starts_with("sequenceDiagram") {
            block_type = Some("sequence");
            break;
        }
    }

    if block_type == Some("flowchart") {
        move_branch_labels(&mut lines);
    } else if block_type == Some("sequence") {
        lines = fix_sequence_diagram_participants(&lines);
    }

    lines
}

/// Sanitizes Mermaid fenced-code blocks within a Markdown document.
///
/// This returns a new string where each fenced code block labeled `mermaid` is
/// sanitized (node labels, malformed node exports, sequence diagram participants,
/// and flowchart branch labels are normalized) while non-mermaid content and the
/// original fence delimiters are preserved. A trailing newline in the input is
/// preserved in the output.
///
/// # Examples
///
/// ```
/// let md = "Intro\n```mermaid\nflowchart LR\nA[\"yes\"] --> B\n```\nOutro\n";
/// let out = sanitize_mermaid(md);
/// assert!(out.contains("```mermaid"));
/// assert!(out.contains("Outro"));
/// ```
pub(super) fn sanitize_mermaid(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut in_block = false;
    let mut block_lines: Vec<String> = Vec::new();

    for line in lines {
        if !in_block && is_mermaid_fence_open(line) {
            in_block = true;
            out.push(line.to_string());
            block_lines.clear();
            continue;
        }
        if in_block {
            if line.starts_with("```") && !is_mermaid_fence_open(line) {
                out.extend(sanitize_mermaid_block(&block_lines));
                out.push(line.to_string());
                in_block = false;
            } else {
                block_lines.push(line.to_string());
            }
            continue;
        }
        out.push(line.to_string());
    }

    if in_block {
        out.extend(sanitize_mermaid_block(&block_lines));
    }

    let mut result = out.join("\n");
    if text.ends_with('\n') {
        result.push('\n');
    }
    result
}

fn is_mermaid_fence_open(line: &str) -> bool {
    let Some(info) = line.strip_prefix("```").map(str::trim_start) else {
        return false;
    };
    let info = info.to_lowercase();
    info == "mermaid" || info.starts_with("mermaid ")
}

#[cfg(test)]
mod tests;
