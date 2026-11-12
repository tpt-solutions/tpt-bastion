# tpt-bastion-provider-kubernetes

Kubernetes provider for TPT Bastion (Phase 6 — core resource coverage).

Implements the `ProviderPlugin` SDK with `kubernetes_namespace` and a working
`create`/`delete` lifecycle. Real `kube` calls are stubbed behind `// TODO(sdk)`.

See [`../README.md`](../README.md) for the status of all provider tracks.
