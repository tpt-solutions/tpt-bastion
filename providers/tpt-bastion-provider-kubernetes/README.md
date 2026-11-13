# tpt-bastion-provider-kubernetes

Kubernetes provider for TPT Bastion (Phase 6 — core resource coverage).

Implements the `ProviderPlugin` SDK with `kubernetes_namespace` and a
`create`/`delete` lifecycle with no cloud calls. Real `kube` calls are stubbed behind `// TODO(sdk)`.

See [`../README.md`](../README.md) for the status of all provider tracks.
