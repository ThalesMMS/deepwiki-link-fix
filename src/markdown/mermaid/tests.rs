use super::*;

#[test]
fn sanitizes_markdown_lists_links_and_urls_in_node_labels() {
    assert_eq!(sanitize_label("- item"), "Unsupported markdown: list");
    assert_eq!(
        sanitize_label("See [docs](https://example.com)"),
        "See Unsupported markdown: link"
    );
    assert_eq!(
        sanitize_label("Go https://example.com"),
        "Go Unsupported markdown: link"
    );
}

#[test]
fn fixes_sequence_diagram_participants_with_spaces() {
    let lines = vec![
        "sequenceDiagram".to_string(),
        "participant User Browser".to_string(),
        "User Browser->>Server: request".to_string(),
        "Server-->>User Browser: mention User Browser in body".to_string(),
        "Note over User Browser: keep User Browser in note body".to_string(),
    ];

    assert_eq!(
        sanitize_mermaid_block(&lines),
        vec![
            "sequenceDiagram".to_string(),
            "  participant User_Browser as User Browser".to_string(),
            "User_Browser->>Server: request".to_string(),
            "Server-->>User_Browser: mention User Browser in body".to_string(),
            "Note over User_Browser: keep User Browser in note body".to_string(),
        ]
    );
}

#[test]
fn preserves_actor_alias_declarations() {
    let lines = vec![
        "sequenceDiagram".to_string(),
        "actor U as User Browser".to_string(),
        "U->>Server: request".to_string(),
    ];

    assert_eq!(
        sanitize_mermaid_block(&lines),
        vec![
            "sequenceDiagram".to_string(),
            "actor U as User Browser".to_string(),
            "U->>Server: request".to_string(),
        ]
    );
}

#[test]
fn preserves_participant_inline_config() {
    let lines = vec![
        "sequenceDiagram".to_string(),
        r#"participant API@{ "type": "boundary" }"#.to_string(),
    ];

    assert_eq!(sanitize_mermaid_block(&lines), lines);
}

#[test]
fn rewrites_create_directive_actor_tokens() {
    let lines = vec![
        "sequenceDiagram".to_string(),
        "participant User Browser".to_string(),
        "create User Browser".to_string(),
    ];

    let out = sanitize_mermaid_block(&lines);

    assert_eq!(out[1], "  participant User_Browser as User Browser");
    assert_eq!(out[2], "create User_Browser");
}

#[test]
fn moves_yes_no_branch_labels_to_unlabeled_outgoing_edges() {
    let mut lines = vec![
        "flowchart TD".to_string(),
        "A[\"Ready?\"]".to_string(),
        "B[\"Proceed\"]".to_string(),
        "C[\"Fail\"]".to_string(),
        "A -->|\"yes\"| D".to_string(),
        "D --> B".to_string(),
        "D --> C".to_string(),
    ];

    move_branch_labels(&mut lines);

    assert_eq!(lines[4], "A --> D");
    assert_eq!(lines[5], "D -->|\"yes\"| B");
}

#[test]
fn moves_unquoted_branch_labels_to_unlabeled_outgoing_edges() {
    let mut lines = vec![
        "flowchart TD".to_string(),
        "A[\"Ready?\"]".to_string(),
        "B[\"Proceed\"]".to_string(),
        "C[\"Fail\"]".to_string(),
        "A -->|yes| D".to_string(),
        "D --> B".to_string(),
        "D --> C".to_string(),
    ];

    move_branch_labels(&mut lines);

    assert_eq!(lines[4], "A --> D");
    assert_eq!(lines[5], "D -->|\"yes\"| B");
}

#[test]
fn skips_mermaid_directives_when_detecting_block_type() {
    let lines = vec![
        "%%{init: {}}%%".to_string(),
        "flowchart TD".to_string(),
        "B[\"Proceed\"]".to_string(),
        "C[\"Fail\"]".to_string(),
        "A -->|yes| D".to_string(),
        "D --> B".to_string(),
        "D --> C".to_string(),
    ];

    let out = sanitize_mermaid_block(&lines);

    assert_eq!(out[4], "A --> D");
    assert_eq!(out[5], "D -->|\"yes\"| B");
}

#[test]
fn sanitizes_complete_mermaid_fenced_blocks() {
    let input = "```mermaid\nflowchart TD\nA[\"[docs](url)\"]\n```\n";

    assert_eq!(
        sanitize_mermaid(input),
        "```mermaid\nflowchart TD\nA[\"Unsupported markdown: link\"]\n```\n"
    );
}

#[test]
fn only_sanitizes_exact_mermaid_fence_info() {
    let input = "```not-mermaid\nA[\"[docs](url)\"]\n```\n```mermaid {theme:base}\nA[\"[docs](url)\"]\n```\n";

    assert_eq!(
        sanitize_mermaid(input),
        "```not-mermaid\nA[\"[docs](url)\"]\n```\n```mermaid {theme:base}\nA[\"Unsupported markdown: link\"]\n```\n"
    );
}

#[test]
fn mermaid_block_does_not_restart_on_inner_mermaid_fence() {
    let input =
        "```mermaid\nflowchart TD\nA[\"[docs](url)\"]\n```mermaid\nB[\"[docs](url)\"]\n```\n";

    assert_eq!(
        sanitize_mermaid(input),
        "```mermaid\nflowchart TD\nA[\"Unsupported markdown: link\"]\n```mermaid\nB[\"Unsupported markdown: link\"]\n```\n"
    );
}

// --- Additional edge-case tests ---

#[test]
fn contains_list_marker_detects_bullet_after_br() {
    assert!(contains_list_marker("text<br/>- item"));
    assert!(contains_list_marker("text<br>* item"));
    assert!(contains_list_marker("text<br/>+ item"));
}

#[test]
fn contains_list_marker_detects_numbered_list_after_br() {
    assert!(contains_list_marker("first<br/>1. second"));
}

#[test]
fn contains_list_marker_rejects_plain_text() {
    assert!(!contains_list_marker("just a normal label"));
    assert!(!contains_list_marker("no-list-here"));
}

#[test]
fn sanitize_label_returns_plain_label_unchanged() {
    assert_eq!(sanitize_label("Plain label"), "Plain label");
}

#[test]
fn sanitize_label_replaces_url_only() {
    let result = sanitize_label("Visit https://example.com for details");
    assert_eq!(result, "Visit Unsupported markdown: link for details");
}

#[test]
fn sanitize_label_prioritizes_list_detection_over_url() {
    // A label that has both a list marker and a URL still returns the list message
    let result = sanitize_label("- item https://example.com");
    assert_eq!(result, "Unsupported markdown: list");
}

#[test]
fn fix_malformed_nodes_replaces_broken_content_pattern() {
    let lines = vec![
        r#"INPUTENC[broken-content]"]"#.to_string(),
        r#"normal line"#.to_string(),
    ];
    let fixed = fix_malformed_nodes(&lines);
    assert_eq!(fixed[0], r#"INPUTENC["INPUTENC"]"#);
    assert_eq!(fixed[1], "normal line");
}

#[test]
fn fix_malformed_nodes_leaves_clean_lines_unchanged() {
    let lines = vec![r#"A["Clean node"]"#.to_string(), "B --> C".to_string()];
    let fixed = fix_malformed_nodes(&lines);
    assert_eq!(fixed, lines);
}

#[test]
fn sanitize_node_labels_replaces_url_in_node_label() {
    let lines = vec![r#"A["http://example.com"]"#.to_string()];
    let out = sanitize_node_labels(&lines);
    assert_eq!(out[0], r#"A["Unsupported markdown: link"]"#);
}

#[test]
fn sanitize_node_labels_replaces_list_in_node_label() {
    let lines = vec![r#"B["- item"]"#.to_string()];
    let out = sanitize_node_labels(&lines);
    assert_eq!(out[0], r#"B["Unsupported markdown: list"]"#);
}

#[test]
fn sanitize_node_labels_leaves_clean_labels() {
    let lines = vec![r#"C["Normal text"]"#.to_string()];
    let out = sanitize_node_labels(&lines);
    assert_eq!(out, lines);
}

#[test]
fn is_branch_label_recognizes_all_branch_tokens() {
    assert!(is_branch_label(&Some("yes".to_string())));
    assert!(is_branch_label(&Some("no".to_string())));
    assert!(is_branch_label(&Some("true".to_string())));
    assert!(is_branch_label(&Some("false".to_string())));
    // Case-insensitive
    assert!(is_branch_label(&Some("YES".to_string())));
    assert!(is_branch_label(&Some(" No ".to_string())));
}

#[test]
fn is_branch_label_rejects_non_branch_tokens() {
    assert!(!is_branch_label(&Some("maybe".to_string())));
    assert!(!is_branch_label(&Some("ok".to_string())));
    assert!(!is_branch_label(&None));
}

#[test]
fn label_polarity_maps_yes_and_true_to_positive() {
    assert_eq!(label_polarity("yes"), Some("positive"));
    assert_eq!(label_polarity("true"), Some("positive"));
    assert_eq!(label_polarity("YES"), Some("positive"));
}

#[test]
fn label_polarity_maps_no_and_false_to_negative() {
    assert_eq!(label_polarity("no"), Some("negative"));
    assert_eq!(label_polarity("false"), Some("negative"));
    assert_eq!(label_polarity(" False "), Some("negative"));
}

#[test]
fn label_polarity_returns_none_for_unknown_tokens() {
    assert_eq!(label_polarity("maybe"), None);
    assert_eq!(label_polarity(""), None);
}

#[test]
fn edge_score_returns_zero_for_none_polarity() {
    assert_eq!(edge_score("success connected", None), 0);
}

#[test]
fn edge_score_positive_polarity_scores_positive_hints_higher() {
    // "success" and "connected" are positive hints
    let score = edge_score("success connected", Some("positive"));
    assert!(score > 0);
}

#[test]
fn edge_score_positive_polarity_scores_negative_hints_lower() {
    // "fail" and "error" are negative hints; with positive polarity the score should be negative
    let score = edge_score("fail error", Some("positive"));
    assert!(score < 0);
}

#[test]
fn edge_score_negative_polarity_inverts_scoring() {
    // "fail" is a negative hint; with negative polarity the score should be positive
    let score = edge_score("fail error", Some("negative"));
    assert!(score > 0);
}

#[test]
fn sanitize_mermaid_preserves_non_mermaid_content() {
    let input = "# Header\nNormal paragraph.\n```python\nprint('hello')\n```\n";
    let output = sanitize_mermaid(input);
    assert_eq!(output, input);
}

#[test]
fn sanitize_mermaid_preserves_trailing_newline() {
    let input = "```mermaid\nflowchart LR\nA --> B\n```\n";
    let output = sanitize_mermaid(input);
    assert!(output.ends_with('\n'));
}

#[test]
fn sanitize_mermaid_handles_unclosed_block_gracefully() {
    // An unclosed mermaid block should still be processed
    let input = "Text\n```mermaid\nflowchart TD\nA --> B";
    let output = sanitize_mermaid(input);
    assert!(output.contains("A --> B"));
}

#[test]
fn sanitize_mermaid_block_handles_empty_input() {
    let out = sanitize_mermaid_block(&[]);
    assert!(out.is_empty());
}

#[test]
fn sanitize_mermaid_block_handles_non_flowchart_non_sequence() {
    // A block that starts with neither "flowchart"/"graph" nor "sequenceDiagram"
    let lines = vec!["pie title Pets".to_string(), "\"Dogs\" : 386".to_string()];
    let out = sanitize_mermaid_block(&lines);
    // Pie charts have no special handling; labels are still sanitized
    assert!(!out.is_empty());
}

#[test]
fn fix_sequence_diagram_participant_with_hyphen() {
    let lines = vec![
        "sequenceDiagram".to_string(),
        "participant my-service".to_string(),
        "my-service->>Client: response".to_string(),
    ];
    let out = sanitize_mermaid_block(&lines);
    // Hyphenated participant should be aliased
    assert_eq!(out[1], "  participant my_service as my-service");
    assert!(out[2].contains("my_service"));
}

#[test]
fn sanitize_mermaid_block_graph_keyword_recognized() {
    // "graph" keyword (not "flowchart") should still trigger flowchart logic
    let lines = vec!["graph LR".to_string(), "A --> B".to_string()];
    let out = sanitize_mermaid_block(&lines);
    // Should process without panic
    assert_eq!(out[0], "graph LR");
}
