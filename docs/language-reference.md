# Bastion DSL Language Reference (`.bcl`)

This document is the authoritative reference for the Bastion configuration
language, filename extension **`.bcl`**. It is kept in sync with the compiler in
`crates/bastion-dsl` (lexer → parser → compiler → `bastion-ir`).

Bastion configs are declarative: you describe the desired infrastructure, and
the toolchain compiles the document to a stable intermediate representation
(IR) that the execution engine plans and applies. The compiler catches a large
class of errors *before* any infrastructure is touched.

> Status: this reference reflects the compiler shipped in Phase 1. Features
> planned for later phases (modules with full evaluation, functions, `for_each`
> expressions, etc.) are noted inline but are **not yet implemented**.

---

## 1. Lexical structure

### 1.1 Identifiers

An identifier is a sequence of ASCII letters, digits, and underscores, starting
with a letter or underscore:

```
letter | _  { letter | digit | _ }
```

Identifiers are used for block kinds, labels, attribute keys, and the bases of
references. Keywords are ordinary identifiers and are only meaningful in
specific positions (e.g. `true`, `false`, `null` as values; block kinds such as
`resource` at the start of a top-level block).

### 1.2 Literals

| Kind       | Syntax                                              | IR value              |
|------------|-----------------------------------------------------|-----------------------|
| String     | `"..."` with escapes `\" \\ \n \t \r`               | `Value::String`       |
| Number     | integer or decimal digits, e.g. `42`, `3.14`        | `Value::Number(f64)`  |
| Boolean    | `true` / `false`                                    | `Value::Bool`         |
| Null       | `null`                                              | `Value::Null`         |
| List       | `[ a, b, c ]`                                        | `Value::List`         |
| Object     | `{ key = val ... }` inside an attribute            | `Value::Map`          |

Numbers are stored as a single `f64` representation; integers and floats share
the same type. There is no separate integer type yet.

### 1.3 Comments

- Line comment: `// ...` to end of line.
- Block comment: `/* ... */`, which must be terminated. Unterminated block
  comments and a stray `/` are lexical errors.

### 1.4 Whitespace and separators

Whitespace separates tokens. Attribute entries inside a block body may be
separated by optional `,` or `;` — or simply by whitespace. The closing brace
of a body ends the block; a trailing separator before it is allowed.

---

## 2. Document structure

A `.bcl` document is a sequence of **top-level blocks**. Blocks are compiled in
source order. The recognized block kinds are:

| Block     | Labels required        | Purpose                                   |
|-----------|------------------------|-------------------------------------------|
| `config`  | none                   | Project metadata (name, region, extras)   |
| `variable`| `name`                 | Declared input variable                   |
| `resource`| `type` then `name`     | A unit of managed infrastructure          |
| `module`  | `name`                 | A module instantiation                    |
| `output`  | `name`                 | A derived value exposed after apply       |

Any other leading identifier is a **compile error** (`unknown top-level block`).

Each block opens with `{` and closes with `}` and contains an ordered list of
attributes of two forms:

```
key = <expression>        # value attribute
key { ... }               # nested object attribute (shorthand for key = { ... })
```

Duplicate attribute keys within a block (or duplicate object keys) are compile
errors. Duplicate `resource` ids of the form `type.name` are compile errors.

---

## 3. Block reference

### 3.1 `config`

Optional, at most one per document (a second is a compile error). Commonly used
keys:

```bcl
config {
  name   = "bastion-demo"
  region = "us-east-1"
  // any other keys are stored verbatim under Config.extra
}
```

`name` and `region` are recognized fields; all other keys are preserved in the
IR `Config.extra` map.

### 3.2 `variable`

```bcl
variable region {
  type    = "string"
  default = "us-east-1"
}
```

- `type` — optional string naming the variable's type. Validation of the type
  against `default` / usages is **not yet implemented**.
- `default` — optional value used when no input is supplied.
- A `variable` block with no `name` label is a compile error.

### 3.3 `resource`

```bcl
resource <type> <name> {
  <attribute> = <expression>
  ...
}
```

- `type` is the provider-scoped resource type (e.g. `aws_s3_bucket`).
- `name` is the local name; together they form the id `type.name`.
- Both labels are required; a missing type or name is a compile error.
- Each attribute is a literal or a **reference** to another object's value.

Example:

```bcl
resource aws_s3_bucket my_bucket {
  bucket = "bastion-demo-bucket"
  acl    = "private"
  tags {
    environment = "dev"
    team        = "platform"
  }
  versioning = true
  count      = 3
}
```

> Note: `count` / `for_each` (Terraform-style multi-instantiation) are parsed as
> ordinary attributes today and are **not yet** expanded into multiple
> resources by the compiler or engine.

### 3.4 `module`

```bcl
module vpc {
  source = "./modules/vpc"
  cidr   = "10.0.0.0/16"
}
```

- `source` (optional string) points at the module definition.
- Other attributes are inputs passed to the module.
- Module expansion/evaluation is **not yet implemented**; the block is carried
  through to the IR unchanged.

### 3.5 `output`

```bcl
output bucket_name {
  value = my_bucket.bucket
}
```

- `name` label is required.
- `value` attribute is required; a missing `value` is a compile error.
- The value is typically a reference, but may be any expression.

---

## 4. Expressions and references

Expressions appear on the right-hand side of `key = ...` and as list/object
elements. The grammar:

```
expr := string | number | "true" | "false" | "null"
      | reference
      | "[" expr ("," expr)* "]"
      | "{" ident "=" expr ... "}"

reference := ident ("." ident)*
```

A **reference** names another object's attribute and is left *unevaluated* in
the IR as `Value::Reference { base, path }`. References enable:

- **Compile-time validation** that the referenced base object exists (planned).
- **Dependency-graph construction** downstream: the execution engine reads
  references to order resource creation.

Examples:

```bcl
output bucket_name { value = my_bucket.bucket }   # base=my_bucket path=[bucket]
output bucket_arn  { value = my_bucket.arn }      # base=my_bucket path=[arn]
```

References may also be nested in lists and objects:

```bcl
output all_arns {
  value = [ my_bucket.arn, replica.arn ]
}
```

`base` is the local name of a `resource` or `module` (or, planned, a `variable`
or another `output`). `path` is the attribute chain after the base.

> Cross-document and provider-data-source references (`data.*`) are **not yet
> supported**.

---

## 5. Canonical example

```bcl
// SPDX-License-Identifier: Apache-2.0 OR MIT

config {
  name   = "bastion-demo"
  region = "us-east-1"
}

variable region {
  type    = "string"
  default = "us-east-1"
}

resource aws_s3_bucket my_bucket {
  bucket = "bastion-demo-bucket"
  acl    = "private"
  tags {
    environment = "dev"
    team        = "platform"
  }
  versioning = true
  count      = 3
}

resource aws_s3_bucket replica {
  bucket = "bastion-demo-bucket-replica"
  acl    = "private"
  tags {
    environment = "dev"
  }
}

output bucket_name {
  value = my_bucket.bucket
}

output bucket_arn {
  value = my_bucket.arn
}
```

Compiling this document:

```sh
cargo run --bin bastion -- compile examples/aws_s3.bcl
```

emits the `bastion-ir` `Document` as pretty JSON, with `ir_version` recorded so
downstream consumers can gate on compatibility.

---

## 6. Compile-time error model

All errors are reported with a phase and (for lex/parse) a `line:col`:

- **Lex errors** — unexpected characters, unterminated strings/block comments,
  invalid escape sequences.
- **Parse errors** — malformed structure (unexpected token, missing `{`/`}`).
- **Compile errors** — semantic problems: unknown block kind, missing labels,
  duplicate `resource` ids, duplicate attribute/object keys, `output` missing
  `value`.

The CLI surfaces the most relevant diagnostic and exits non-zero on failure.

---

## 7. Roadmap relationships

This reference documents the surface implemented in **Phase 1**. The following
are tracked in `todo.md` and will extend this grammar:

- Phase 2 — TypeScript/Python bindings (consume IR, not the DSL directly).
- Phase 3 — Execution engine: references drive a dependency graph; `plan`/`apply`
  and `dry-run` operate on the IR.
- Phase 4 — Native state management is implemented in the `bastion-state` crate:
  encrypted state files (AES-256-GCM + Argon2id), append-only version history,
  cooperative locking, drift detection, and automatic reconciliation. The CLI
  exposes it via `bastion state` and `bastion drift`.
- Phase 5 — Provider plugin system is implemented in the `bastion-providers`
  crate: the `ProviderPlugin` SDK/ABI, a capability schema format, a
  pluggable runtime (`NativeRuntime` in-process and `SubprocessRuntime` for
  isolated processes), a `PluginRegistry`, scaffolding, and a conformance
  suite. The CLI exposes it via `bastion provider`.
- Phase 6 — Cloud providers (AWS, GCP, Azure, Kubernetes, Terraform shim).
- Phase 7 — Event-driven updates (internal event source/trigger interface).
- Phase 8 — Testing & quality: provider conformance suite, end-to-end
  plan/apply scenarios, and fuzz/property tests for the DSL compiler and IR
  (CI now reports coverage).
