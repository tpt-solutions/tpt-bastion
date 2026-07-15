name: Feature request
about: Propose a new capability, provider, or improvement for Bastion
title: "[feature]: "
labels: ["enhancement"]
body:
  - type: markdown
    attributes:
      value: |
        Thanks for suggesting an idea. Please check `todo.md` first to see if
        the work already fits into the published roadmap.
  - type: textarea
    id: problem
    attributes:
      label: Problem
      description: What problem would this feature solve? Who experiences it?
    validations:
      required: true
  - type: textarea
    id: proposal
    attributes:
      label: Proposed solution
      description: Describe the behavior you'd like to see.
    validations:
      required: true
  - type: input
    id: roadmap
    attributes:
      label: Roadmap phase
      description: If this maps to `todo.md`, which phase? (e.g. Phase 5 — Provider Plugin System)
    validations:
      required: false
  - type: dropdown
    id: area
    attributes:
      label: Area
      options:
        - DSL / language
        - CLI
        - IR
        - Execution engine
        - State management
        - Provider system
        - Language bindings
        - Docs / website
        - Other
    validations:
      required: true
