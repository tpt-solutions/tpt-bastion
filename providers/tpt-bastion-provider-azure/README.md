# tpt-bastion-provider-azure

Azure provider for TPT Bastion (Phase 6 — core resource coverage).

Implements the `ProviderPlugin` SDK with `azurerm_resource_group` and a
`create`/`delete` lifecycle with no cloud calls. Real SDK calls are stubbed behind `// TODO(sdk)`.

See [`../README.md`](../README.md) for the status of all provider tracks.
