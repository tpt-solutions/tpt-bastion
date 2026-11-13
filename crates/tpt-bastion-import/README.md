# tpt-bastion-import

Migration from Terraform and OpenTofu to TPT Bastion. `convert` turns a `.tf`
file into `.bcl` and reports every construct it could not convert with its line
number. `import_tfstate` reads a version 4 `terraform.tfstate` as Bastion
resources. See `docs/migrate-from-terraform.md` in the repository.

Licensed under Apache-2.0 OR MIT.
