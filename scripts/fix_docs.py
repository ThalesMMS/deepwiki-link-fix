#!/usr/bin/env python3
import argparse
import re
import shutil
from dataclasses import dataclass
from pathlib import Path


INTERNAL_LINK_RE = re.compile(
    r"\]\((/[^)/\s]+/[^)/\s]+(?:/[^)\s]*)?)\)"
)
REF_STYLE_RE = re.compile(
    r"(^\s*\[[^\]]+\]:\s*)(/[^)/\s]+/[^)/\s]+(?:/[^)\s]*)?)",
    re.M,
)
LINK_COPIED_RE = re.compile(r"\s*Link copied!")
GITHUB_BLOB_SHA_RE = re.compile(
    r"https://github.com/([^/]+)/([^/]+)/blob/([0-9a-f]{7,40})/"
)

SECTION_ANCHORS = {
    "Networking Section": "networking-configuration",
    "Virtual Environment Section": "virtual-environment-setup",
    "Module Import Section": "module-import-issues",
    "WSL.exe Section": "wslexe-issues",
    "Path Translation Section": "path-translation-issues",
    "Performance Section": "performance-optimization",
    "Line Ending Section": "line-ending-issues",
    "Distribution Section": "distribution-selection",
}

NODE_LABEL_RE = re.compile(r'\["(.*?)"\]')
EDGE_RE = re.compile(
    r"^(?P<indent>\s*)(?P<src>[A-Za-z0-9_]+)\s*(?P<arrow>[-.=]+>)\s*"
    r"(?:\|\"(?P<label>[^\"]*)\"\|\s*)?(?P<dst>[A-Za-z0-9_]+)\s*$"
)

LIST_ITEM_RE = re.compile(r"^\s*(?:\d+\.\s+|[-*+]\s+)")
MD_LINK_RE = re.compile(r"\[[^\]]+\]\([^)]+\)")
URL_RE = re.compile(r"https?://\S+")

BRANCH_LABELS = {"yes", "no", "true", "false"}
POSITIVE_HINTS = [
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
]
NEGATIVE_HINTS = [
    "fail",
    "error",
    "invalid",
    "reject",
    "timeout",
    "blocked",
    "missing",
    "not",
    "false",
    "empty",
    "return",
    "skip",
    "default",
]


@dataclass
class Edge:
    line_idx: int
    indent: str
    src: str
    arrow: str
    label: str | None
    dst: str


def fix_internal_links(text: str) -> str:
    text = INTERNAL_LINK_RE.sub(lambda m: f"](https://github.com{m.group(1)})", text)
    text = REF_STYLE_RE.sub(
        lambda m: f"{m.group(1)}https://github.com{m.group(2)}", text
    )
    return text


def fix_section_links(text: str) -> str:
    for section, anchor in SECTION_ANCHORS.items():
        pattern = re.compile(
            r"https://github.com/(?P<owner>[^/]+)/(?P<repo>[^/]+)/blob/"
            r"(?P<sha>[0-9a-f]{7,40})/"
            + re.escape(section)
        )
        text = pattern.sub(
            lambda m: (
                "https://github.com/"
                f"{m.group('owner')}/{m.group('repo')}/blob/"
                f"{m.group('sha')}/README.md#{anchor}"
            ),
            text,
        )
    return text


def strip_github_blob_sha(text: str) -> str:
    return GITHUB_BLOB_SHA_RE.sub(r"https://github.com/\1/\2/", text)


def strip_preamble(text: str) -> str:
    lines = text.splitlines()
    first_header_idx = None
    for idx, line in enumerate(lines):
        if line.lstrip().startswith("#"):
            first_header_idx = idx
            break
    if first_header_idx is None or first_header_idx == 0:
        return text
    remaining = "\n".join(lines[first_header_idx:])
    if text.endswith("\n"):
        remaining += "\n"
    return remaining


def remove_link_copied(text: str) -> str:
    return LINK_COPIED_RE.sub("", text)


def contains_list_marker(label: str) -> bool:
    parts = re.split(r"<br\s*/?>", label)
    for part in parts:
        if LIST_ITEM_RE.search(part.strip()):
            return True
    return False


def sanitize_label(label: str) -> str:
    if contains_list_marker(label):
        return "Unsupported markdown: list"
    label = MD_LINK_RE.sub("Unsupported markdown: link", label)
    label = URL_RE.sub("Unsupported markdown: link", label)
    return label


def sanitize_node_labels(lines: list[str]) -> list[str]:
    def replace_label(match: re.Match[str]) -> str:
        label = match.group(1)
        return f'["{sanitize_label(label)}"]'

    return [NODE_LABEL_RE.sub(replace_label, line) for line in lines]


def is_branch_label(label: str | None) -> bool:
    if not label:
        return False
    lowered = label.strip().lower()
    return lowered in BRANCH_LABELS


def label_polarity(label: str) -> str | None:
    lowered = label.strip().lower()
    if lowered in {"yes", "true"}:
        return "positive"
    if lowered in {"no", "false"}:
        return "negative"
    return None


def edge_score(text: str, polarity: str | None) -> int:
    if not polarity:
        return 0
    text_lower = text.lower()
    pos_hits = sum(1 for hint in POSITIVE_HINTS if hint in text_lower)
    neg_hits = sum(1 for hint in NEGATIVE_HINTS if hint in text_lower)
    if polarity == "positive":
        return pos_hits - neg_hits
    return neg_hits - pos_hits


def choose_edge(
    edges: list[Edge],
    node_labels: dict[str, str],
    candidates: list[int],
    polarity: str | None,
) -> int | None:
    if not candidates:
        return None
    if len(candidates) == 1:
        return candidates[0]
    if not polarity:
        unlabeled = [idx for idx in candidates if edges[idx].label is None]
        return unlabeled[0] if len(unlabeled) == 1 else None

    scored: list[tuple[int, int]] = []
    for idx in candidates:
        target_label = node_labels.get(edges[idx].dst, edges[idx].dst)
        scored.append((edge_score(target_label, polarity), idx))
    max_score = max(score for score, _ in scored)
    if max_score <= 0:
        unlabeled = [idx for idx in candidates if edges[idx].label is None]
        return unlabeled[0] if len(unlabeled) == 1 else None
    winners = [idx for score, idx in scored if score == max_score]
    return winners[0] if len(winners) == 1 else None


def move_branch_labels(lines: list[str]) -> list[str]:
    edges: list[Edge] = []
    node_labels: dict[str, str] = {}

    for idx, line in enumerate(lines):
        label_match = NODE_LABEL_RE.search(line)
        if label_match:
            # Try to capture node id for label lookup.
            prefix = line.split('["', 1)[0].strip()
            if prefix:
                node_labels[prefix] = label_match.group(1)

        match = EDGE_RE.match(line)
        if match:
            edges.append(
                Edge(
                    line_idx=idx,
                    indent=match.group("indent"),
                    src=match.group("src"),
                    arrow=match.group("arrow"),
                    label=match.group("label"),
                    dst=match.group("dst"),
                )
            )

    outgoing: dict[str, list[int]] = {}
    for edge_idx, edge in enumerate(edges):
        outgoing.setdefault(edge.src, []).append(edge_idx)

    moved_targets: set[int] = set()

    for edge_idx, edge in enumerate(edges):
        if edge_idx in moved_targets:
            continue
        if not is_branch_label(edge.label):
            continue
        if len(outgoing.get(edge.src, [])) != 1:
            continue

        candidates = outgoing.get(edge.dst, [])
        if len(candidates) < 2:
            continue

        candidates = [idx for idx in candidates if edges[idx].label is None]
        if not candidates:
            continue

        polarity = label_polarity(edge.label or "")
        target_idx = choose_edge(edges, node_labels, candidates, polarity)
        if target_idx is None:
            continue

        target_edge = edges[target_idx]

        target_edge.label = edge.label
        edge.label = None
        moved_targets.add(target_idx)

    for edge in edges:
        if edge.label:
            lines[edge.line_idx] = (
                f'{edge.indent}{edge.src} {edge.arrow}|"{edge.label}"| {edge.dst}'
            )
        else:
            lines[edge.line_idx] = f"{edge.indent}{edge.src} {edge.arrow} {edge.dst}"

    return lines


def sanitize_mermaid_block(lines: list[str]) -> list[str]:
    lines = sanitize_node_labels(lines)
    block_type = None
    for line in lines:
        stripped = line.strip()
        if not stripped:
            continue
        if stripped.startswith("flowchart") or stripped.startswith("graph"):
            block_type = "flowchart"
        break
    if block_type == "flowchart":
        return move_branch_labels(lines)
    return lines


def sanitize_mermaid(text: str) -> str:
    lines = text.splitlines()
    out: list[str] = []
    in_block = False
    block_lines: list[str] = []

    for line in lines:
        if line.startswith("```") and "mermaid" in line:
            in_block = True
            out.append(line)
            block_lines = []
            continue
        if in_block:
            if line.startswith("```"):
                out.extend(sanitize_mermaid_block(block_lines))
                out.append(line)
                in_block = False
            else:
                block_lines.append(line)
            continue
        out.append(line)

    if in_block:
        out.extend(sanitize_mermaid_block(block_lines))

    result = "\n".join(out)
    if text.endswith("\n"):
        result += "\n"
    return result


def process_text(text: str) -> str:
    text = strip_preamble(text)
    text = remove_link_copied(text)
    text = fix_internal_links(text)
    text = fix_section_links(text)
    text = strip_github_blob_sha(text)
    text = sanitize_mermaid(text)
    return text


def process_directory(input_dir: Path, output_dir: Path, dry_run: bool) -> list[Path]:
    changed_files: list[Path] = []
    for path in input_dir.rglob("*"):
        if path.is_dir():
            continue
        rel_path = path.relative_to(input_dir)
        if rel_path.name.startswith("."):
            continue
        out_path = output_dir / rel_path
        if path.suffix.lower() != ".md":
            if not dry_run:
                out_path.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, out_path)
            continue
        original = path.read_text()
        updated = process_text(original)
        if updated != original:
            changed_files.append(out_path)
        if not dry_run:
            out_path.parent.mkdir(parents=True, exist_ok=True)
            out_path.write_text(updated)
    return changed_files


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Normalize DeepWiki markdown links and mermaid diagrams."
    )
    parser.add_argument("input_dir", type=Path)
    parser.add_argument("output_dir", type=Path, nargs="?")
    parser.add_argument(
        "--in-place",
        action="store_true",
        help="Rewrite files in place instead of writing to output_dir.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print files that would change without writing output.",
    )
    args = parser.parse_args()

    if args.in_place:
        output_dir = args.input_dir
    else:
        if not args.output_dir:
            parser.error("output_dir is required unless --in-place is set")
        output_dir = args.output_dir

    changed = process_directory(args.input_dir, output_dir, args.dry_run)
    if args.dry_run:
        for path in changed:
            print(path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
