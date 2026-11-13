# tpt-bastion-cli

The `bastion` command-line tool for [TPT Bastion][bastion].

The CLI compiles `.bcl` documents to the Bastion IR and runs the pipeline on
them:

```sh
cargo run --bin bastion -- compile examples/aws_s3.bcl
cargo run --bin bastion -- validate examples/aws_s3.bcl
cargo run --bin bastion -- plan examples/aws_s3.bcl --json
```

Subcommands: `compile`, `validate`, `graph`, `fmt`, `plan` (`--json`, `--out`,
`--explain`), `apply`, `destroy`, `refresh`, `state` (`show`/`pull`, `rm`, `mv`,
`push`, `history`, `lock-status`), `drift`, `output`, `provider` (`list`,
`schema`, `docs`, `scaffold`), `events`, `ui`, `import` (`tf`, `state`), `lsp`,
`init`, `doctor`, and `completions`. Run `bastion <subcommand> --help` for details.

`apply` and `destroy` refuse to run without `--mock`. That flag selects the
in-memory mock provider, which does not change real infrastructure. No real
provider is wired into the CLI yet. See [`docs/status.md`](../../docs/status.md).

See the workspace [README](../../README.md) for the project overview.

## License

Apache-2.0 OR MIT, at your option.

[bastion]: https://bastion.tpt.dev
