"""Quotes `description:` values in the acr plugin's skills and agents, so YAML never reads a
`: ` inside them as a mapping (which silently drops every frontmatter field).

    python tools/claude-plugin/fix_frontmatter.py
"""

import glob
import os

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "acr")


def fix(path):
    text = open(path, encoding="utf-8").read()
    if not text.startswith("---\n"):
        return False
    end = text.index("\n---", 4)
    head, rest = text[4:end], text[end:]
    lines = head.split("\n")
    changed = False
    for i, line in enumerate(lines):
        if line.startswith("description: "):
            value = line[len("description: "):].strip()
            if not (value.startswith('"') and value.endswith('"')):
                value = value.replace("\\", "\\\\").replace('"', '\\"')
                lines[i] = f'description: "{value}"'
                changed = True
    if changed:
        open(path, "w", encoding="utf-8", newline="\n").write("---\n" + "\n".join(lines) + rest)
    return changed


if __name__ == "__main__":
    paths = glob.glob(os.path.join(ROOT, "skills", "*", "SKILL.md")) + glob.glob(os.path.join(ROOT, "agents", "*.md"))
    for p in sorted(paths):
        if fix(p):
            print("quoted", os.path.relpath(p, ROOT))
