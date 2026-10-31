# Duke Nukem II in Rust

A Rust rewrite of **[Duke Nukem II](https://de.wikipedia.org/wiki/Duke_Nukem_II)** (Apogee, 1993): the game's engine written again
from scratch. It reads the original game files and plays them, in a desktop window
or in the browser.

The game data is not part of this repository. The first episode is shareware and
free to download (see below); the full game has all four episodes.

## Play it

### In the browser

Open **<https://voelklmichael.github.io/duke2-in-rust/>**, drop the game on the page
(the shareware's `4duke.zip` as it is, or any game folder or zip) and press *Play*.
The game files never leave your computer: they are unpacked and kept in the
browser only.

### On your computer

You need a recent stable Rust (edition 2024) and the game files.

1. Download the shareware [4duke.zip](https://dnr.duke4.net/dl/024fbc5/4duke.zip)
   (from [Duke Nukem Repository's shareware page](https://dnr.duke4.net/shareware.php.html)).
2. Unpack it, then unpack the installer archive `dn2.1` inside it, which is a zip too:

   ```sh
   unzip 4duke.zip -d 4duke
   unzip 4duke/dn2.1 -d game
   ```

   The game folder needs `NUKEM2.CMP` and `NUKEM2.F1` to `NUKEM2.F5`.
3. Run it:

   ```sh
   cargo r --release -- --data game
   ```

`cargo r --release -- --help` lists the options, for example:

```sh
cargo r --release -- --data game --wide --filter xbr   # 16:9, smoothed pixels
cargo r --release -- --data game --play L1             # straight into a level
cargo r --release -- --data game --viewer              # the asset viewer
```

Keys: arrows to move, Ctrl to jump, Alt to fire, Esc for the menu.

## Features

- The whole game: intro, menus, story movies, demos, bonus screens, all levels,
  saved games and high scores.
- Sound and music as with a Sound Blaster and AdLib: digitized effects, and AdLib
  effects and IMF music on an emulated OPL2 chip ([oplon](https://crates.io/crates/oplon)).
- **Widescreen**: a 16:9 view (424×200) that shows more of the level; the game
  plays the same.
- **Upscaling filters**: sharp pixels (nearest), EPX, or xBR-style smoothing with
  blended edges, at any render scale.
- **HD packs**: higher-detail pictures (a folder or zip of PNGs) replace the
  original art.
- Original tall-pixel 4:3 aspect or square pixels, integer scaling, fullscreen.
- A level browser and an asset viewer for the game's maps, tiles, sprites and
  pictures.
- Desktop: headless screenshots after a scripted run of input
  (`--screenshot shot.png --input "R:20,F:1"`), handy for testing.

## Under the hood

- **`#![no_std]` engine.** All engine crates (`engine/`) are `no_std` with
  `alloc`: they never do I/O, read the clock or block. A frontend hands in the
  game files, input and time, and gets back frames, sound samples and events.
- **No `unsafe`.** The code in this repository contains no `unsafe` blocks.
- **WebAssembly.** The same engine runs in the browser, compiled to
  `wasm32-unknown-unknown`; GitHub Pages builds and publishes it on every push to
  `main`.
- **Deterministic simulation.** The game logic steps only when told to, on the
  original's 140 Hz clock.

| Crate | What it does |
| --- | --- |
| `engine/dn2_assets` | Parses the original data files: archive, levels, tiles, sprites, sounds, movies |
| `engine/dn2_game` | The simulation: player, actors, level state and rules |
| `engine/dn2_render` | Draws the world into a frame, with the upscaling filters |
| `engine/dn2_ui` | Menus, text scripts, intro, demos, bonus screens, saves |
| `engine/dn2_audio` | Sound effects and music, mixed to samples |
| `engine/dn2_engine` | The engine as frontends see it |
| `frontends/dn2_desktop` | A desktop window (winit, softbuffer, cpal) |
| `frontends/dn2_web` | The web page (wasm-bindgen) |

### Building the web page

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.121   # must match dn2_web's wasm-bindgen
frontends/dn2_web/build.sh
python3 -m http.server -d frontends/dn2_web/www
```

## Legal

This is a fan project, not affiliated with Apogee, 3D Realms or the owners of
Duke Nukem. Duke Nukem is a trademark of its respective owners. No game data is
included; use the shareware episode or your own copy of the game.
