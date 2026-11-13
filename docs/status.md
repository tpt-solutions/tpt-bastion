# Status

This page records what TPT Bastion does today, feature by feature. It is the
reference to check before relying on a feature. The roadmap in
[`todo.md`](https://github.com/tpt/bastion/blob/master/todo.md) lists what is
planned.

| Area | Status | What it does today | Not yet |
| --- | --- | --- | --- |
| DSL to IR compiler | Working | Lexes, parses, and compiles `.bcl` to a versioned IR, with line/column diagnostics. | Source-span snippets, "did you mean", compile-time reference validation, `count` / `for_each`, modules. |
| Dependency graph | Working | Orders resources from structured references; reports cycles and unknown references. | Graph export beyond `bastion graph` (DOT and Mermaid). |
| Plan | Working | Diffs declared attributes against recorded state and reports create, update, delete, and no-op. | `Replace` actions. Detecting a removed attribute (state does not record which attributes were declared). Colored output and saved plan files. |
| Apply | Mock provider only | The CLI runs the engine with an in-memory mock provider. The engine creates, updates, and destroys resources and rolls back on failure. | Real provider calls from the CLI. Provider configuration (region, credentials). |
| Native state | Working | Plaintext or encrypted (Argon2id key derivation, AES-256-GCM) state, with history and a lock. | External state backends. |
| Drift detection | Plan time only | Compares the document with recorded state. | Live refresh from infrastructure. The provider `read` operation exists but drift detection does not call it. |
| Provider plugin ABI | Working (subprocess) | Providers run as separate processes that speak line-delimited JSON over stdio. The child gets resource limits; on Linux it also gets a read-only landlock policy. | The Linux landlock path is not yet covered by a passing runtime test. macOS and Windows get no filesystem confinement. |
| Wasm provider runtime | Not built | Nothing. Wasm is a possible future transport. | Everything. |
| AWS, GCP, Azure, Kubernetes providers | Stubs | Schemas and input validation. Creates and deletes do not call the cloud APIs. Computed attributes such as ids and ARNs are generated locally. | Real SDK calls. Do not use these providers to manage real infrastructure. |
| Terraform shim | Experimental | Renders HCL and runs `terraform` or `tofu` in a per-resource directory that keeps Terraform's state. `delete` refuses to report success when there is no state to destroy. | Verified runs against a real Terraform install in CI. |
| Events | Working | File-change and authenticated webhook triggers that run plan or apply. | Other event backends (TPT Flux). |
| Dashboard (`bastion ui`) | Working | Localhost-only web view of compile, plan, state, and drift. Apply is off unless enabled, and it rejects cross-origin requests. | Editing from the browser. Views beyond compile, plan, state, and drift. |
| Language bindings | TypeScript and Python | Compile DSL to IR from the host language. | Publishing depends on the release workflow. |

If a row says "stub" or "mock", the feature exists in code but does not change
real infrastructure.
