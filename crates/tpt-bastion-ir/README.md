# tpt-bastion-ir

The versioned intermediate representation (IR) for [TPT Bastion][bastion].

`tpt-bastion-ir` defines the stable, serializable artifact that the DSL compiler
produces and that the execution engine, state backends, and provider runtime
consume. Keeping the IR decoupled from the source DSL lets language bindings
(TypeScript, Python) target the same artifact.

See the workspace [README](../../README.md) for the project overview.

## License

Apache-2.0 OR MIT, at your option.

[bastion]: https://bastion.tpt.dev
