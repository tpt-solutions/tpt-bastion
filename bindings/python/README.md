# Bastion Python bindings

Author TPT Bastion infrastructure in Python. A `BastionDocument` mirrors the
stable Bastion IR; `to_bcl()` renders it to `.bcl` source and the `compile` /
`plan` / `apply` helpers drive the `bastion` CLI against that source.

```python
from bastion import BastionDocument, s, b, ref, mp, to_bcl

doc = BastionDocument(
    config={"name": "hello-bastion", "region": "us-east-1"},
    resources=[{
        "type_name": "aws_s3_bucket",
        "name": "my_bucket",
        "attributes": {
            "bucket": s("hello-bastion-bucket"),
            "acl": s("private"),
            "versioning": b(True),
            "tags": mp(environment=s("dev")),
        },
    }],
    outputs=[{"name": "bucket_name", "value": ref("my_bucket", "bucket")}],
)

print(to_bcl(doc))
```

## Running

Requires Python >= 3.9 (no native build step):

```sh
python examples/aws_s3.py
```

To drive real infrastructure, install the CLI (`cargo install --path
crates/tpt-bastion-cli`) and call `plan(doc)` / `apply(doc)` from the bindings — they
shell out to `bastion` with the generated `.bcl`.

## Layout

- `bastion/__init__.py` — types, `to_bcl`/`to_ir`/`to_ir_json` serializers, and
  the CLI runners.
- `examples/aws_s3.py` — a full example document.
- `../typescript` — the equivalent TypeScript package.
- `../../docs/bindings.md` — the binding architecture decision.
