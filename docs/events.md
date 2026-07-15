# SPDX-License-Identifier: Apache-2.0 OR MIT

# Event-driven updates (Phase 7)

Bastion can reconcile infrastructure when external events fire. This crate
provides the pluggable model and the reconcile-on-event loop; the `bastion
events` CLI subcommand exposes it.

## Model

- `InfraEvent` — `FileChanged`, `Webhook`, `Tick`.
- `EventSource` — the pluggable trait an event backend implements. Ships with
  `FileEventSource` (watch files) and `WebhookEventSource` (accept HTTP POSTs).
  A future [TPT Flux] backend implements the same trait.
- `run_forever` / `run_with_limit` — call your handler for every event so
  infrastructure is brought back in line as the world changes.

## CLI

```sh
# Re-plan whenever main.bcl (or a watched file) changes:
bastion events watch main.bcl --watch ./modules --apply

# Listen for webhooks and re-apply on each POST:
bastion events webhook main.bcl --port 8080 --apply
```

On each event the CLI re-runs `bastion plan` (or `bastion apply --auto-approve`
with `--apply`) for the document, reusing the document's state file.

## Extending

Implement `EventSource` for any backend — a Git poller, a cloud control-plane
stream, or TPT Flux — and pass it to `run_forever`. No engine changes required.

[TPT Flux]: https://bastion.tpt.dev/flux
