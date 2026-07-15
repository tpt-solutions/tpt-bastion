# bastion-provider-terraform-shim

Terraform / OpenTofu-compatible provider shim for TPT Bastion (Phase 6).

Lets Bastion drive existing Terraform providers today: each resource is rendered
to an HCL `resource` block and applied via `terraform`/`tofu` (override the
binary with `BASTION_TF_BIN`). The permissive schema accepts any upstream
resource type, so coverage grows as you install providers.

The long-term path is to speak the Terraform plugin gRPC protocol directly, so
Bastion can consume provider schemas natively without a `terraform` binary.

See [`../README.md`](../README.md) for the status of all provider tracks.
