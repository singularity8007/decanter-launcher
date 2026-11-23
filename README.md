# Decanter

*Made out of spite.*

Decanter is a Linux launcher for anime gacha games. It installs and updates
the games, manages Proton builds and the Steam Linux Runtime, and launches
everything with one Play button.

> Early development. Expect rough edges.

## Games

| Game | Downloads via |
|---|---|
| Zenless Zone Zero | HoYoverse Sophon |
| Honkai: Star Rail | HoYoverse Sophon |
| Genshin Impact | HoYoverse Sophon |
| Wuthering Waves | Kuro (via [fischl-rs](https://github.com/TwintailTeam/fischl-rs)) |
| Arknights: Endfield | Zipped packs (via fischl-rs) |

## Features

- **Install and update games**, with hash-checked downloads, pause and
  resume, and a live speed graph. Updates can run automatically when you
  press Play.
- **Proton manager** for GE-Proton, Proton-CachyOS, Proton-EM, UMU-Proton,
  Proton-Twintail, Wineland and Plop. Builds are verified against GitHub's
  sha256 digests. Auto-update is optional, and you can pin a version per game.
- **Steam Linux Runtime**: each Proton build gets the container it asks for
  (sniper or SLR 4).
- **Per-game defaults**, such as the right Proton family and one-time Windows
  components (vcrun2022 etc.) installed through pinned, hash-checked
  winetricks.
- **Playtime tracking** and per-game launch options.

## Building

Requirements:

- Rust (edition 2024)
- Node.js and npm
- The [Tauri v2 Linux prerequisites](https://v2.tauri.app/start/prerequisites/#linux)
  (WebKitGTK 4.1 dev packages, etc.)
- The Tauri CLI: `cargo install tauri-cli --version "^2"`

```sh
git clone git@github.com:singularity8007/decanter-launcher.git
cd decanter-launcher
(cd ui && npm install)

cargo tauri dev      # run in development
cargo tauri build    # build .deb and AppImage
```

`dev-env.sh` is optional. Source it to use a project-local Rust toolchain
in `.toolchain/`.

### UI only

You can work on the UI in a normal browser without Tauri. In development,
an IPC mock (`ui/src/mock.ts`) stands in for the backend:

```sh
cd ui && npm run dev
```

## Layout

```
core/       decanter-core: games, Proton, Steam Runtime, downloads, launching
src-tauri/  Tauri shell: a thin command layer over decanter-core
ui/         React + TypeScript + Tailwind frontend
```

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).

Decanter uses [fischl-rs](https://github.com/TwintailTeam/fischl-rs) by the
Twintail team. On first launch, it downloads the process reaper from
[TwintailLauncher](https://github.com/TwintailTeam/TwintailLauncher) at a
pinned commit and checks its SHA-256. The reaper isn't included in this
repository.

Decanter is not affiliated with HoYoverse, Kuro Games, Hypergryph/Gryphline
or Valve. All game names and art belong to their owners.
