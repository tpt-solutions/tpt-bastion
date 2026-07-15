# Language Bindings (Phase 2)

Bastion's value is the stable, versioned **IR**. Every consumer — the DSL
compiler, the engine, state backends, and providers — speaks IR. Language
bindings should too, so there is one source of truth and no divergent semantic
models.

## Decision: bindings target the IR, not HCL

Two options were on the table:

1. **FFI / Wasm** — embed the Rust engine in the host language (e.g. a Wasm
   build of `bastion-engine`, or `cbindgen` + a native addon). Highest fidelity,
   but each language needs a build/runtime shim and the engine must be
   Wasm-friendly today.
2. **Codegen from IR + thin CLI client** *(chosen)* — bindings construct a
   `BastionDocument` (typed mirror of `bastion_ir::Document`), render it to
   `.bcl` source (or IR JSON), and drive the `bastion` CLI. No native build step,
   works everywhere the CLI runs, and stays correct as the engine evolves because
   it goes through the real compiler.

We chose **(2)** as the primary, shipped path because it is portable, testable,
and requires no per-language native toolchain. The IR `Document` shape is the
contract; if a future Wasm build of the engine lands, the same `BastionDocument`
types can serialize straight to IR JSON and call it with zero changes to user
code.

## What the bindings provide

Each binding (`bindings/typescript`, `bindings/python`) exposes:

- **Typed document model** — `BastionDocument` plus value constructors
  (`s`/`str`, `n`/`num`, `b`/`bool`, `ref`, `lst`/`list`, `mp`/`map`).
- **Serializers** — `toBcl`/`to_bcl` (`.bcl` source) and
  `toIrJson`/`to_ir_json` (IR JSON, the portable artifact).
- **CLI runners** — `compile` / `plan` / `apply` that shell out to `bastion`
  with the generated document, supporting `--state`, `--passphrase-env`, and
  `--auto-approve`.

## TypeScript

```ts
import { str, bool, ref, map, toBcl, type BastionDocument } from "bastion";

const doc: BastionDocument = {
  config: { name: "hello-bastion", region: "us-east-1" },
  resources: [{
    typeName: "aws_s3_bucket",
    name: "my_bucket",
    attributes: { bucket: str("x"), acl: str("private"), versioning: bool(true),
                  tags: map({ environment: str("dev") }) },
  }],
  outputs: [{ name: "bucket_name", value: ref("my_bucket", "bucket") }],
};

console.log(toBcl(doc));
```

Run with native TypeScript (Node >= 22.6): `node --experimental-strip-types
examples/aws_s3.ts`. See `bindings/typescript/README.md`.

## Python

```python
from bastion import BastionDocument, s, b, ref, mp, to_bcl

doc = BastionDocument(
    config={"name": "hello-bastion", "region": "us-east-1"},
    resources=[{"typeName": "aws_s3_bucket", "name": "my_bucket",
                "attributes": {"bucket": s("x"), "versioning": b(True),
                               "tags": mp(environment=s("dev"))}}],
    outputs=[{"name": "bucket_name", "value": ref("my_bucket", "bucket")}],
)
print(to_bcl(doc))
```

See `bindings/python/README.md`.

## Examples

- `examples/aws_s3.bcl` — the DSL original.
- `examples/aws_s3.ts` — the TypeScript equivalent.
- `examples/aws_s3.py` — the Python equivalent.

All three describe the same infrastructure and compile through the same engine.

## Publishing

Both packages are publish-ready and ship through dedicated, tag-triggered
workflows (no secrets stored in the repo):

- **npm** — push a `bindings-typescript-v*.*.*` tag to run
  `.github/workflows/publish-npm.yml`, which publishes with npm provenance
  (`npm publish --provenance --access public`).
- **PyPI** — push a `bindings-python-v*.*.*` tag to run
  `.github/workflows/publish-pypi.yml`, which builds the sdist/wheel and uploads
  via PyPI Trusted Publishing (OIDC). Configure a trusted publisher in the PyPI
  `bastion` project pointing at this workflow + `release` environment.

The version is taken from the tag, so a release is just `git tag bindings-typescript-v0.1.0 && git push --tags`.
