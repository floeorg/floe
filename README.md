<p align="center">
  <img src="assets/logo.svg" alt="Floe logo" width="128">
</p>

<p align="center">
  <a href="https://github.com/floeorg/floe/releases"><img src="https://img.shields.io/github/release/floeorg/floe" alt="GitHub release"></a>
  <a href="https://crates.io/crates/floe"><img src="https://img.shields.io/crates/v/floe" alt="crates.io"></a>
  <a href="https://www.npmjs.com/package/@floeorg/vite-plugin"><img src="https://img.shields.io/npm/v/@floeorg/vite-plugin" alt="npm"></a>
  <a href="https://open-vsx.org/extension/floeorg/floe"><img src="https://img.shields.io/open-vsx/v/floeorg/floe" alt="Open VSX"></a>
  <a href="https://marketplace.visualstudio.com/items?itemName=floeorg.floe"><img src="https://img.shields.io/visual-studio-marketplace/v/floeorg.floe?label=marketplace" alt="VS Code Marketplace"></a>
</p>

<!-- A spacer -->
<div>&nbsp;</div>

A functional language that compiles to TypeScript. Pipes, pattern matching, Result/Option types, and full npm interop.

> [!WARNING]
> **Floe is experimental.** The language is pre-1.0, under active development, and should not be used in production. Expect bugs, rough edges, and breaking changes to the syntax, compiler output, and public APIs between releases. Pin your version and read the [CHANGELOG](CHANGELOG.md) before upgrading.

```floe
import trusted { useState } from "react"

type User {
  name: string,
  role: string,
  active: boolean,
}

type Status {
  | Loading
  | Failed(string)
  | Ready(Array<User>)
}

export fn Dashboard() -> JSX.Element {
  const [status, setStatus] = useState<Status>(Loading)

  status |> match {
    Loading -> <Spinner />,
    Failed(msg) -> <Alert message={msg} />,
    Ready(users) -> {
      const active = users
        |> filter(.active)
        |> sortBy(.name)

      <div>
        <h2>{active |> length} active</h2>
        {active |> map((u) =>
          <Card key={u.name} title={u.name} badge={u.role} />
        )}
      </div>
    },
  }
}
```

## Links

- [Website](https://floe-lang.dev)
- [Documentation](https://floe-lang.dev/docs/)
- [Playground](https://floe-lang.dev/playground/)
- [Changelog](CHANGELOG.md)
