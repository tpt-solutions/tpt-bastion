name: Bug report
about: Report a defect in the Bastion compiler, CLI, or tooling
title: "[bug]: "
labels: ["bug"]
body:
  - type: markdown
    attributes:
      value: |
        Thanks for taking the time to file a bug report. Please fill in as much
        detail as you can so we can reproduce and fix the issue quickly.
  - type: textarea
    id: what-happened
    attributes:
      label: What happened?
      description: A clear and concise description of the bug.
      placeholder: When I compile X, the CLI panics instead of reporting an error.
    validations:
      required: true
  - type: textarea
    id: reproduce
    attributes:
      label: Steps to reproduce
      description: Minimal steps and a `.bcl` snippet that triggers the issue.
      render: shell
    validations:
      required: true
  - type: textarea
    id: expected
    attributes:
      label: Expected behavior
      description: What did you expect to happen?
    validations:
      required: true
  - type: input
    id: version
    attributes:
      label: Bastion version
      description: Output of `bastion --version`
    validations:
      required: false
  - type: dropdown
    id: area
    attributes:
      label: Area
      description: Which part of the toolchain is affected?
      options:
        - DSL compiler (lex/parse/compile)
        - CLI
        - IR
        - Execution engine
        - State management
        - Provider system
        - Language bindings
        - Other
    validations:
      required: true
