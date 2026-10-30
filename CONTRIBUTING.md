# Contributing

Thanks for helping. Issues, ideas and pull requests are all welcome.

You can expect a first reply on an issue or pull request within 48 hours. To take an issue,
comment on it, and it is yours. Security problems go through [SECURITY.md](SECURITY.md), not issues.

## Build and run

You need Windows 10 or 11, [Rust](https://rustup.rs/) (stable), [Node.js](https://nodejs.org/) 22
and the WebView2 runtime (already on Windows 11).

```sh
npm install
npm run tauri dev        # the app, with hot reload for the window
```

Restart `tauri dev` after changing Rust code: a long-running dev instance can end up running an
old binary under new window code.

## Before you open a pull request

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
npm run build
npm run test:e2e         # builds and drives the real app window
```

CI runs the same checks on every pull request.

## How the code is laid out

- `src-tauri/src/core/`: the engines and rules, platform-free and tested against a mock.
- `src-tauri/src/platform/`: everything Windows-specific. `cfg(windows)` lives only here.
- `src-tauri/src/ipc/`: thin commands the window calls.
- `src/`: the React window, one file per tab.
- `docs/`: the specification. [ARCHITECTURE.md](docs/ARCHITECTURE.md) is the place to start.

## A few rules

- New engine logic comes with a test (`cargo test`), written first where you can.
- Be honest in UI text and docs: never "undetectable" or "human-like", and never promise that a
  chat status stays green. CI checks the shipped code for those words.
- No persistent system changes. Whatever the app holds, it lets go of on exit.
- Keep it small. A new dependency needs a reason a few lines of code cannot meet.

## Good places to start

Issues labelled [good first issue](https://github.com/kalanadidulanga/project-mouse/labels/good%20first%20issue).
Questions and ideas go in [Discussions](https://github.com/kalanadidulanga/project-mouse/discussions).
