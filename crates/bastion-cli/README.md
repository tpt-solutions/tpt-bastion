# bastion-cli

The `bastion` command-line tool for [TPT Bastion][bastion].

The initial CLI compiles `.bcl` documents to the Bastion IR:

```sh
cargo run --bin bastion -- compile examples/aws_s3.bcl
```

`plan`, `apply`, and `state` subcommands will be added here as the execution
engine and state backends land.

See the workspace [README](../../README.md) for the project overview.

## License

Apache-2.0 OR MIT, at your option.

[bastion]: https://bastion.tpt.dev
