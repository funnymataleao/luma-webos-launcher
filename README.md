# Luma: LG webOS TV Launcher

**A fast, remote-first launcher and home screen for LG webOS TVs, with an Apple TV feel.**

Luma replaces the cluttered LG Home with one calm screen: your favorite apps, live video wallpapers, widgets, your Jellyfin library, your PC games over Moonlight and your Home Assistant devices. It's a native Rust app drawn with OpenGL ES, so it stays smooth on TV hardware.

Free and open source (MIT). Fork it, change it, ship it.

> [!NOTE]
> **This is a fork.** It builds Luma **2.3.0** under its own app ID, **`com.archbtw.luma`**, so it
> installs beside the original Luma (`com.denisefremov.luma`) with its own settings. What differs
> from the original:
>
> - Video wallpapers are the default, and you can upload your own videos from a phone or computer.
> - The Home headline and greetings are tech themed.
> - LG's own apps start hidden.
> - The crypto widget is removed.
> - A command-line autostart hook (`autostart.sh`) is included.
>
> The promo video and most screenshots below show the original Luma, so headline text and widgets
> look a little different from this build.

<a href="https://github.com/funnymataleao/luma-webos-launcher/releases/download/v2.2.160/Luma-promo.mp4"><img src="docs/promo-preview.webp" width="100%" alt="Luma promo video"></a>

▶ [Watch the original Luma promo video (1 min)](https://github.com/funnymataleao/luma-webos-launcher/releases/download/v2.2.160/Luma-promo.mp4)

![Home](docs/screenshots/home.jpg)

## Features

**Home**

- Favorite apps row and "stacks": rows of content from Jellyfin, Moonlight and more, each capped at 20 cards with a **View Library** page behind it.
- Live video wallpapers, on by default: Apple and Amazon aerials played in hardware. Pick whole collections or single clips.
- Your own video wallpapers: **Settings › Wallpaper › Upload Videos** shows a QR code. Open it on a phone or computer on the same network and pick MP4/MOV files (H.264 or HEVC). They play from the TV's storage, and **Only My Videos** rotates just your uploads.
- Frosted-glass UI with one accent gradient, adjustable colors, heading and text fonts, and UI resolution (Auto / 4K / 1440p / 1080p).
- A tech-themed headline that comments on the weather ("Thermal throttling: off.", "Packet loss: rain."), plus a clock and date. Prefer a plain greeting? Switch **Settings › General › Greeting** from Tech to Classic (also tech themed: "System booted. Good morning.").
- LG's own apps (Multi View, Gallery, Home Hub, Camera, Web Browser, LG Link, Sports, Memory Care and the like) start hidden. Live TV stays. Unhide any of them from **Apps › Hidden**.

**Widgets**

- Weather (Open-Meteo, no API key), with hourly and 10-day forecasts.
- HDMI input preview and Home Assistant status line.
- Small, medium, large and text-line sizes, laid out on a grid.

**Movies, series and music (Jellyfin, Plex)**

- Sign in with a code or a QR code on your phone: no typing with the remote.
- Movie and series pages with backdrop, cast, seasons, episodes and "More like this".
- A built-in HTML5 player (`com.archbtw.luma.player`) with audio and subtitle track selection, quality and seek previews. Playback progress syncs back to your server.
- Music: albums, songs and a now-playing screen.
- Plex support is new and less tested than Jellyfin.

**Games (Moonlight)**

- Your Steam library from a PC running Sunshine or Apollo, shown as a stack with Steam artwork.
- Game pages work like movie pages. **Play** opens the stream in Moonlight.

**Smart Home (Home Assistant)**

- Pair by scanning a QR code with your phone. Your token never has to be typed on the TV.
- Status chips (climate, alarm, lights, doors), scenes, live cameras and device tiles you can toggle with the remote.
- Hide any camera or device from the TV.

**Remote and pointer**

- Built for the D-pad: Back always steps one level out.
- The Magic Remote pointer works everywhere, including drag-to-scroll.

## Screenshots

### Home and apps

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/home-2.jpg" alt="Home with widgets"></td>
    <td width="50%"><img src="docs/screenshots/home-3.jpg" alt="Home, morning mist wallpaper"></td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/screenshots/home-jellyfin.jpg" alt="Jellyfin rows on Home"></td>
    <td width="50%"><img src="docs/screenshots/apps.jpg" alt="All apps"></td>
  </tr>
</table>

### Movies, series and music

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/movie.jpg" alt="Movie page"></td>
    <td width="50%"><img src="docs/screenshots/series.jpg" alt="Series page with episodes"></td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/screenshots/music.jpg" alt="Now playing"></td>
    <td width="50%"><img src="docs/screenshots/album.jpg" alt="Album"></td>
  </tr>
</table>

### Games (Moonlight)

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/game.jpg" alt="007 First Light"></td>
    <td width="50%"><img src="docs/screenshots/game-2.jpg" alt="Cyberpunk 2077"></td>
  </tr>
</table>

### Smart Home

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/smart-home.jpg" alt="Smart Home tab with cameras"></td>
    <td width="50%"><img src="docs/screenshots/settings-smart-home.jpg" alt="Home Assistant settings"></td>
  </tr>
</table>

### Widgets and your own videos

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/weather.jpg" alt="Weather"></td>
    <td width="50%"><img src="docs/screenshots/upload-videos.jpg" alt="Upload Videos: QR code and address for sending your own video wallpapers"></td>
  </tr>
</table>

### Settings

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/settings-general.jpg" alt="General: language, clock, motion"></td>
    <td width="50%"><img src="docs/screenshots/wallpaper-sources.jpg" alt="Wallpaper sources"></td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/screenshots/wallpapers.jpg" alt="Pick wallpaper clips"></td>
    <td width="50%"><img src="docs/screenshots/settings-appearance.jpg" alt="Appearance"></td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/screenshots/appearance-colors.jpg" alt="Accent colors"></td>
    <td width="50%"><img src="docs/screenshots/settings-stacks.jpg" alt="Stacks"></td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/screenshots/settings-inputs.jpg" alt="HDMI inputs"></td>
    <td width="50%"><img src="docs/screenshots/settings-system.jpg" alt="System and root features"></td>
  </tr>
</table>

## Support the original author

Luma is designed and maintained by one person in their free time, and this fork builds on that work. It's free and will stay free. Donations to the original author keep updates coming, pay for test hardware and move the roadmap forward (Plex, more TV models, new widgets). If Luma makes your TV better, a coffee helps keep it alive.

Luma was made by [Denis Efremov](https://denisefremov.com), an interface designer with 16+ years of experience and the maker of Luma and Rift, two free open-source launchers. Read the design story in the [Luma case study](https://denisefremov.com/work/luma.html).

- ☕ **Ko-fi:** [ko-fi.com/dennis_oizo](https://ko-fi.com/dennis_oizo)
- 🍵 **Buy Me a Coffee:** [buymeacoffee.com/denisefremov](https://buymeacoffee.com/denisefremov)
- 💖 **GitHub Sponsors:** [github.com/sponsors/funnymataleao](https://github.com/sponsors/funnymataleao)
- 🪙 **USDT (TRC20):** `TKPVRy9oYjLsbQMtbDzvkXDu73mg7tLN5u`

## Root features

Luma runs as a normal app without root. On a rooted TV (Homebrew Channel, elevated), **Settings › System › Root features** appears with extras:

| Feature                                                    | Without root | With root       |
| ---------------------------------------------------------- | ------------ | --------------- |
| Open Luma when the TV turns on                             | —            | ✓               |
| Turn LG Home "Auto Launch" off from Luma                   | —            | ✓               |
| Leave a Moonlight stream with Start + Select pressed twice | —            | ✓               |
| Full app and HDMI input list, app icons                    | basic        | ✓ more reliable |
| Detect whether an HDMI input has a signal                  | —            | ✓               |

Without root the Root features section is hidden, and Luma never tries to change system files.

### Autostart from the command line

Opens Luma 25 seconds after a full cold boot, as a regular app (the Home button is not remapped).
Run in the TV's root SSH shell:

```sh
sh /media/developer/apps/usr/palm/applications/com.archbtw.luma/autostart.sh enable
```

Check or undo it, and disable it before uninstalling Luma:

```sh
sh /media/developer/apps/usr/palm/applications/com.archbtw.luma/autostart.sh status
sh /media/developer/apps/usr/palm/applications/com.archbtw.luma/autostart.sh disable
```

It installs `/var/lib/webosbrew/init.d/65-luma`, which Homebrew Channel runs at boot, and logs to
`/tmp/luma-boot.log`. **Open Luma when the TV turns on** above also covers waking from Quick Start+;
you only need one of the two.

## Install

### Manual (IPK)

Download `com.archbtw.luma_2.3.0_arm.ipk` from [Releases](../../releases/latest), or build it yourself
(see below), and install it with [webOS Dev Manager](https://github.com/webosbrew/dev-manager-desktop) or
`ares-install`. To play Jellyfin and Plex videos, also install `com.archbtw.luma.player_0.0.11_all.ipk`.

```sh
ares-install --device tv com.archbtw.luma_2.3.0_arm.ipk
ares-install --device tv com.archbtw.luma.player_0.0.11_all.ipk
```

Coming from the original Luma? It stays installed as a separate app (`com.denisefremov.luma`) with its
own settings and its own companion player. Once you're happy with this build, disable the original's
autostart or Root features and uninstall it. The old player only works with the old app.

### Homebrew Channel

`pnpm run package` also writes a Homebrew Channel feed (`dist/repo.json`). Once a release with that feed
is published on this repository, add it in Homebrew Channel › Settings › Repositories:

```text
https://github.com/archuser-del/luma-webos-launcher/releases/latest/download/repo.json
```

The original author's feed (`funnymataleao/luma-webos-launcher`) installs the original
`com.denisefremov.luma`, not this build.

To use Moonlight, install Moonlight for webOS and pair it with your PC first, then open Luma.

## Build from source

Requirements: Rust 1.88+ with the `armv7-unknown-linux-gnueabi` target, Node 22+, pnpm 10,
`woff2_decompress` and Python with fontTools (for `pnpm assets`), and SDL2 (for the desktop preview).
`nix develop` provides all of them.

```sh
rustup target add armv7-unknown-linux-gnueabi
pnpm install --frozen-lockfile
pnpm sdk                     # downloads the webOS NDK into .build/
export WEBOS_NDK="$PWD/.build/toolchain/arm-webos-linux-gnueabi_sdk-buildroot"
pnpm run package             # → dist/com.archbtw.luma_2.3.0_arm.ipk, player IPK, sha256sums, repo.json
```

Desktop preview with sample data (no TV needed):

```sh
pnpm assets
pnpm dev
```

Tests: `pnpm test`. Lint and format: `pnpm check`, `pnpm format`. More in [development.md](development.md).

## Tech stack

| Part                | Built with                                                                                       |
| ------------------- | ------------------------------------------------------------------------------------------------ |
| Launcher            | Rust 2024, SDL2, OpenGL ES 2 with custom GLSL shaders                                            |
| Text and vector art | `owned_ttf_parser`, our own SVG → triangle compiler (`lyon` tessellation), signed-distance edges |
| Images              | `image` (PNG, JPEG, WebP), decoded off the render thread                                         |
| Networking          | `ureq` + `rustls`, `tungstenite` WebSocket (Home Assistant), `mdns-sd` and UDP discovery         |
| Moonlight pairing   | `rcgen`, `rsa`, `x509-parser`, AES (GameStream / Sunshine protocol)                              |
| QR codes            | `qrcode`                                                                                         |
| Video wallpapers    | webOS hardware media pipeline (C helpers in `native/`), HEVC and H.264, streamed or uploaded     |
| Player              | Separate web app: HTML5 video, HLS, vanilla JS                                                   |
| TV integration      | Luna service bus, Homebrew Channel elevated service                                              |
| Build tooling       | pnpm, Node, `sharp`, `@webos-tools/cli`, Nix flake, oxlint / oxfmt, Changesets                   |
| Data sources        | Open-Meteo, Jellyfin, Plex, Home Assistant, Steam store artwork                                  |

## Privacy

- No telemetry, no accounts, no ads.
- Settings, favorites and tokens stay in a local JSON file on the TV.
- No API keys are built in. Sign-ins go through Quick Connect, Plex link codes or a QR code on your phone.
- Weather uses the location you choose, never IP geolocation.
- Uploaded videos stay on the TV. The upload page is only reachable on your local network, needs the
  one-time code in its address, and stops when you leave **Upload Videos** (or after 30 minutes).

## Fonts and assets

All bundled fonts are under the SIL Open Font License; each font ships with its `OFL.txt`. See [assets.md](assets.md) for wallpapers, icons and their licenses.
Brand logos belong to their owners. Luma is not affiliated with LG, Apple, Amazon, Valve, Jellyfin, Plex or Home Assistant.

## Credits

This build is a fork of **[Luma](https://github.com/funnymataleao/luma-webos-launcher)** by Denis Efremov, which started as a fork of **[ecklf/lg-luma](https://github.com/ecklf/lg-luma)** by ecklf and contributors. Thank you for the native renderer, the build pipeline and the original idea. The command-line autostart hook comes from ecklf/lg-luma.

Also thanks to [webOS Homebrew](https://www.webosbrew.org/), [Moonlight](https://moonlight-stream.org/), [Sunshine](https://github.com/LizardByte/Sunshine) / [Apollo](https://github.com/ClassicOldSong/Apollo), [Jellyfin](https://jellyfin.org/) and [Home Assistant](https://www.home-assistant.io/).

## How this was made

Most of the original Luma was written with AI coding assistants (Claude Code and OpenAI Codex), directed and tested on a real LG OLED C1 by its maintainer, who checked every change on the TV before release.

The 2.3.0 changes in this fork were written with Claude Code and checked with the test suite, desktop
rendering and a browser test of the upload page. They have not been tested on a TV yet.

## License

[MIT](license). Original copyright © 2026 Luma contributors.
