# tpt-bastion-ui

A local web dashboard for [TPT Bastion][bastion]: compile, plan, state, history,
and drift visualization in the browser.

`tpt-bastion-ui` runs a small HTTP server with no external web framework. It binds
to `127.0.0.1`, serves one embedded single-page app, and exposes a JSON API that
reuses the core crates to compile a `.bcl` document, build a plan, inspect state,
show history, and detect drift.

It is normally started through the CLI:

```sh
bastion ui examples/aws_s3.bcl
```

Security: the server listens on loopback only and supports an optional bearer
token (`--token`). The `apply` action is disabled unless explicitly enabled, and it runs
the mock provider, so it does not change real infrastructure.

See the workspace [README](../../README.md) for the project overview.

## License

Apache-2.0 OR MIT, at your option.

[bastion]: https://bastion.tpt.dev
