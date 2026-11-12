# Getting Started with TPT Bastion

This guide takes you from zero to an applied stack. It assumes Rust 1.74+ for
building from source, or that you've installed the prebuilt binary (see the
[README](https://github.com/tpt/bastion#installation)).

## 1. Install

```sh
# Prebuilt binary (Linux/macOS)
curl -fsSL https://raw.githubusercontent.com/tpt/bastion/master/scripts/install.sh | sh

# From source
cargo install --path crates/tpt-bastion-cli

# Verify
bastion --help
```

## 2. Your first document

Bastion configs use the `.bcl` language. Create `main.bcl`:

```bcl
config {
    name = "hello-bastion"
    region = "us-east-1"
}

resource aws_s3_bucket my_bucket {
    bucket = "hello-bastion-bucket"
    acl = "private"
    tags {
        environment = "dev"
    }
}

output bucket_name {
    value = my_bucket.bucket
}
```

## 3. Compile

Compile to the stable intermediate representation (IR) and inspect it:

```sh
bastion compile main.bcl
```

The output is JSON: the exact artifact the engine, providers, state backend,
and language bindings all consume.

## 4. Plan (dry-run)

See what *would* happen, without touching anything:

```sh
bastion plan main.bcl
```

With `--state <file>` the plan diffs against recorded state to show
creates/updates/deletes.

## 5. Apply

Apply the plan. State is written to a file you control:

```sh
bastion apply --auto-approve --state ./state.bclstate main.bcl
```

State is encrypted when you pass `--passphrase-env BASTION_PASS` (the variable
must hold the passphrase). The apply acquires a lock on the state file so
concurrent runs can't clobber it.

## 6. Drift detection

Infrastructure drifts when it's changed outside Bastion. Detect it:

```sh
bastion drift --state ./state.bclstate main.bcl
```

Add `--reconcile` to print the plan that brings state back in line, then re-run
`apply`.

## 7. Providers

Built-in providers register via the plugin runtime. Scaffold your own:

```sh
bastion provider scaffold acme --out ./providers
bastion provider list
```

See [`provider-guide.md`](provider-guide.md) and
[`bindings.md`](bindings.md) for extending Bastion with custom providers and
language bindings.

## 8. Event-driven updates

Bastion can reconcile infrastructure when external events fire (a file change,
a webhook, or a future [TPT Flux] backend). See
[`events.md`](events.md) for the `events` subcommand and the `EventSource`
pluggable interface.

## Next steps

- [Language reference](language-reference.md) — the full `.bcl` syntax.
- [Provider guide](provider-guide.md) — build and ship a provider.
- [Binding architecture](bindings.md) — drive Bastion from TypeScript/Python.
- [Migrating from Terraform/OpenTofu](migrate-from-terraform.md).

[TPT Flux]: https://bastion.tpt.dev/flux
