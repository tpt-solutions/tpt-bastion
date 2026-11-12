# SPDX-License-Identifier: Apache-2.0 OR MIT
"""TPT Bastion — Python bindings.

Author Bastion infrastructure in Python. A :class:`BastionDocument` mirrors the
stable Bastion IR; :func:`to_bcl` renders it to ``.bcl`` source and the
``compile`` / ``plan`` / ``apply`` helpers drive the ``bastion`` CLI against that
source. This targets the *same* artifact (IR) as the DSL and the Rust engine.

>>> from bastion import BastionDocument, s, b, ref, mp, to_bcl
>>> doc = BastionDocument(
...     config={"name": "hello", "region": "us-east-1"},
...     resources=[{"type_name": "aws_s3_bucket", "name": "b",
...                 "attributes": {"bucket": s("x"), "versioning": b(True),
...                                "tags": mp(environment=s("dev"))}}],
...     outputs=[{"name": "bucket_name", "value": ref("b", "bucket")}],
... )
>>> print(to_bcl(doc))
"""

from __future__ import annotations

import json
import os
import subprocess
import tempfile
from dataclasses import dataclass, field
from typing import Any, Optional

__all__ = [
    "BastionDocument",
    "s",
    "n",
    "b",
    "nil",
    "ref",
    "lst",
    "mp",
    "to_bcl",
    "to_ir",
    "to_ir_json",
    "compile",
    "plan",
    "apply",
]


def s(value: str) -> dict:
    """A string value."""
    return {"kind": "string", "value": value}


def n(value: float) -> dict:
    """A numeric value."""
    return {"kind": "number", "value": value}


def b(value: bool) -> dict:
    """A boolean value."""
    return {"kind": "bool", "value": value}


def nil() -> dict:
    """The explicit null value."""
    return {"kind": "null"}


def ref(base: str, *path: str) -> dict:
    """A reference to another object's attribute, e.g. ``my_bucket.bucket``."""
    return {"kind": "reference", "base": base, "path": list(path)}


def lst(*items: dict) -> dict:
    """An ordered list value."""
    return {"kind": "list", "value": list(items)}


def mp(**items: dict) -> dict:
    """An ordered map value (renders as a nested ``.bcl`` block)."""
    return {"kind": "map", "value": dict(items)}


@dataclass
class BastionDocument:
    """A full Bastion document, mirroring ``tpt_bastion_ir::Document``."""

    config: Optional[dict] = None
    variables: list[dict] = field(default_factory=list)
    resources: list[dict] = field(default_factory=list)
    modules: list[dict] = field(default_factory=list)
    outputs: list[dict] = field(default_factory=list)


def _render_value(v: dict) -> str:
    kind = v["kind"]
    if kind == "string":
        escaped = (
            v["value"]
            .replace("\\", "\\\\")
            .replace('"', '\\"')
            .replace("\n", "\\n")
            .replace("\t", "\\t")
            .replace("\r", "\\r")
        )
        return f'"{escaped}"'
    if kind == "number":
        return str(v["value"])
    if kind == "bool":
        return "true" if v["value"] else "false"
    if kind == "null":
        return "null"
    if kind == "reference":
        return ".".join([v["base"], *v["path"]])
    if kind == "list":
        return "[ " + ", ".join(_render_value(i) for i in v["value"]) + " ]"
    if kind == "map":
        inner = ", ".join(f"{k} = {_render_value(val)}" for k, val in v["value"].items())
        return "{ " + inner + " }"
    raise ValueError(f"unknown value kind: {kind}")


def _render_body(attrs: dict[str, dict]) -> str:
    lines: list[str] = []
    for key, value in attrs.items():
        if value["kind"] == "map":
            lines.append(f"    {key} {{")
            for k, v in value["value"].items():
                lines.append(f"        {k} = {_render_value(v)}")
            lines.append("    }")
        else:
            lines.append(f"    {key} = {_render_value(value)}")
    return "\n".join(lines)


def to_bcl(doc: BastionDocument) -> str:
    """Render a :class:`BastionDocument` to ``.bcl`` source text."""
    blocks: list[str] = []

    if doc.config:
        body: dict[str, dict] = {}
        if doc.config.get("name") is not None:
            body["name"] = s(doc.config["name"])
        if doc.config.get("region") is not None:
            body["region"] = s(doc.config["region"])
        for k, v in doc.config.get("extra", {}).items():
            body[k] = v
        blocks.append(f"config {{\n{_render_body(body)}\n}}")

    for v in doc.variables:
        body = {}
        if v.get("typeName"):
            body["type"] = s(v["typeName"])
        if v.get("default") is not None:
            body["default"] = v["default"]
        blocks.append(f"variable {v['name']} {{\n{_render_body(body)}\n}}")

    for r in doc.resources:
        blocks.append(
            f"resource {r['typeName']} {r['name']} {{\n{_render_body(r['attributes'])}\n}}"
        )

    for m in doc.modules:
        body = {}
        if m.get("source"):
            body["source"] = s(m["source"])
        body.update(m.get("attributes", {}))
        blocks.append(f"module {m['name']} {{\n{_render_body(body)}\n}}")

    for o in doc.outputs:
        blocks.append(f"output {o['name']} {{\n    value = {_render_value(o['value'])}\n}}")

    return "\n\n".join(blocks) + "\n"


def to_ir(doc: BastionDocument) -> dict:
    """Convert a document into the IR JSON shape."""
    return {
        "ir_version": 1,
        "config": (
            {
                "name": doc.config.get("name") if doc.config else None,
                "region": doc.config.get("region") if doc.config else None,
                "extra": doc.config.get("extra", {}) if doc.config else {},
            }
            if doc.config
            else None
        ),
        "variables": [
            {
                "name": v["name"],
                "type_name": v.get("typeName"),
                "default": v.get("default"),
            }
            for v in doc.variables
        ],
        "resources": [
            {
                "id": f"{r['typeName']}.{r['name']}",
                "type_name": r["typeName"],
                "name": r["name"],
                "attributes": r["attributes"],
            }
            for r in doc.resources
        ],
        "modules": [
            {
                "name": m["name"],
                "source": m.get("source"),
                "attributes": m.get("attributes", {}),
            }
            for m in doc.modules
        ],
        "outputs": [{"name": o["name"], "value": o["value"]} for o in doc.outputs],
    }


def to_ir_json(doc: BastionDocument) -> str:
    """Serialize a document to IR JSON (the portable artifact)."""
    return json.dumps(to_ir(doc), indent=2)


def _run(bin_name: str, args: list[str], cwd: Optional[str]) -> dict:
    proc = subprocess.run(
        [bin_name, *args],
        cwd=cwd,
        capture_output=True,
        text=True,
    )
    return {"stdout": proc.stdout, "stderr": proc.stderr, "status": proc.returncode}


def compile(doc: BastionDocument, opts: Optional[dict] = None) -> dict:
    """Compile a document to IR JSON via the CLI."""
    opts = opts or {}
    bin_name = opts.get("bastion_bin", "bastion")
    cwd = opts.get("cwd")
    with tempfile.TemporaryDirectory(prefix="bastion-") as d:
        path = os.path.join(d, "doc.bcl")
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(to_bcl(doc))
        return _run(bin_name, ["compile", path], cwd)


def plan(doc: BastionDocument, opts: Optional[dict] = None) -> dict:
    """Build an execution plan (dry-run)."""
    opts = opts or {}
    bin_name = opts.get("bastion_bin", "bastion")
    cwd = opts.get("cwd")
    args = ["plan"]
    if opts.get("state"):
        args += ["--state", opts["state"]]
    if opts.get("passphrase_env"):
        args += ["--passphrase-env", opts["passphrase_env"]]
    with tempfile.TemporaryDirectory(prefix="bastion-") as d:
        path = os.path.join(d, "doc.bcl")
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(to_bcl(doc))
        return _run(bin_name, ["plan", path, *args[1:]], cwd)


def apply(doc: BastionDocument, opts: Optional[dict] = None) -> dict:
    """Apply a document, creating/updating infrastructure."""
    opts = opts or {}
    bin_name = opts.get("bastion_bin", "bastion")
    cwd = opts.get("cwd")
    args = ["apply"]
    if opts.get("auto_approve"):
        args.append("--auto-approve")
    if opts.get("state"):
        args += ["--state", opts["state"]]
    if opts.get("passphrase_env"):
        args += ["--passphrase-env", opts["passphrase_env"]]
    with tempfile.TemporaryDirectory(prefix="bastion-") as d:
        path = os.path.join(d, "doc.bcl")
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(to_bcl(doc))
        return _run(bin_name, ["apply", path, *args[1:]], cwd)
