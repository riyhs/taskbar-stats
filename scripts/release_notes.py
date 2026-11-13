#!/usr/bin/env python3
"""Turn conventional commits into grouped release notes."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass

SECTIONS = ("Added", "Changed", "Fixed", "Removed")
SECTION_BY_TYPE = {
    "feat": "Added",
    "fix": "Fixed",
    "perf": "Changed",
    "refactor": "Changed",
    "revert": "Fixed",
    "remove": "Removed",
}
NOISE_TYPES = {"build", "chore", "ci", "docs", "style", "test"}
CONVENTIONAL = re.compile(
    r"^(?P<type>[A-Za-z]+)(?:\((?P<scope>[^)]*)\))?(?P<breaking>!)?:\s*(?P<summary>.+)$"
)
REFERENCE = re.compile(r"(?<![\w&])#(\d+)\b")
TRAILING_REFS = re.compile(r"\s*\((?:#\d+(?:,\s*)?)+\)\s*$")
NOREPLY = re.compile(r"^(?:\d+\+)?([^@]+)@users\.noreply\.github\.com$", re.IGNORECASE)
CO_AUTHOR = re.compile(
    r"^Co-authored-by: .*<(?:\d+\+)?([^@>]+)@users\.noreply\.github\.com>",
    re.IGNORECASE | re.MULTILINE,
)
BOTS = {"dependabot[bot]", "github-actions[bot]", "renovate[bot]"}


@dataclass
class Entry:
    section: str
    summary: str
    references: list[int]
    thanks: list[str]
    commit: str


def git(*args: str) -> str:
    result = subprocess.run(
        ["git", *args],
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    if result.returncode != 0:
        raise SystemExit(result.stderr.strip() or f"git {' '.join(args)} failed")
    return result.stdout


def previous_tag(target: str) -> str:
    result = subprocess.run(
        ["git", "describe", "--tags", "--abbrev=0", "--match", "v[0-9]*", f"{target}^"],
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    return result.stdout.strip() if result.returncode == 0 else ""


def read_commits(start: str, end: str) -> list[tuple[str, str, str, str]]:
    args = ["log", "--no-merges", "--reverse", "--format=%H%x1f%s%x1f%b%x1f%ae%x1e"]
    args.append(f"{start}..{end}" if start else end)
    commits = []
    for record in git(*args).split("\x1e"):
        record = record.strip("\n")
        if not record:
            continue
        parts = record.split("\x1f")
        if len(parts) != 4:
            continue
        commits.append((parts[0], parts[1].strip(), parts[2].strip(), parts[3].strip()))
    return commits


def logins(body: str, emails: list[str], owner: str) -> list[str]:
    found: list[str] = []
    for email in emails:
        match = NOREPLY.match(email)
        if match:
            found.append(match.group(1))
    found.extend(CO_AUTHOR.findall(body))
    seen: set[str] = set()
    thanks = []
    for login in found:
        key = login.lower()
        if key in seen or key in BOTS or key == owner.lower() or key.endswith("[bot]"):
            continue
        seen.add(key)
        thanks.append(login)
    return thanks


def entry_for(
    subject: str, body: str, email: str, owner: str, commit: str
) -> Entry | None:
    match = CONVENTIONAL.match(subject)
    if match:
        kind = match.group("type").lower()
        if kind in NOISE_TYPES:
            return None
        section = SECTION_BY_TYPE.get(kind, "Changed")
        summary = TRAILING_REFS.sub("", match.group("summary")).strip()
        breaking = bool(match.group("breaking")) or "BREAKING CHANGE:" in body.upper()
    else:
        section = "Changed"
        summary = TRAILING_REFS.sub("", subject).strip()
        breaking = "BREAKING CHANGE:" in body.upper()
    if not summary:
        return None
    if summary[0].islower():
        summary = summary[0].upper() + summary[1:]
    if breaking:
        summary = f"**Breaking:** {summary}"
    references = sorted({int(number) for number in REFERENCE.findall(f"{subject}\n{body}")})
    thanks = logins(body, [email], owner)
    return Entry(section, summary, references, thanks, commit)


def commit_link(commit: str, repo: str) -> str:
    short = commit[:7]
    if "/" not in repo:
        return short
    return f"[{short}](https://github.com/{repo}/commit/{commit})"


def render(entries: list[Entry], repo: str) -> str:
    lines: list[str] = []
    for section in SECTIONS:
        items = [entry for entry in entries if entry.section == section]
        if not items:
            continue
        lines.append(f"### {section}")
        for entry in items:
            suffix = [f"#{number}" for number in entry.references]
            if entry.thanks:
                suffix.append("thanks " + ", ".join(f"@{login}" for login in entry.thanks))
            suffix.append(commit_link(entry.commit, repo))
            lines.append(f"- {entry.summary} ({', '.join(suffix)})")
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--from", dest="start", default="", help="previous tag or commit")
    parser.add_argument("--to", dest="end", default="HEAD", help="tag or commit to release")
    parser.add_argument("--repo", default="", help="owner/name used to skip maintainer thanks")
    parser.add_argument("--output", default="", help="write the notes here instead of stdout")
    args = parser.parse_args()

    owner = args.repo.split("/", 1)[0] if "/" in args.repo else ""
    start = args.start or previous_tag(args.end)
    commits = read_commits(start, args.end)

    entries: list[Entry] = []
    for commit, subject, body, email in commits:
        entry = entry_for(subject, body, email, owner, commit)
        if entry:
            entries.append(entry)

    if not entries and commits:
        entries = [
            Entry("Changed", TRAILING_REFS.sub("", subject).strip(), [], [], commit)
            for commit, subject, _, _ in commits
            if subject
        ]
        for entry in entries:
            if entry.summary and entry.summary[0].islower():
                entry.summary = entry.summary[0].upper() + entry.summary[1:]

    notes = render(entries, args.repo) if entries else "No changes in this release.\n"
    if args.output:
        with open(args.output, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(notes)
    else:
        if hasattr(sys.stdout, "reconfigure"):
            sys.stdout.reconfigure(encoding="utf-8")
        sys.stdout.write(notes)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
