#!/usr/bin/env python3
"""
从 WeaveRiftAgent.java 里的硬编码常量生成 versions/*.json。

用法：
  python gen_versions.py > ../src/main/resources/versions/1.12.2.json

设计原则：
  - 单一真相来源：硬编码常量 → 脚本 → JSON
  - JSON key 用 SRG 名（不解耦，但简单）
  - 加字段只改脚本 + 常量，不手打 JSON

注意：
  - srg 名必须从 obf2srg.srg 校验，脚本不负责校验
  - sig 字段是占位符，Step 4 时用真实签名替换
"""

import re
import json
import sys
from pathlib import Path

JAVA_PATH = Path(__file__).parent.parent / "src" / "main" / "java" / \
            "com" / "fastnow" / "weaverift" / "WeaveRiftAgent.java"

def extract_string_constants(java_src: str):
    """从 Java 源码里提取 findField/findMethod/invokeStatic 的路径字符串。"""
    fields = set()
    methods = set()

    for m in re.finditer(r'findField\([^,]+,\s*"([^"]+)"\)', java_src):
        fields.add(m.group(1))

    for m in re.finditer(r'(?:findMethod|invokeStatic)\([^,]+,\s*"([^"]+)"\)', java_src):
        methods.add(m.group(1))

    return fields, methods

TYPE_MAP = {
    "field_71439_g":   "EntityPlayerSP",
    "field_71441_e":   "World",
    "field_70165_t":   "double",
    "field_70163_u":   "double",
    "field_70161_v":   "double",
    "field_70177_z":   "float",
    "field_70125_A":   "float",
    "field_72996_f":   "java.util.List",
}

SIG_MAP = {
    "func_71410_x":    "()Lnet/minecraft/client/Minecraft;",
    "func_110143_aJ":  "()F",
    "func_110138_aP":  "()F",
}

OWNER_MAP = {
    "net/minecraft/client/Minecraft":              "Minecraft",
    "net/minecraft/entity/Entity":                 "Entity",
    "net/minecraft/entity/EntityLivingBase":       "EntityLivingBase",
    "net/minecraft/entity/player/EntityPlayer":    "EntityPlayer",
    "net/minecraft/client/entity/EntityPlayerSP":  "EntityPlayerSP",
    "net/minecraft/world/World":                   "World",
}

def owner_of(path: str) -> str:
    owner_full, _ = path.rsplit('/', 1)
    return OWNER_MAP.get(owner_full, owner_full)

def build_json(fields: set, methods: set) -> dict:
    classes = {
        "Minecraft":        {"srg": "net/minecraft/client/Minecraft",  "obf": "bib"},
        "Entity":           {"srg": "net/minecraft/entity/Entity",     "obf": "vg"},
        "EntityLivingBase": {"srg": "net/minecraft/entity/EntityLivingBase"},
        "EntityPlayer":     {"srg": "net/minecraft/entity/player/EntityPlayer"},
        "EntityPlayerSP":   {"srg": "net/minecraft/client/entity/EntityPlayerSP"},
        "World":            {"srg": "net/minecraft/world/World"},
    }

    fields_out = {}
    for path in sorted(fields):
        owner_full, srg = path.rsplit('/', 1)
        owner = owner_of(path)
        key = f"{owner}.{srg}"
        fields_out[key] = {
            "owner": owner,
            "srg": srg,
            "type": TYPE_MAP.get(srg, "unknown"),
        }

    methods_out = {}
    for path in sorted(methods):
        owner_full, srg = path.rsplit('/', 1)
        owner = owner_of(path)
        key = f"{owner}.{srg}"
        methods_out[key] = {
            "owner": owner,
            "srg": srg,
            "sig": SIG_MAP.get(srg, "()V"),
        }

    return {
        "schema_version": 2,
        "mc_version": "1.12.2",
        "namespace": "srg",
        "classes": classes,
        "fields": fields_out,
        "methods": methods_out,
        "probe": {
            "class": "Minecraft",
            "discriminator_field": "Minecraft.field_71439_g",
        },
    }

def main():
    if not JAVA_PATH.exists():
        print(f"错误：找不到 {JAVA_PATH}", file=sys.stderr)
        sys.exit(1)

    src = JAVA_PATH.read_text(encoding="utf-8")
    fields, methods = extract_string_constants(src)

    if not fields and not methods:
        print("警告：没提取到任何字段/方法", file=sys.stderr)

    config = build_json(fields, methods)
    print(json.dumps(config, indent=2, ensure_ascii=False))

if __name__ == "__main__":
    main()