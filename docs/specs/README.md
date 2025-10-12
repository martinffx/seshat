# Feature Specifications

This directory contains detailed specifications for each feature.

Each specification should include:
- Business context and requirements
- Technical design
- Implementation tasks in dependency order
- Test scenarios

## Creating a New Specification

Use the `/spec:create <feature_name>` command to create a new feature specification.

Example:
```
/spec:create raft-consensus
```

This will guide you through:
1. Requirements gathering (analyst-agent)
2. Technical design (architect-agent)
3. Task breakdown (architect-agent)
4. Implementation planning (coder-agent)

## Workflow

1. **Create spec**: `/spec:create <feature>`
2. **Design architecture**: `/spec:design <feature>`
3. **Generate tasks**: `/spec:plan <feature>`
4. **Implement**: `/spec:implement <feature>`
5. **Track progress**: `/spec:progress <feature>`
