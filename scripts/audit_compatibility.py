#!/usr/bin/env python3
"""Compare pinned upstream registries with the native compatibility fixtures.

Usage: python3 scripts/audit_compatibility.py /path/to/rondocode
Reads source only; does not execute upstream JS or fetch anything.
Run cargo test as well: fixture coverage alone does not prove behavior.
"""
import json
import re
import subprocess
import sys
from pathlib import Path


def audit(upstream: Path) -> dict:
    root = Path(__file__).resolve().parents[1]
    reference = re.search(r"`([0-9a-f]{40})`", (root / "NOTICE.md").read_text()).group(1)
    revision = subprocess.run(
        ["git", "-C", str(upstream), "rev-parse", "HEAD"],
        check=True, capture_output=True, text=True,
    ).stdout.strip()
    registry = (upstream / "packages/rondo/src/builtins.ts").read_text()
    registry = registry.split("export const BUILTINS:")[1].split("/** Names usable")[0]
    entries = list(re.finditer(r"^  (\w+):", registry, re.MULTILINE))
    fixtures = (root / "tests/compatibility.rs").read_text()
    def fixture_array(name: str) -> str:
        return re.search(r"const " + name + r":.*?=\s*&\[(.*?)\];", fixtures, re.DOTALL).group(1)

    def strings(source: str) -> list[str]:
        return re.findall(r'"([^"\\]*)"', source)

    native = dict((name, expression) for name, expression in re.findall(
        r'\(\s*"(\w+)",\s*"((?:[^"\\]|\\.)*)",\s*(?:true|false),?\s*\)', fixture_array("BUILTINS")
    ))
    external = {"mic"}
    host_builtins = {"ddsp"}
    host_directives = {"sing"}
    gaps = []
    builtins = []
    option_count = 0
    native_option_count = 0
    host_option_count = 0
    for i, match in enumerate(entries):
        name = match.group(1)
        spec = registry[match.end():entries[i+1].start() if i+1 < len(entries) else len(registry)]
        named = re.search(r"named:\s*\{(.*?)\}", spec, re.DOTALL)
        options = re.findall(r"(\w+):\s*'(?:sig|num|enum|bool)'", named.group(1)) if named else []
        option_count += len(options)
        if name in external:
            status = "excluded by scope"
        elif name in host_builtins:
            status = "host adapter fixture"
        else:
            status = "native fixture"
        if name not in external and name not in native:
            gaps.append(f"builtin {name} has no fixture")
            status = "uncovered"
        missing_options = [option for option in options if name not in external
                           and f"{option}:" not in native.get(name, "")]
        gaps.extend(f"{name}.{option} has no fixture" for option in missing_options)
        excluded_options = options if name in external else []
        covered_options = len(options) - len(excluded_options) - len(missing_options)
        native_option_count += covered_options
        if name in host_builtins:
            host_option_count += covered_options
        builtins.append({"name": name, "status": status, "named_options": options,
                         "unsupported_named_options": excluded_options})
    parser = (upstream / "packages/rondo/src/parser.ts").read_text()
    directives = re.findall(r"'([^']+)'", re.search(
        r"BLOCK_KEYWORDS:.*?=\s*\[(.*?)\]", parser, re.DOTALL
    ).group(1))
    supported = strings(fixture_array("DIRECTIVES"))
    excluded = {"visual", "mask", "draw", "js"}
    gaps.extend(f"directive {name} has no classification" for name in directives
                if name not in supported and name not in excluded)
    combs = re.findall(r"'([^']+)'", re.search(
        r"COMB_WORDS = new Set\(\[(.*?)\]\)", parser, re.DOTALL
    ).group(1))
    fn_combs = re.findall(r"^  (\w+):", re.search(
        r"FN_COMBS:.*?=\s*\{(.*?)\n\}", parser, re.DOTALL
    ).group(1), re.MULTILINE)
    combinators = sorted({name.lower() for name in combs + fn_combs})
    positive = re.search(r"fn supported_modifier_surface.*?for modifier in \[(.*?)\]", fixtures, re.DOTALL).group(1)
    native_modifiers = {s.split()[0].rstrip(':').lower() for s in strings(positive)}
    unsupported_modifiers = set(combinators) - native_modifiers
    for name in combinators:
        if name not in native_modifiers:
            gaps.append(f"modifier {name} has no fixture")
    voice_flags = re.findall(r"'([^']+)'", re.search(r"VOICE_FLAGS:.*?new Set\(\[(.*?)\]\)", parser).group(1))
    voice_options = re.findall(r"'([^']+)'", re.search(r"VOICE_OPTS:.*?new Set\(\[(.*?)\]\)", parser).group(1))
    signals = re.findall(r"'([^']+)'", re.search(r"const SIGNALS = new Set\(\[(.*?)\]\)", parser).group(1))
    for name in voice_flags + voice_options:
        if name not in strings(fixture_array("VOICE_OPTIONS")):
            gaps.append(f"voice option {name} has no fixture")
    for name in signals:
        if name not in strings(fixture_array("SIGNALS")):
            gaps.append(f"control signal {name} has no fixture")
    scales_source = (upstream / "packages/pattern/src/scales.ts").read_text()
    scales = re.findall(r"^  (\w+):", re.search(r"export const SCALES:.*?=\s*\{(.*?)^\}", scales_source, re.DOTALL | re.MULTILINE).group(1), re.MULTILINE)
    aliases = re.findall(r"(\w+):\s*'", re.search(r"export const SCALE_MODE:.*?=\s*\{(.*?)\}", scales_source, re.DOTALL).group(1))
    scale_fixtures = strings(re.search(r"fn scalar_and_patterned_scales.*?for mode in \[(.*?)\]", fixtures, re.DOTALL).group(1))
    for name in scales + aliases:
        if name not in scale_fixtures:
            gaps.append(f"scale {name} has no fixture")
    chord_source = (upstream / "packages/pattern/src/chords.ts").read_text()
    qualities = re.search(r"const QUALITIES:.*?=\s*\{(.*?)^\}", chord_source, re.DOTALL | re.MULTILINE).group(1)
    qualities = re.sub(r"/\*.*?\*/|//[^\n]*", "", qualities, flags=re.DOTALL)
    qualities = [a or b for a, b in re.findall(r"(?:'([^']*)'|(\w+)):\s*\[", qualities)]
    for name in qualities:
        if name not in strings(fixture_array("CHORD_QUALITIES")):
            gaps.append(f"chord quality {name} has no fixture")
    if revision != reference:
        gaps.append("checkout revision differs from NOTICE.md; refresh the audited contract")
    return {
        "reference": reference, "checkout": revision,
        "builtin_count": len(entries), "named_option_count": option_count,
        "supported_named_option_count": native_option_count,
        "native_named_option_count": native_option_count - host_option_count,
        "host_named_option_count": host_option_count,
        "supported_builtin_count": len(native),
        "native_builtin_count": len(native.keys() - host_builtins),
        "host_builtin_count": len(native.keys() & host_builtins),
        "directive_count": len(directives), "supported_directive_count": len(supported),
        "native_directive_count": len(set(supported) - host_directives),
        "host_directive_count": len(set(supported) & host_directives),
        "modifier_count": len(combinators), "modifiers": combinators,
        "native_modifier_count": len(set(combinators) & native_modifiers),
        "unsupported_modifiers": sorted(set(combinators) & unsupported_modifiers),
        "voice_option_count": len(voice_flags) + len(voice_options),
        "control_signal_count": len(signals), "scale_count": len(scales),
        "scale_alias_count": len(aliases), "chord_quality_count": len(qualities),
        "builtins": builtins, "uncovered": gaps,
    }


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    result = audit(Path(sys.argv[1]))
    print(json.dumps(result, indent=2))
    raise SystemExit(bool(result["uncovered"]))
