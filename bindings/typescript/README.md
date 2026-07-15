# Bastion TypeScript bindings

Author TPT Bastion infrastructure in TypeScript. A `BastionDocument` mirrors the
stable Bastion IR; `toBcl()` renders it to `.bcl` source and the `compile` /
`plan` / `apply` helpers drive the `bastion` CLI against that source.

```ts
import { str, bool, ref, map, toBcl, type BastionDocument } from "bastion";

const doc: BastionDocument = {
  config: { name: "hello-bastion", region: "us-east-1" },
  resources: [
    {
      typeName: "aws_s3_bucket",
      name: "my_bucket",
      attributes: {
        bucket: str("hello-bastion-bucket"),
        acl: str("private"),
        versioning: bool(true),
        tags: map({ environment: str("dev") }),
      },
    },
  ],
  outputs: [{ name: "bucket_name", value: ref("my_bucket", "bucket") }],
};

console.log(toBcl(doc));
```

## Running

Node >= 22.6 can run the TypeScript directly (no build step):

```sh
node --experimental-strip-types examples/aws_s3.ts
```

To drive real infrastructure, build the CLI (`cargo install --path
crates/bastion-cli`) and call `plan(doc)` / `apply(doc)` from the bindings — they
shell out to `bastion` with the generated `.bcl`.

## Layout

- `src/index.ts` — types, `toBcl`/`toIr`/`toIrJson` serializers, and the CLI
  runners.
- `examples/aws_s3.ts` — a full example document.
- `../python` — the equivalent Python package.
- `../../docs/bindings.md` — the binding architecture decision.
