# Wayfarer

A native GNOME application for planning automated drone survey and photogrammetry missions. Supports DJI Fly waypoint format and Litchi CSV export.

Built with Rust, GTK4, and libadwaita.

> [!WARNING]
> Wayfarer is in **early alpha**. Things will break, UI might be rough, and features are still missing. If you run into a bug, please [open an issue](https://github.com/SilkePilon/Wayfarer/issues/new?template=bug_report.yml) — it genuinely helps. Feature requests are also very welcome.

## Features

- **Native GNOME experience** — built with GTK4 and libadwaita, follows GNOME HIG
- **Interactive map** — draw survey polygons directly on the map using libshumate
- **Boustrophedon flight paths** — automatic lawnmower-pattern waypoint generation
- **DJI Fly export** — generates KMZ files compatible with DJI drones that support waypoints
- **Litchi CSV export** — for drones that don't support native waypoints
- **KML import/export** — share survey boundaries between tools
- **Camera presets** — built-in profiles for 17+ DJI and Autel drones, plus custom presets
- **Terrain following** — optional AGL altitude adjustments via Open-Elevation
- **Direct controller upload** — push missions to a connected DJI RC over MTP
- **Project management** — save, load, and organize multiple survey projects

## Supported Drones

**DJI Fly (native waypoints):** Mini 4 Pro, Mini 5 Pro, Air 3, Air 3S, Mavic 3 series, and others with waypoint support.

**Litchi:** Mini 2, Mini SE, Air 2S, Mavic Mini, Mavic Air 2, Mavic 2 series, Phantom 3/4 series, Inspire 1/2, Spark, and more.

## Installation

### Download

Grab the latest build from the [Releases page](https://github.com/SilkePilon/Wayfarer/releases). Binaries are available for Linux, Windows, and macOS.

**Linux:**
```bash
# Extract and run
tar -xf wayfarer-linux-x86_64.tar.gz
cd wayfarer-linux-x86_64
./wayfarer
```

**Windows:** Extract the zip and run `wayfarer.exe`. If Windows Defender complains, click "More info" → "Run anyway".

**macOS:** Open the `.dmg` and drag Wayfarer to Applications. On first launch you may need to right-click → Open to bypass Gatekeeper.

### Build from source

**Dependencies:**

- Rust toolchain (stable, 1.75+)
- GTK4 development libraries
- libadwaita development libraries
- libshumate development libraries

On Fedora:
```bash
sudo dnf install gtk4-devel libadwaita-devel libshumate-devel
```

On Ubuntu/Debian:
```bash
sudo apt install libgtk-4-dev libadwaita-1-dev libshumate-1.0-dev
```

On Arch:
```bash
sudo pacman -S gtk4 libadwaita libshumate
```

Then build and run:
```bash
git clone https://github.com/SilkePilon/Wayfarer.git
cd Wayfarer
cargo build --release
./target/release/wayfarer
```

## Getting Started

1. Launch Wayfarer and create a new project. Search for the area you want to survey.
2. On the **Draw** tab, click the map to place polygon vertices around your survey area.
3. Switch to the **Aircraft** tab to configure altitude, speed, overlap, and other flight parameters.
4. On the **Camera** tab, pick your drone's camera from the presets or enter custom sensor specs.
5. The **Review** tab shows mission stats. Export as DJI KMZ or Litchi CSV, or upload directly to a connected controller.

That's it — load the exported file onto your drone and fly.

## Contributing

Found a bug? [Report it.](https://github.com/SilkePilon/Wayfarer/issues/new?template=bug_report.yml)

Have an idea? [Request a feature.](https://github.com/SilkePilon/Wayfarer/issues/new?template=feature_request.yml)

Pull requests are welcome. If you're planning something big, open an issue first to discuss.

## License

GPL-3.0-or-later — see [LICENSE](LICENSE) for details.
