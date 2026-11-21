# Whiteboard

Whiteboard is a desktop app that shows a folder of Markdown notes as cards on a canvas, grouped into sections. `wb` is a command-line tool that reads and writes the same boards.

## Layout

| Path           | Holds                                                                                  |
| -------------- | -------------------------------------------------------------------------------------- |
| `crates/model` | `wb-model`: the shapes that are stored or sent to the window, and pure rules over them |
| `crates/core`  | `wb-core`: boards on disk                                                              |
| `crates/cli`   | `wb`: the command-line tool                                                            |
| `crates/shell` | `whiteboard`: the Tauri desktop shell                                                  |
| `crates/xtask` | the check command and the bindings generator                                           |
| `window`       | the TypeScript window: `bindings`, `domain`, `bridge`, `state` and `ui`                |

The TypeScript types in `window/src/bindings` are generated from the Rust types. Change the Rust type, then run `cargo xtask bindings`.

## Build

You need Rust (the version in `rust-toolchain.toml` is installed on first use), Node (the version in `.node-version`) and pnpm (the version in `package.json`).

```sh
pnpm install
cargo build --workspace
pnpm tauri dev
```

`pnpm tauri build --debug` builds the app bundle under `target/debug/bundle`.

## Check

```sh
cargo xtask check
```

This runs every check and prints one line per step. Continuous integration runs the same command. To run one step, name it: `cargo xtask check comments`. The steps are `comments`, `layers`, `bindings`, `commits`, `hygiene`, `rust`, `window` and `shell-default-clean`.

`shell-default-clean` builds the default shell and fails when the binary contains any part of the automation channel, which exists only under the `automation` feature.

Code carries no comments. Say it in code: a name, a type, a test.
