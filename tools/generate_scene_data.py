#!/usr/bin/env python3
"""Generate Rust scene-word tables from src/data/scene/*.c.

Scope: exactly the 51 files in ``src/data/scene``.  These files carry no
``#include "assets/*.inc"`` payload includes, so the C text in the tree is the
complete oracle and no C compiler is involved.

Constants are transcribed from the repository headers
(``include/m_scene.h``, ``include/m_scene_table.h``) and the actor profile enum
at ``include/m_actor.h`` is evaluated positionally from the header text at
generation time, so no enum value is guessed.

Pointer caveat: a relocatable Rust ``const`` cannot hold a host address, so a
scene word that carries a pointer records the target *symbol name*.  The five
scalar/packed words are exact; the pointer is compared by symbol identity.  This
is recorded in SOURCE_MANIFEST.toml as ``pointer_parity = "symbol-identity"``.

Run ``python3 tools/generate_scene_data.py`` to rewrite the generated tree, or
add ``--check`` to fail when it is stale.
"""

from __future__ import annotations

import argparse
import hashlib
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC_DIR = ROOT / "src/data/scene"
CRATE = Path("rust/nookery-scene-data")
OUT_FILE = CRATE / "src/generated/scene_words.rs"
MANIFEST = CRATE / "SOURCE_MANIFEST.toml"

# --- constants transcribed from include/m_scene.h and include/m_scene_table.h
SCENE_DATA_TYPE = {
    "mSc_SCENE_DATA_TYPE_PLAYER_PTR": 0,
    "mSc_SCENE_DATA_TYPE_CTRL_ACTOR_PTR": 1,
    "mSc_SCENE_DATA_TYPE_ACTOR_PTR": 2,
    "mSc_SCENE_DATA_TYPE_OBJECT_EXCHANGE_BANK_PTR": 3,
    "mSc_SCENE_DATA_TYPE_DOOR_DATA_PTR": 4,
    "mSc_SCENE_DATA_TYPE_FIELD_CT": 5,
    "mSc_SCENE_DATA_TYPE_MY_ROOM_CT": 6,
    "mSc_SCENE_DATA_TYPE_ARRANGE_ROOM_CT": 7,
    "mSc_SCENE_DATA_TYPE_ARRANGE_FURNITURE_CT": 8,
    "mSc_SCENE_DATA_TYPE_SOUND": 9,
    "mSc_SCENE_DATA_TYPE_END": 10,
}
ITEM_TYPE = {
    "mSc_ITEM_TYPE_BGITEM": 0,
    "mSc_ITEM_TYPE_DUMMY": 1,
    "mSc_ITEM_TYPE_BGPOLICEITEM": 2,
    "mSc_ITEM_TYPE_BGPOSTITEM": 3,
}
ROOM_TYPE = {
    "mSc_ROOM_TYPE_OUTDOORS": 0,
    "mSc_ROOM_TYPE_MY_ROOM": 1,
    "mSc_ROOM_TYPE_NPC_ROOM": 2,
    "mSc_ROOM_TYPE_MISC_ROOM": 3,
}
DRAW_TYPE = {
    "FIELD_DRAW_TYPE_OUTDOORS": 0,
    "FIELD_DRAW_TYPE_INDOORS": 1,
    "FIELD_DRAW_TYPE_TRAIN": 2,
    "FIELD_DRAW_TYPE_PLAYER_SELECT": 3,
}
DOOR_SCENE = {"SCENE_FG": 0, "SCENE_BG": 1}

CONSTANTS = {**SCENE_DATA_TYPE, **ITEM_TYPE, **ROOM_TYPE, **DRAW_TYPE, **DOOR_SCENE}

PTR_KIND = {
    "mSc_DATA_PLAYER": ("player", "mSc_SCENE_DATA_TYPE_PLAYER_PTR"),
    "mSc_DATA_CTRL_ACTORS": ("ctrl_actor", "mSc_SCENE_DATA_TYPE_CTRL_ACTOR_PTR"),
    "mSc_DATA_ACTORS": ("actor", "mSc_SCENE_DATA_TYPE_ACTOR_PTR"),
    "mSc_DATA_OBJ_BANK": ("obj_bank", "mSc_SCENE_DATA_TYPE_OBJECT_EXCHANGE_BANK_PTR"),
    "mSc_DATA_DOOR_DATA": ("door_data", "mSc_SCENE_DATA_TYPE_DOOR_DATA_PTR"),
}


def load_positional_enum(header: str, opening: str) -> dict[str, int]:
    """Evaluate a positional C enum from a repository header.

    Any explicit ``= value`` inside the block is honoured, so the result is
    derived from the header text rather than from a hand-copied list.
    """
    text = (ROOT / header).read_text(encoding="utf-8", errors="replace")
    if opening not in text:
        raise SystemExit(f"abort: enum {opening!r} not found in {header}")
    start = text.index(opening)
    body = text[text.index("{", start) + 1 : text.index("};", start)]
    values: dict[str, int] = {}
    index = 0
    for raw in body.splitlines():
        line = raw.split("//")[0].strip().rstrip(",")
        if not line:
            continue
        if "=" in line:
            name, value = (part.strip() for part in line.split("=", 1))
            index = int(value, 0)
        else:
            name = line
        if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name):
            values[name] = index
            index += 1
    if not values:
        raise SystemExit(f"abort: no enumerators parsed from {opening} in {header}")
    return values


def load_actor_profiles() -> dict[str, int]:
    return load_positional_enum("include/m_actor.h", "enum actor_profile_table {")


def load_bank_ids() -> dict[str, int]:
    return load_positional_enum("include/m_actor.h", "enum bank_id {")


PROFILES: dict[str, int] = {}

DECL = re.compile(
    r"^(?:extern\s+)?(?P<kind>Scene_Word_u|Actor_data|s16)\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*\[\s*\]\s*=\s*\{(?P<body>.*?)\}\s*;\s*$",
    re.DOTALL | re.MULTILINE,
)
MACRO_CALL = re.compile(r"^(?P<macro>mSc_DATA_[A-Z_]+)\s*\((?P<args>[^()]*)\)$", re.DOTALL)


def strip_comments(text: str) -> str:
    text = re.sub(r"/\*.*?\*/", " ", text, flags=re.DOTALL)
    return re.sub(r"//[^\n]*", " ", text)


def split_top_level(body: str) -> list[str]:
    """Split on commas at brace AND paren depth zero.

    Macro calls such as ``mSc_DATA_SOUND(0, 0)`` contain commas inside
    parentheses; splitting only on braces would tear them apart.
    """
    items: list[str] = []
    brace = 0
    paren = 0
    current: list[str] = []
    for char in body:
        if char == "{":
            brace += 1
        elif char == "}":
            brace -= 1
        elif char == "(":
            paren += 1
        elif char == ")":
            paren -= 1
        if char == "," and brace == 0 and paren == 0:
            items.append("".join(current).strip())
            current = []
        else:
            current.append(char)
    tail = "".join(current).strip()
    if tail:
        items.append(tail)
    return [item for item in items if item]


def resolve(token: str, where: str) -> int:
    token = token.strip()
    if token in CONSTANTS:
        return CONSTANTS[token]
    if token in PROFILES:
        return PROFILES[token]
    if re.fullmatch(r"-?(0[xX][0-9a-fA-F]+|\d+)", token):
        return int(token, 0)
    raise SystemExit(f"abort: unresolvable constant {token!r} at {where}")


@dataclass
class Word:
    kind: str
    a: int
    b: int
    c: int
    d: int
    packed: int = 0
    ptr: str | None = None


@dataclass
class Table:
    source: str
    symbol: str
    kind: str
    words: list[Word] = field(default_factory=list)
    actors: list[tuple[int, tuple[int, int, int], tuple[int, int, int], int]] = field(default_factory=list)
    scalars: list[int] = field(default_factory=list)


def parse_scene_words(body: str, source: str, symbol: str) -> list[Word]:
    words: list[Word] = []
    for index, entry in enumerate(split_top_level(body)):
        entry = entry.strip()
        where = f"{source}:{symbol}[{index}]"
        match = MACRO_CALL.match(entry)
        if not match:
            raise SystemExit(f"abort: unsupported scene word entry at {where}: {entry[:90]}")
        macro = match.group("macro")
        args = [a.strip() for a in split_top_level(match.group("args"))]
        if macro == "mSc_DATA_SOUND":
            words.append(Word("sound", SCENE_DATA_TYPE["mSc_SCENE_DATA_TYPE_SOUND"],
                              resolve(args[0], where), resolve(args[1], where), 0))
        elif macro == "mSc_DATA_END":
            words.append(Word("end", SCENE_DATA_TYPE["mSc_SCENE_DATA_TYPE_END"], 0, 0, 0))
        elif macro == "mSc_DATA_MY_ROOM_CT":
            words.append(Word("my_room", SCENE_DATA_TYPE["mSc_SCENE_DATA_TYPE_MY_ROOM_CT"], 0, 0, 0))
        elif macro == "mSc_DATA_ARRANGE_ROOM_CT":
            words.append(Word("arrange_room", SCENE_DATA_TYPE["mSc_SCENE_DATA_TYPE_ARRANGE_ROOM_CT"], 0, 0, 0))
        elif macro == "mSc_DATA_ARRANGE_FTR":
            words.append(Word("arrange_ftr", SCENE_DATA_TYPE["mSc_SCENE_DATA_TYPE_ARRANGE_FURNITURE_CT"],
                              resolve(args[0], where), 0, 0))
        elif macro in PTR_KIND:
            kind, type_name = PTR_KIND[macro]
            if macro == "mSc_DATA_PLAYER":
                count, target = 1, args[0]
            else:
                count, target = resolve(args[0], where), args[1]
            words.append(Word(kind, SCENE_DATA_TYPE[type_name], count, 0, 0, ptr=target))
        elif macro == "mSc_DATA_FIELDCT":
            item = resolve(args[0], where)
            bg = resolve(args[1], where)
            disp = resolve(args[2], where)
            room = resolve(args[3], where)
            draw = resolve(args[4], where)
            packed = ((disp & 0xFFFF) << 16) | ((room & 0xFF) << 8) | (draw & 0xFF)
            words.append(Word("field_ct", SCENE_DATA_TYPE["mSc_SCENE_DATA_TYPE_FIELD_CT"],
                              item, bg, 0, packed=packed))
        else:
            raise SystemExit(f"abort: unsupported macro {macro} at {where}")
    return words


def parse_actors(body: str, source: str, symbol: str) -> list[tuple[int, tuple[int, int, int], tuple[int, int, int], int]]:
    rows = []
    for index, entry in enumerate(split_top_level(body)):
        inner = entry.strip()
        where = f"{source}:{symbol}[{index}]"
        if not (inner.startswith("{") and inner.endswith("}")):
            raise SystemExit(f"abort: Actor_data entry is not a brace initializer at {where}")
        parts = split_top_level(inner[1:-1])
        if len(parts) != 4:
            raise SystemExit(f"abort: Actor_data entry has {len(parts)} fields, expected 4, at {where}")
        profile = resolve(parts[0], where)
        pos = [resolve(v, where) for v in split_top_level(parts[1].strip()[1:-1])]
        rot = [resolve(v, where) for v in split_top_level(parts[2].strip()[1:-1])]
        arg = resolve(parts[3], where)
        if len(pos) != 3 or len(rot) != 3:
            raise SystemExit(f"abort: s_xyz must have three components at {where}")
        rows.append((profile, (pos[0], pos[1], pos[2]), (rot[0], rot[1], rot[2]), arg))
    return rows


def parse_file(path: Path) -> list[Table]:
    text = strip_comments(path.read_text(encoding="utf-8", errors="replace"))
    tables: list[Table] = []
    for match in DECL.finditer(text):
        kind, symbol, body = match.group("kind"), match.group("name"), match.group("body")
        source = str(path.relative_to(ROOT))
        table = Table(source=source, symbol=symbol, kind=kind)
        if kind == "Scene_Word_u":
            table.words = parse_scene_words(body, source, symbol)
        elif kind == "Actor_data":
            table.actors = parse_actors(body, source, symbol)
        else:
            table.scalars = [resolve(t, f"{source}:{symbol}[{i}]")
                             for i, t in enumerate(split_top_level(body))]
        tables.append(table)
    if not tables:
        raise SystemExit(f"abort: no table declarations found in {path}")
    return tables


def rust_int(value: int) -> str:
    return f"-{-value}" if value < 0 else str(value)


def emit(table: Table) -> str:
    lines = [f"    /// `{table.symbol}` from `{table.source}`."]
    ident = table.symbol.upper()
    if table.kind == "Scene_Word_u":
        lines.append(f"    pub const {ident}: &[SceneWord] = &[")
        for w in table.words:
            variant = "".join(part.title() for part in w.kind.split("_"))
            ptr = f'Some("{w.ptr}")' if w.ptr else "None"
            lines.append(
                f"        SceneWord {{ kind: SceneWordKind::{variant}, a: {w.a}, b: {w.b}, "
                f"c: {w.c}, d: {w.d}, packed: {w.packed}, ptr: {ptr} }},"
            )
        lines.append("    ];")
    elif table.kind == "Actor_data":
        lines.append(f"    pub const {ident}: &[ActorData] = &[")
        for profile, pos, rot, arg in table.actors:
            lines.append(
                f"        ActorData {{ profile: {profile}, "
                f"pos: [{pos[0]}, {pos[1]}, {pos[2]}], "
                f"rot: [{rot[0]}, {rot[1]}, {rot[2]}], arg: {arg} }},"
            )
        lines.append("    ];")
    else:
        lines.append(f"    pub const {ident}: &[i16] = &[")
        row: list[str] = []
        for value in table.scalars:
            row.append(rust_int(value))
            if len(row) == 10:
                lines.append("        " + ", ".join(row) + ",")
                row = []
        if row:
            lines.append("        " + ", ".join(row) + ",")
        lines.append("    ];")
    return "\n".join(lines)


def tokens(table: Table) -> list[int]:
    out: list[int] = []
    if table.kind == "Scene_Word_u":
        for w in table.words:
            out.extend([w.a, w.b, w.c, w.d, w.packed])
    elif table.kind == "Actor_data":
        for profile, pos, rot, arg in table.actors:
            out.extend([profile, *pos, *rot, arg])
    else:
        out.extend(table.scalars)
    return out


def fnv1a64(values: list[int]) -> int:
    digest = 0xCBF29CE484222325
    for value in values:
        for byte in (value & 0xFFFFFFFF).to_bytes(4, "little"):
            digest ^= byte
            digest = (digest * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return digest


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()

    PROFILES.update(load_actor_profiles())
    CONSTANTS.update(load_bank_ids())

    sources = sorted(SRC_DIR.glob("*.c"))
    if not sources:
        raise SystemExit("abort: no sources under src/data/scene")

    chunks: list[str] = []
    manifest: list[str] = []
    totals = {"tables": 0, "words": 0, "actors": 0, "scalars": 0}

    for path in sources:
        module = path.stem
        tables = parse_file(path)
        raw = path.read_bytes()
        rendered = "\n\n".join(emit(t) for t in tables)
        chunks.append(
            f"pub mod {module} {{\n    use super::{{ActorData, SceneWord, SceneWordKind}};\n\n{rendered}\n}}"
        )
        for table in tables:
            values = tokens(table)
            totals["tables"] += 1
            totals["words"] += len(table.words)
            totals["actors"] += len(table.actors)
            totals["scalars"] += len(table.scalars)
            manifest.append(
                f'[[file]]\npath = "{table.source}"\nsymbol = "{table.symbol}"\nkind = "{table.kind}"\n'
                f'source_bytes = {len(raw)}\nsource_sha256 = "{hashlib.sha256(raw).hexdigest()}"\n'
                f'token_count = {len(values)}\ntoken_fnv1a64 = "0x{fnv1a64(values):016x}"\n'
                f'token_sum = {sum(values)}\npointer_parity = "symbol-identity"\n'
            )

    header = [
        "// @generated by tools/generate_scene_data.py -- DO NOT EDIT.",
        "//",
        "// A relocatable Rust constant cannot hold a host address, so pointer-bearing",
        "// scene words record the target symbol name and store no address. The four",
        "// scalar bytes and the packed field-construct word are exact.",
        "//",
        "// Module identifiers mirror the C file stems, which are upper case. Allowing",
        "// non-snake-case keeps one generated module per source file, which the parity",
        "// gate checks by name.",
        "#![allow(non_snake_case)]",
        "",
        "/// Which `mSc_DATA_*` macro produced this word.",
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]",
        "pub enum SceneWordKind {",
        *[f"    {''.join(p.title() for p in kind.split('_'))}," for kind in (
            "sound", "end", "my_room", "arrange_room", "arrange_ftr", "field_ct",
            "player", "ctrl_actor", "actor", "obj_bank", "door_data",
        )],
        "}",
        "",
        "/// One `Scene_Word_u` entry. `a`/`b`/`c`/`d` mirror the four scalar fields;",
        "/// `packed` is the field-construct word; `ptr` is the referenced symbol.",
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]",
        "pub struct SceneWord {",
        "    pub kind: SceneWordKind,",
        "    pub a: i32,",
        "    pub b: i32,",
        "    pub c: i32,",
        "    pub d: i32,",
        "    pub packed: u32,",
        "    pub ptr: Option<&'static str>,",
        "}",
        "",
        "/// One `Actor_data` entry: profile id, position, rotation, ct argument.",
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]",
        "pub struct ActorData {",
        "    pub profile: i32,",
        "    pub pos: [i32; 3],",
        "    pub rot: [i32; 3],",
        "    pub arg: i32,",
        "}",
        "",
    ]

    generated = "\n".join(header) + "\n" + "\n\n".join(chunks) + "\n"
    manifest_text = (
        "# @generated by tools/generate_scene_data.py -- DO NOT EDIT.\n"
        '# source_glob = "src/data/scene/*.c"\n'
        f"# tables = {totals['tables']}\n"
        f"# scene_words = {totals['words']}\n"
        f"# actor_rows = {totals['actors']}\n"
        f"# scalar_values = {totals['scalars']}\n\n" + "\n".join(manifest)
    )

    out_path = ROOT / OUT_FILE
    manifest_path = ROOT / MANIFEST
    out_path.parent.mkdir(parents=True, exist_ok=True)

    root_text = (
        "// @generated by tools/generate_scene_data.py -- DO NOT EDIT.\n"
        "//\n"
        "// Module root for the scene-word tables. The tables themselves live in\n"
        "// `generated/scene_words.rs`, which is regenerated from `src/data/scene/*.c`.\n"
        "//\n"
        "// `rustfmt::skip` is deliberate: the emitter owns this file's layout, and\n"
        "// rustfmt would rewrite 380 table rows on every run, making the generator's\n"
        "// `--check` mode permanently stale.\n"
        "\n"
        "#[rustfmt::skip]\n"
        '#[path = "generated/scene_words.rs"]\n'
        "mod scene_words;\n"
        "\n"
        "pub use scene_words::*;\n"
    )
    root_path = ROOT / CRATE / "src/generated.rs"

    if args.check:
        stale = []
        if not out_path.exists() or out_path.read_text(encoding="utf-8") != generated:
            stale.append(str(out_path.relative_to(ROOT)))
        if not manifest_path.exists() or manifest_path.read_text(encoding="utf-8") != manifest_text:
            stale.append(str(manifest_path.relative_to(ROOT)))
        if not root_path.exists() or root_path.read_text(encoding="utf-8") != root_text:
            stale.append(str(root_path.relative_to(ROOT)))
        if stale:
            print("STALE: " + ", ".join(stale))
            return 1
        print(
            f"OK tables={totals['tables']} words={totals['words']} "
            f"actors={totals['actors']} scalars={totals['scalars']}"
        )
        return 0

    out_path.write_text(generated, encoding="utf-8")
    manifest_path.write_text(manifest_text, encoding="utf-8")
    root_path.write_text(root_text, encoding="utf-8")
    print(
        f"WROTE tables={totals['tables']} words={totals['words']} "
        f"actors={totals['actors']} scalars={totals['scalars']}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
