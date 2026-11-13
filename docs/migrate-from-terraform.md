# Migrating from Terraform / OpenTofu

Bastion was built in response to HashiCorp's 2023 switch of Terraform from MPL
to BSL. If you're coming from Terraform or OpenTofu, this guide maps the
concepts you know onto Bastion and shows the interop paths that let you reuse
existing provider investments.

## Why migrate

- **License** — Bastion is `Apache-2.0 OR MIT`, forever. No BSL, no surprise
  relicensing.
- **Compile-time safety** — the DSL compiles to a versioned IR, so a large
  class of errors is caught before any infrastructure changes.
- **Owned state** — encrypted, versioned, lock-aware state is built in; no
  separate backend daemon required (pluggable to [TPT Keystone] later).
- **Reuse** — Bastion ships a Terraform-compatible provider shim (Phase 6) so
  you can keep using existing Terraform providers while you migrate.

## Concept map

| Terraform / OpenTofu        | Bastion                                      |
| --------------------------- | -------------------------------------------- |
| `main.tf` (HCL)             | `main.bcl` (Bastion DSL)                     |
| `resource "aws_s3_bucket"`  | `resource aws_s3_bucket`                     |
| `variable` / `output`       | `variable` / `output`                        |
| `terraform { backend }`     | native state file (`--state`) + encryption   |
| `terraform plan`            | `bastion plan`                               |
| `terraform apply`           | `bastion apply --auto-approve`               |
| `terraform state`           | `bastion state` / `bastion drift`            |
| `terraform init` (providers)| `bastion provider scaffold` / registry       |
| modules                     | `module` blocks (IR-level; full eval planned)|

## DSL vs HCL

The Bastion DSL is deliberately close to HCL so migration is mechanical:

```hcl
# Terraform
resource "aws_s3_bucket" "my_bucket" {
  bucket = "hello-bucket"
  tags = {
    environment = "dev"
  }
}
```

```bcl
# Bastion
resource aws_s3_bucket my_bucket {
    bucket = "hello-bucket"
    tags {
        environment = "dev"
    }
}
```

Differences to watch for:

- Attribute access uses `.` not `["..."]` subscripts: `my_bucket.bucket`.
- References are validated at compile time against the planned graph.
- Numbers are a single `f64` type today (no distinct integer type yet).

## Reusing Terraform providers

The Terraform-compatible shim (`providers/tpt-bastion-provider-terraform-shim`) adapts the Terraform
plugin gRPC protocol so Bastion can drive existing community providers. Point
Bastion at a provider binary and it is exposed through the same
`ProviderPlugin` interface as native providers:

```sh
bastion provider list
```

Provider schemas are described in data (`ProviderSchema`), so tooling can
validate and document a provider without executing it — the same model Terraform
uses for `terraform providers schema`.

## Converting configuration

`bastion import tf <dir>` converts each `.tf` file to a `.bcl` file, by default
under `<dir>/bastion/`:

```sh
bastion import tf ./infra
bastion validate ./infra/bastion/main.bcl
```

The converter is conservative. Anything it cannot translate faithfully is left
out and reported with its file and line number. Nothing is approximated. Typical
notes:

- `count` and `for_each` create several resources from one block. The whole
  resource is skipped, because one resource would describe something different
  from what gets created.
- Variable references (`var.*`) and string interpolation (`${...}`) are not
  supported yet. The attribute is skipped.
- Function calls, `locals`, `data`, `module`, `dynamic`, `lifecycle`, and
  `provisioner` are not supported yet.
- `variable` descriptions and `terraform` settings are dropped. Configure
  backends and providers in Bastion.

Every converted document is checked with the Bastion compiler before it is
written, so a converted file always compiles. Review every note before applying
the result. The converted output for the repository's example is in
`examples/migration/`, and its tests fail if the converter's output changes.

## State migration

`bastion import state <terraform.tfstate>` reads a version 4 state file and
writes a Bastion state file:

```sh
terraform show -json > /dev/null        # optional: confirm the state is readable
bastion import state terraform.tfstate --state infra.bclstate --passphrase-env BASTION_PASS
```

- Each managed resource with one instance becomes a Bastion resource with the
  same `type.name` id, so plans diff cleanly against the converted document.
- Resources with several instances (from `count` or `for_each`) are reported and
  not imported, because the indexed ids are not modelled yet.
- Data sources are read-only and are not imported.
- The command refuses to overwrite an existing state file unless `--force` is
  given.

Imported state records what Terraform reported. It does not check the real
infrastructure, so run `bastion refresh` once the provider reads are implemented
(see [Status](status.md)) before relying on it.

## Rollout strategy

A safe migration runs Bastion **alongside** Terraform in read/plan mode first:

1. Port a non-critical module to `.bcl`.
2. `bastion plan` and compare against `terraform plan`.
3. Use the Terraform shim to keep shared providers working.
4. Cut over `apply` for that module, then repeat.

[TPT Keystone]: https://bastion.tpt.dev/keystone
