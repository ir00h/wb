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
| `e2e`          | journeys that drive the real desktop app, and the driver they use                      |

The TypeScript types in `window/src/bindings` are generated from the Rust types. Change the Rust type, then run `cargo xtask bindings`.

## Build

You need Rust (the version in `rust-toolchain.toml` is installed on first use), Node (the version in `.node-version`) and pnpm (the version in `package.json`).

```sh
pnpm install
cargo build --workspace
pnpm tauri dev -- -- --workspace <folder>
```

The app shows one board of a workspace folder. Name the folder with `--workspace <folder>` or `WHITEBOARD_WORKSPACE`, and the board with `--board <board>` or `WB_BOARD`; without a board it opens the first one. It watches the board's folder and shows changes other programs make.

`pnpm tauri build --debug` builds the app bundle under `target/debug/bundle`.

## Check

```sh
cargo xtask check
```

This runs every check and prints one line per step. Continuous integration runs the same command. To run one step, name it: `cargo xtask check comments`. The steps are `comments`, `layers`, `bindings`, `commits`, `hygiene`, `rust`, `window`, `shell-default-clean` and `e2e`.

## Journeys in the real app

`cargo xtask check e2e` builds the window and the shell with the `automation` feature, then runs every journey in `e2e/journeys` against the real app in its own web view. Each launch gets a new temporary workspace and data folder. The shell built for the journeys stays at `target/debug/whiteboard-automation`.

With the feature on, setting `WB_AUTOMATION_PORT` makes the shell listen on `127.0.0.1` at that port (`0` lets the system choose) and print `AUTOMATION_LISTENING 127.0.0.1:<port>` once the page has loaded. It answers only programs on the same machine, and refuses requests that carry an `Origin` header, so a web page cannot reach it.

| Route            | Does                                                                                                                                           |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `POST /evaluate` | runs the request body as the body of an async function in the web view and returns its value as JSON; status 422 with the error when it throws |
| `POST /type`     | types the request body into the web view as text input from the system                                                                         |
| `POST /key`      | presses one key named in the request body: `Escape`, `Enter`, `Tab` or `Backspace`                                                             |
| `POST /click`    | clicks at `{"x": .., "y": .., "count": ..}` in page points, `count` 2 for a double click                                                       |
| `GET /snapshot`  | returns a PNG of the web view                                                                                                                  |
| `GET /window`    | returns the title, size, scale, window number, whether the window is visible and whether background mode is on                                 |
| `GET /applied`   | returns how many operations the shell has applied, by kind and note                                                                            |
| `POST /quit`     | quits the app                                                                                                                                  |

`WB_AUTOMATION_BACKGROUND=1` keeps the app from ever becoming the frontmost app and keeps the page rendering while the window is hidden behind others (macOS only). It needs `WB_AUTOMATION_PORT` too; without it the app refuses to start rather than start in the foreground. When the system's web view cannot keep a hidden window rendering, the app says why and exits instead of announcing the channel. With the channel on, the app exits by itself when the program whose process id is in `WB_AUTOMATION_RUNNER` ends, or, without that variable, the program that started it; the journey runner passes its own, so an app started through a wrapper still ends with the run. Journeys run in background mode; `WB_E2E_FOREGROUND=1` turns it off, as continuous integration does. `shell-default-clean` fails when the default shell binary contains any part of the channel.

Code carries no comments. Say it in code: a name, a type, a test.
