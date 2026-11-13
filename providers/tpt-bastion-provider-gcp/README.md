# tpt-bastion-provider-gcp

Google Cloud provider for TPT Bastion (Phase 6 — core resource coverage).

Implements the `ProviderPlugin` SDK with `google_storage_bucket` and a
`create`/`delete` lifecycle with no cloud calls. Real SDK calls are stubbed behind `// TODO(sdk)`.

See [`../README.md`](../README.md) for the status of all provider tracks.
