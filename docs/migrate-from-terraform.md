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

## State migration

Bastion state is a self-describing, encrypted file. To migrate:

1. Export current Terraform state to JSON (`terraform show -json`).
2. Map resource addresses to Bastion `type.name` ids.
3. Import into a Bastion state file (`bastion state` tooling), preserving the
   resource ids so future plans diff cleanly.

> Note: automated Terraform→Bastion state import is tracked for a later release.
> The native state format and `from`/`into` conversions between
> `tpt_bastion_state::State` and `tpt_bastion_engine::State` already make the in-memory
> mapping straightforward.

## Rollout strategy

A safe migration runs Bastion **alongside** Terraform in read/plan mode first:

1. Port a non-critical module to `.bcl`.
2. `bastion plan` and compare against `terraform plan`.
3. Use the Terraform shim to keep shared providers working.
4. Cut over `apply` for that module, then repeat.

[TPT Keystone]: https://bastion.tpt.dev/keystone
