#!/usr/bin/env python3
"""Generate the Rust field-data tables from the eligible C data sources.

The field data is intentionally kept as typed Rust data.  The C files are
used only as the source of truth while generating this crate; generated-disc
payload translation units are rejected by the classifier and never read by
this script.
"""

from __future__ import annotations

import hashlib
import re
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
SOURCE_ROOT = ROOT / "src/data/field"
CRATE_ROOT = ROOT / "rust/nookery-field-data"
OUT = CRATE_ROOT / "src/generated.rs"
MANIFEST_RS = CRATE_ROOT / "src/source_manifest.rs"
MANIFEST = CRATE_ROOT / "SOURCE_MANIFEST.toml"


def strip_comments(source: str) -> str:
    source = re.sub(r"/\*.*?\*/", "", source, flags=re.S)
    return re.sub(r"//.*", "", source)


def matching(source: str, start: int) -> int:
    depth = 0
    for index in range(start, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return index
    raise ValueError(f"unclosed initializer at {start}")


def parse_braces(source: str, start: int) -> list[object]:
    """Parse a C initializer into nested lists and scalar strings."""
    if source[start] != "{":
        raise ValueError("initializer must start with '{'")

    def parse_list(index: int) -> tuple[list[object], int]:
        assert source[index] == "{"
        index += 1
        result: list[object] = []
        scalar_start = index
        while True:
            if index >= len(source):
                raise ValueError("unclosed initializer")
            char = source[index]
            if char == "{":
                if source[scalar_start:index].strip().strip(","):
                    raise ValueError(f"unexpected scalar before nested list: {source[scalar_start:index]}")
                nested, index = parse_list(index)
                result.append(nested)
                scalar_start = index
                continue
            if char == "}":
                scalar = source[scalar_start:index].strip().strip(",").strip()
                if scalar:
                    result.extend(x.strip() for x in scalar.split(",") if x.strip())
                return result, index + 1
            if char == ",":
                scalar = source[scalar_start:index].strip()
                if scalar:
                    result.append(scalar)
                scalar_start = index + 1
            index += 1

    return parse_list(start)[0]


def initializer(source: str, marker: str) -> list[object]:
    start = source.index(marker)
    brace = source.index("{", start)
    return parse_braces(source, brace)


def number(value: str) -> int:
    value = value.strip().replace("U", "").replace("L", "")
    return int(value, 0)


def ident(value: str) -> str:
    value = value.strip()
    if value == "NULL":
        return "None"
    if re.fullmatch(r"[+-]?(?:0[xX][0-9a-fA-F]+|[0-9]+)", value):
        return str(number(value))
    return value


def rust_ident(value: str) -> str:
    value = re.sub(r"[^A-Za-z0-9_]", "_", value)
    if value and value[0].isdigit():
        value = "_" + value
    return value


def parse_anime(path: Path) -> tuple[str, list[tuple[str, list[tuple[int, int, int, int]]]], list[tuple[int, int, str]]]:
    source = strip_comments(path.read_text())
    arrays: list[tuple[str, list[tuple[int, int, int, int]]]] = []
    for match in re.finditer(r"EVW_ANIME_SCROLL\s+([A-Za-z0-9_]+)\s*\[\]\s*=", source):
        values = initializer(source, match.group(0))
        rows = []
        for row in values:
            assert isinstance(row, list)
            rows.append(tuple(number(str(x)) for x in row))
        arrays.append((match.group(1), rows))
    main = re.search(r"EVW_ANIME_DATA\s+([A-Za-z0-9_]+)\s*\[\]\s*=", source)
    if not main:
        raise ValueError(f"missing animation data in {path}")
    entries = []
    for row in initializer(source, main.group(0)):
        assert isinstance(row, list)
        entries.append((number(str(row[0])), number(str(row[1])) if str(row[1]).isdigit() else 1, str(row[2])))
    return main.group(1), arrays, entries


def parse_actor(path: Path) -> tuple[str, list[list[str]]]:
    source = strip_comments(path.read_text())
    match = re.search(r"mFM_move_actor_data_c\s+([A-Za-z0-9_]+)\s*\[[^]]*\]\s*=", source)
    if not match:
        raise ValueError(f"missing move actor data in {path}")
    rows = []
    for row in initializer(source, match.group(0)):
        assert isinstance(row, list)
        rows.append([ident(str(x)) for x in row])
    return match.group(1), rows


def parse_field(source: str) -> list[list[object]]:
    return initializer(source, "mFM_field_data_c data_fdd")


def parse_bg(source: str) -> list[list[object]]:
    return initializer(source, "mFM_bg_data_c data_bgd")


def fmt_symbol(value: str) -> str:
    return '"' + value.replace('\\', '\\\\').replace('"', '\\"') + '"'


def fmt_opt_symbol(value: str) -> str:
    return "None" if value == "None" else f"Some({fmt_symbol(value)})"


def emit_anime(out: list[str], path: Path) -> str:
    name, arrays, entries = parse_anime(path)
    for array_name, rows in arrays:
        out.append(f"pub static {rust_ident(array_name)}: [Scroll; {len(rows)}] = [")
        for x, y, width, height in rows:
            out.append(f"    Scroll {{ x: {x}, y: {y}, width: {width}, height: {height} }},")
        out.append("];\n")
    out.append(f"pub static {rust_ident(name)}: [Animation; {len(entries)}] = [")
    for segment, kind, data in entries:
        out.append(
            f"    Animation {{ segment: {segment}, kind: {kind}, scrolls: &{rust_ident(data)} }},"
        )
    out.append("];\n")
    return name


def emit_actor(out: list[str], path: Path) -> str:
    name, rows = parse_actor(path)
    out.append(f"pub static {rust_ident(name)}: [MoveActorData; {len(rows)}] = [")
    for row in rows:
        assert len(row) == 8
        out.append(
            "    MoveActorData { name_id: %s, block_x: %s, block_z: %s, ut_x: %s, "
            "ut_z: %s, npc_info_idx: %s, arg: %s, desired_block_kind: %s },"
            % tuple(fmt_symbol(x) if not re.fullmatch(r"-?\d+", x) else x for x in row)
        )
    out.append("];\n")
    return name


def emit_field(out: list[str], source: str) -> None:
    entries = parse_field(source)
    out.append(f"pub static data_fdd: [FieldData; {len(entries)}] = [")
    for entry in entries:
        assert len(entry) == 9
        field_name, block_x, block_z, combinations, actors, a, b, c, d = entry
        assert isinstance(combinations, list)
        out.append("    FieldData {")
        out.append(f"        field_name: {fmt_symbol(str(field_name))},")
        out.append(f"        block_x_max: {number(str(block_x))}, block_z_max: {number(str(block_z))},")
        out.append(f"        combinations: &[")
        for combo in combinations:
            assert isinstance(combo, list) and len(combo) == 2
            out.append(f"            Combination {{ combination_type: {fmt_symbol(str(combo[0]))}, height: {number(str(combo[1]))} }},")
        for _ in range(70 - len(combinations)):
            out.append('            Combination { combination_type: "BLOCK_COMBI_GRD_0", height: 0 },')
        out.append("        ],")
        out.append(f"        move_actor_table: {fmt_opt_symbol(ident(str(actors)))},")
        out.append(f"        reserved: [{number(str(a))}, {number(str(b))}, {number(str(c))}, {number(str(d))}],")
        out.append("    },")
    out.append("];\n")


def emit_bg(out: list[str], source: str) -> None:
    entries = parse_bg(source)
    out.append(f"pub static data_bgd: [BgData; {len(entries)}] = [")
    for entry in entries:
        assert len(entry) == 9, len(entry)
        bg_id, opaque, translucent, animation, count, rom_start, rom_end, collisions, sounds = entry
        assert isinstance(collisions, list) and isinstance(sounds, list)
        out.append("    BgData {")
        out.append(f"        bg_id: {fmt_symbol(str(bg_id))},")
        out.append(f"        opaque_gfx: {fmt_opt_symbol(ident(str(opaque)))}, translucent_gfx: {fmt_opt_symbol(ident(str(translucent)))},")
        out.append(f"        animation: {fmt_opt_symbol(ident(str(animation)))}, animation_count: {number(str(count))},")
        out.append(f"        rom_start_addr: {number(str(rom_start))}, rom_end_addr: {number(str(rom_end))},")
        out.append("        collision: &[")
        for row in collisions:
            assert isinstance(row, list) and len(row) == 16
            for cell in row:
                assert isinstance(cell, list) and len(cell) == 7
                values = ", ".join(str(number(str(x))) for x in cell[:6])
                out.append(f"            Collision {{ values: [{values}], attribute: {fmt_symbol(str(cell[6]))} }},")
        out.append("        ],")
        out.append("        sound_source: &[")
        for sound in sounds:
            assert isinstance(sound, list) and len(sound) == 3
            out.append(f"            SoundSource {{ kind: {fmt_symbol(str(sound[0]))}, ut_x: {number(str(sound[1]))}, ut_z: {number(str(sound[2]))} }},")
        out.append("        ],")
        out.append("    },")
    out.append("];\n")


@dataclass(frozen=True)
class Source:
    path: Path
    kind: str
    symbol: str


def classify() -> tuple[list[Source], list[Path]]:
    eligible: list[Source] = []
    generated: list[Path] = []
    for path in sorted(SOURCE_ROOT.rglob("*.c")):
        source = path.read_text()
        if '#include "assets/' in source:
            generated.append(path)
            continue
        if path.name.endswith("_evw_anime.c"):
            symbol = re.search(r"EVW_ANIME_DATA\s+([A-Za-z0-9_]+)", source).group(1)
            kind = "event_animation"
        elif path.name == "field_data.c":
            symbol, kind = "data_fdd", "field_data"
        elif path.name == "bg_data.c":
            symbol, kind = "data_bgd", "background_data"
        elif "mFM_move_actor_data_c" in source:
            symbol = re.search(r"mFM_move_actor_data_c\s+([A-Za-z0-9_]+)", source).group(1)
            kind = "move_actor"
        else:
            raise ValueError(f"unclassified eligible file: {path}")
        eligible.append(Source(path, kind, symbol))
    assert len(eligible) == 182, len(eligible)
    assert len(generated) == 274, len(generated)
    return eligible, generated


def main() -> None:
    eligible, _generated = classify()
    out = [
        "// @generated by tools/generate.py; do not edit by hand.",
        "#![allow(dead_code, non_upper_case_globals)]",
        "use crate::{Animation, BgData, Collision, Combination, FieldData, MoveActorData, Scroll, SoundSource};",
        "",
    ]
    for source in eligible:
        text = source.path.read_text()
        if source.kind == "event_animation":
            emit_anime(out, source.path)
        elif source.kind == "move_actor":
            emit_actor(out, source.path)
        elif source.kind == "field_data":
            emit_field(out, strip_comments(text))
        elif source.kind == "background_data":
            emit_bg(out, strip_comments(text))
    OUT.write_text("\n".join(out).rstrip("\n") + "\n")

    lines = [
        "# Generated from checked-in field-data sources.",
        "# Files containing assets/*.inc are intentionally excluded.",
        "",
    ]
    for source in eligible:
        digest = hashlib.sha256(source.path.read_bytes()).hexdigest()
        rel = source.path.relative_to(ROOT).as_posix()
        lines.extend(["[[source]]", f'path = "{rel}"', f'kind = "{source.kind}"', f'symbol = "{source.symbol}"', f'sha256 = "{digest}"', ""])
    MANIFEST.write_text("\n".join(lines))

    manifest_rs = ["// @generated by tools/generate.py; do not edit by hand.", "use crate::SourceFile;", "", "pub static SOURCE_MANIFEST: &[SourceFile] = &["]
    for source in eligible:
        digest = hashlib.sha256(source.path.read_bytes()).hexdigest()
        rel = source.path.relative_to(ROOT).as_posix()
        manifest_rs.extend(
            [
                "    SourceFile {",
                f'        path: "{rel}",',
                f'        kind: "{source.kind}",',
                f'        symbol: "{source.symbol}",',
                f'        sha256: "{digest}",',
                "    },",
            ]
        )
    manifest_rs.extend(["];"])
    MANIFEST_RS.write_text("\n".join(manifest_rs) + "\n")


if __name__ == "__main__":
    main()
