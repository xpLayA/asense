# ASense

A native Linux control panel for Acer Predator, Nitro and related notebooks.
It provides performance profiles, fan control, lighting, selected firmware
options and live telemetry without requiring PredatorSense or NitroSense.

The Predator Helios Neo 16 **PHN16-72** is the reference-tested platform.
Since v0.2, ASense discovers compatible Linux, Acer WMI and HID interfaces on
other models and shows only the controls found on the machine.

<!-- markdownlint-disable MD013 MD033 -->
<p align="center">
  <img src="docs/screenshots/asense-compact.png" alt="ASense compact control panel" width="32%">
  <img src="docs/screenshots/asense-advanced.png" alt="ASense advanced metrics panel" width="63%">
</p>
<!-- markdownlint-enable MD013 MD033 -->

## Features

- profile choices from live Linux kernel interfaces, with a known-command Acer
  Gaming-WMI fallback whose writes are verified by readback;
- firmware Auto, manual CPU/GPU and Maximum fan modes through kernel PWM or
  Acer Gaming-WMI;
- temperatures, load and up to eight detected fan RPM channels;
- NVIDIA load, VRAM, clocks, power, P-state and throttle telemetry;
- exact PHN16-72 Turbo GPU offsets with NVML readback and rollback;
- one-to-four-zone WMI lighting;
- ENEK5130 keyboard and cover-logo lighting with runtime-discovered zones and
  effects;
- battery charge limit and firmware calibration;
- USB-off charging, keyboard timeout, boot sound, LCD override and rear-logo
  controls when exposed by firmware;
- compact controls plus advanced graphs and hardware information;
- English, Czech and Simplified Chinese UI.

Missing capabilities are hidden independently: a notebook can have profiles
and RPM without fan writes, or lighting without battery options.

## Supported hardware

<!-- markdownlint-disable MD013 MD033 -->
| Model | Profiles | Fans | Lighting | Platform |
| --- | :---: | :---: | :---: | :---: |
| <code>PHN16&#8209;72</code> | ✅ | ✅ | ✅ | ✅ |
| <code>PH16&#8209;72</code> | 🟢 | 🟢 | 🔎 | 🔎 |
| <code>PT14&#8209;51</code> | 🟢 | 🟢 | 🔎 | 🔎 |
| <code>AN515&#8209;58</code> | 🟢 | 🟢 | 🟡 | 🔎 |
| <code>PHN16&#8209;71</code> | 🟢 | 🟢·🔎 | 🔎 | 🔎 |
| <code>PH16&#8209;71</code> | 🟢 | 🟢·🔎 | 🔎 | 🔎 |
| <code>PH18&#8209;71</code> | 🟢 | 🟢·🔎 | 🔎 | 🔎 |
| <code>PHN14&#8209;51</code> | 🔎 | 🔎 | 🟡 | 🔎 |
| <code>PHN16S&#8209;71</code> | 🔎 | 🔎 | 🟡 | 🔎 |
| <code>PHN16&#8209;73</code> | 🔎 | 🔎 | 🟡 | 🔎 |
| <code>AN16S&#8209;61</code> | 🔎 | 🔎 | 🔎 | 🔎 |
| <code>AN515&#8209;45</code> | 🔎 | 🔎 | 🟡 | 🔎 |
| <code>AN515&#8209;55</code> | 🔎 | 🔎 | 🟡 | 🔎 |
| <code>AN515&#8209;56</code> | 🔎 | 🔎 | 🟡 | 🔎 |
| <code>AN515&#8209;57</code> | 🔎 | 🔎 | 🟡 | 🔎 |
| <code>AN517&#8209;41</code> | 🔎 | 🔎 | 🟡 | 🔎 |
| <code>PH315&#8209;52</code> | 🔎 | 🔎 | 🟡 | 🔎 |
| <code>PH315&#8209;53</code> | 🔎 | 🔎 | 🟡 | 🔎 |
| <code>PH315&#8209;54</code> | 🔎 | 🔎 | 🟡 | 🔎 |
| <code>PH317&#8209;53</code> | 🔎 | 🔎 | 🟡 | 🔎 |
| <code>PH517&#8209;61</code> | 🔎 | 🔎 | 🟡 | 🔎 |
| <code>PT314&#8209;51</code> | 🔎 | 🔎 | 🟡 | 🔎 |
| <code>PT315&#8209;51</code> | 🔎 | 🔎 | 🟡 | 🔎 |
| <code>PT316&#8209;51</code> | 🔎 | 🔎 | 🟡 | 🔎 |
| <code>PT515&#8209;51</code> | 🔎 | 🔎 | 🟡 | 🔎 |
| <code>PT516&#8209;52s</code> | 🔎 | 🔎 | 🟡 | 🔎 |
<!-- markdownlint-enable MD013 MD033 -->

**Legend:** ✅ Reference tested · 🟢 Linux provides it · 🟡 Known Acer
controller/protocol · 🔎 Enabled only when the live probe finds it ·
🟢·🔎 RPM is available, fan control is probed · 🤝 Community confirmed.

PHN16-72 is the full reference platform. PHN14-51 has a known three-zone WMI
layout; PHN16S-71 and PHN16-73 use ENEK5130 lighting. Yellow does not force a
control: it still appears only after the controller answers correctly.

<!-- markdownlint-disable MD033 -->
<details>
<summary><strong>99 model and internal identifiers from the research inventory</strong></summary>
<!-- markdownlint-enable MD033 -->

The following identifiers occur in official Acer Sense packages or public
hardware reports. They are useful machines to test; they are not an all-feature
allow-list.

### Current PredatorSense cohort

```text
PH16-71 PH18-71 PH3D15-71 PHN16-71 PT14-51 PT16-51 PTX17-71
PH16-72 PH18-72 PHN14-51 PHN16-72 PHN18-71 PTN16-51 T7001
PH16-73 PH18-73 PHN14-71 PHN16-73 PHN18-72 PHN16S-71 PT14-52T PTN16-71
```

### Current NitroSense cohort

```text
AN14-41 AN16-41 AN16-42 AN16-43 AN16-51 AN16-61 AN16-72 AN16-73
AN16S-61 AN18-61 AN17-41 AN17-42 AN17-51 AN17-71 AN17-72
ANV14-61 ANV14-62 ANV14-71 ANV15-41 ANV15-42 ANV15-51 ANV15-52
ANV16-41 ANV16-42 ANV16-61 ANV16-71 ANV16-72 ANV16S-61 ANV16S-71
ANV17-41 ANV17-61
```

### Legacy NitroSense cohort

```text
AN515-42 AN515-43 AN515-44 AN515-45 AN515-46 AN515-47 AN515-51s
AN515-52 AN515-53 AN515-54 AN515-55 AN515-56 AN515-57 AN515-58
AN517-41 AN517-42 AN517-43 AN517-51 AN517-52 AN517-53 AN517-54
AN517-55 AN715-41 AN715-51 AN715-52
```

### Additional Predator/Triton candidates

```text
PH315-52 PH315-53 PH315-54 PH315-55 PH317-53 PH317-54 PH517-51
PH517-52 PH517-61 PH717-71 PH717-72 PT314-51 PT315-51 PT314-52s
PT315-52 PT316-51 PT316-51s PT515-51 PT515-52 PT516-52s PT917-71
```

Battery/APGE discovery can also help non-gaming Acers. Public working reports
include `A315-24PT`, `A315-44P`, `A315-59`, `A315-510P`, `A515-45`,
`A515-46-R14K`, `A715-42G`, `AG15-42P`, `AV15-53P`, `EUN314A-51W`,
`AN515-44`, `AN515-57`, `AN515-58`, `AN517-54`, `ANV15-51`, `AN16-43-R7N7`,
`ANV16-42`, `PHN16-71`, `SF314-34`, `SF314-43`, `SFE16-44-R48X`,
`SFG14-63-R6PU`, `SFG16-72`, `SFX14-71G` and `SFX16-61G`.

<!-- markdownlint-disable MD033 -->
</details>
<!-- markdownlint-enable MD033 -->

The model names above do not control discovery. The live backend order is:

```text
profiles: kernel platform_profile -> Acer Gaming-WMI -> unavailable
fans:     kernel PWM -> Acer Gaming-WMI -> RPM only
lighting: zoned WMI or a detected ENEK5130 target
```

Kernel-backed profile choices come from the live kernel `choices` interface.
The Gaming-WMI fallback instead exposes the driver's bounded set of known
commands; it is not a firmware-enumerated list. The probe makes this distinction
explicit as `profiles.choices_source = kernel-live` or
`known-gaming-wmi-commands`.

The battery 80% health limit is a firmware setting, not a Linux charge
threshold file. Enabling it while the battery is already above 80% does not
actively discharge the battery; the effect becomes visible after normal use,
when firmware prevents a subsequent charge from exceeding the limit.

## Install

### Ubuntu PPA (recommended)

On Ubuntu 26.04, install ASense and receive updates through APT:

```bash
sudo add-apt-repository ppa:fladirmacht/asense
sudo apt update
sudo apt install asense
```

APT installs the application, privileged daemon, DKMS transport and desktop
integration together. Rust is not required. Remove the package with
`sudo apt remove asense`, or remove its ASense-owned configuration and state as
well with `sudo apt purge asense`.

### Standalone release

The recommended release asset is the
[`ubuntu-26.04-x86_64-installer` ZIP](https://github.com/fladirm/asense/releases/latest).
It contains prebuilt `asense` and `asensed` binaries, so Rust is not required.
Ubuntu 26.04 x86_64 is the supported prebuilt baseline.

Install runtime and optional DKMS prerequisites:

```bash
sudo apt update
sudo apt install \
  build-essential dkms "linux-headers-$(uname -r)" kmod udev util-linux \
  python3 unzip mokutil desktop-file-utils \
  libgtk-3-0t64 libwebkit2gtk-4.1-0 libxdo3 libssl3t64
```

Download the installer ZIP and matching `.zip.sha256` from the Release page,
then verify and install it as the logged-in desktop user (not with `sudo`):

```bash
sha256sum --check asense-v0.3.0-ubuntu-26.04-x86_64-installer-*.zip.sha256
unzip asense-v0.3.0-ubuntu-26.04-x86_64-installer-*.zip
cd asense-v0.3.0-ubuntu-26.04-x86_64-installer-*/
./install.sh
```

The installer requests elevation only for system files, builds the optional
DKMS transport when needed, configures the daemon/socket and verifies the
installation. Re-running a newer installer upgrades in place.

Standalone releases can be reinstalled or upgraded in place. A Debian package
can likewise be reinstalled or upgraded through APT and safely takes ownership
of an older standalone installation. To avoid mixed ownership, the standalone
installer and uninstaller refuse to modify an installation currently managed
by dpkg. Run `sudo apt purge asense` before switching from the Debian package
back to a standalone release; a plain `apt remove` deliberately leaves package
state whose later purge could otherwise affect the standalone installation.

### Arch Linux / AUR

The stable [`asense`](https://aur.archlinux.org/packages/asense) AUR source
package builds ASense and its DKMS transport from the tagged source with the
system Rust toolchain. Clone and install it as your regular build user:

```bash
git clone https://aur.archlinux.org/asense.git
cd asense
makepkg -si
sudo asense-configure-user "$USER"
```

The final command explicitly selects the desktop account allowed to use the
private ASense control socket; the package never guesses the packager's user.
Remove it with `sudo pacman -Rns asense`.

Launch ASense from the application menu or run:

```bash
asense
```

Create a local read-only report for a GitHub issue with:

```bash
asense probe > asense-probe.json
asense probe --summary
```

Close the ASense window first so the one-shot probe can use the daemon's
single control session. The first command creates the authoritative schema-3
JSON attachment. The second prints a compact human-readable view of a fresh
capture; it does not replace the JSON evidence.

The report contains model, profile, fan, known WMI and known HID capability
data. It sends `HELLO 2` followed by the fixed read-only `DIAG PASSIVE` request
to the local daemon. It does not call general capability discovery, send an
ENEK target selector or invoke a setter. It performs no upload. The JSON omits
serials, UUIDs, hostname, user and network identity, boot and storage identity,
HID serial/physical paths, journals, raw ACPI tables, absolute device paths and
the process environment. Review it before sharing; the exact collection,
privacy and support-state contract is documented in
[Probe privacy and support workflow](docs/PROBE_PRIVACY.md).

Uninstall with the copy retained by the installer:

```bash
/usr/libexec/asense/uninstall.sh
```

Uninstall returns an active fan session to Auto, removes ASense services,
DKMS/HWDB/udev integration and the desktop entry. Other firmware settings
(profile, lighting, charge limit and similar choices) remain configured.

### Secure Boot

DKMS uses the distribution signing setup. If module loading reports
`Key was rejected by service`, import the key path printed by DKMS (commonly
`/var/lib/shim-signed/mok/MOK.der`) and complete MOK enrollment after reboot:

```bash
sudo mokutil --import /var/lib/shim-signed/mok/MOK.der
```

## Build from source

Install build dependencies:

```bash
sudo apt update
sudo apt install \
  build-essential rustc cargo pkg-config git dkms \
  "linux-headers-$(uname -r)" libelf-dev \
  libgtk-3-dev libwebkit2gtk-4.1-dev libxdo-dev libssl-dev \
  desktop-file-utils python3 mokutil udev
```

Use the Rust toolchain installed by the operating system. ASense does not
install, pin, downgrade or override it. Then run:

```bash
cargo test --locked
./build.sh
./install.sh ./target/release/asense
```

`build.sh` builds the release GUI and daemon using the installed Rust toolchain
and can be run from any directory. It does not install dependencies or ASense.
The separate installer rebuilds and activates the optional kernel driver.

## Automatic fan curves

See the [PHN16S-71 troubleshooting and session record](docs/PHN16S-71_FAN_CONTROL.md)
for the reported problems, implemented fixes, validation, and remaining live checks.

Select **Auto Curve** to edit separate CPU and GPU temperature/speed points,
then choose **Apply and enable**. The daemon follows the curves once per second,
continues after the window closes, and restores enabled curves after reboot.
Installation enables the daemon at boot after configuring its private socket.
**Firmware Auto** returns control to Acer. Selecting Firmware Auto, Manual, or
Maximum disables the saved curve mode without deleting its points.

The defaults ramp CPU speed from 30% at 45°C through 60% at 65°C to 100% at
85°C; GPU speed reaches 60% at 60°C and 100% at 78°C. Speed increases are
immediate; decreases wait five seconds and then fall by at most 5% per second.
Curves require 2–8 increasing temperature points, nondecreasing percentages
within 20–100%, and a final 100% point by 85°C CPU / 78°C GPU.

The daemon reads fresh CPU package and GPU temperatures. A positively identified
sleeping NVIDIA GPU uses its curve minimum without being woken for telemetry.
By default, CPU ≥92°C or GPU ≥84°C triggers thermal Emergency and Maximum.
The Emergency tab saves separate CPU (60–100°C) and GPU (60–90°C) limits;
Auto Curve and Manual protection both use them. Recovery requires five fresh
complete safe samples below each saved limit minus 5°C. Manual recovery restores
its verified settings; Firmware Auto keeps firmware ownership. Emergency settings
are stored independently in `/var/lib/asense/fan-emergency.json`.
A sensor fault holds the last verified speeds for up to three seconds, then
applies at least 80% to both fans (higher valid curve demand still wins). It
recovers after five fresh complete valid samples. Fan write failures are shown
as control faults, and requested percentages update only after hardware readback.
The daemon shares one retained NVML session across curve and Manual sampling,
with initialization retries capped at 30 seconds. When NVML is required, the
session stays open until software control ends or the GPU sleeps; this may keep
the GPU awake. Optional ASense hwmon channels provide measured RPM and firmware
temperatures when supported. Missing RPM displays as unavailable, never as an
estimate derived from fan percentage. The fan panel
shows background state, requested percentages, faults, and existing RPM gauges.
Settings are stored privately in `/var/lib/asense/fan-curve.json`. Upgrades start
with curves disabled unless valid enabled settings already exist. Older daemons
keep their existing controls and do not expose the curve editor.

Rebuild with `./build.sh`, install with `./install.sh ./target/release/asense`,
then check temperature, requested speed, and RPM during load and cooldown.
Confirm the curve continues after closing ASense and is restored after reboot.
After installing this update, monitor descriptor counts for one hour while
alternating GPU activity and idle:

```bash
sudo python3 scripts/monitor-daemon-fds.py > asense-fds.csv
```

The CSV includes total and NVIDIA descriptors. Restarting the daemon begins a
new measurement; the monitor stops rather than mixing counts from two processes.

## Control behaviour

- profile and WMI settings are read back after writing;
- failed multi-step fan/profile changes use the existing rollback path;
- Manual fan mode is tied to the GUI session and returns to Auto after a
  disconnect;
- a confirmed Maximum request remains active after the GUI closes; daemon
  restart and resume reconciliation return firmware control to Auto;
- HID lighting without a getter shows `State unknown` after discovery and
  `Last applied` after a successful write;
- battery calibration shows only real firmware state and live battery/AC
  telemetry. Keep the AC adapter connected during calibration;
- NVIDIA offsets and the Predator hardware-key mapping remain exact
  PHN16-72 features.

The GUI is unprivileged. Hardware writes run through the root-owned,
socket-activated `asensed` helper. ASense exposes typed profile, fan, lighting
and platform operations; it does not expose a raw WMI/ACPI/EC/HID console.

## Local API

The installed desktop user owns the `0600` Unix socket
`/run/asense-control.sock`. Every UTF-8 command is newline-terminated; the
first command must be `HELLO 2`:

```bash
python3 - <<'PY'
import socket

s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect("/run/asense-control.sock")
f = s.makefile("rwb", buffering=0)
for command in (b"HELLO 2\n", b"CAPS\n"):
    f.write(command)
    print(f.readline(4097).decode().rstrip())
PY
```

The expected replies begin with `OK protocol=2` and `OK caps=1`; the latter is
followed by capability JSON. Every reply is `OK <payload>` or `ERR <message>`.

<!-- markdownlint-disable MD013 -->
| Operation | Command |
| --- | --- |
| Discover | `PING`, `CAPS`, `HARDWARE GET`, `PLATFORM GET` |
| Profile | `PROFILE <raw-token-from-CAPS>` |
| Fans | `FAN AUTO`, `FAN MAXIMUM`, `FAN MANUAL <cpu-20..100> <gpu-20..100>`, `FAN CURVE GET`, `FAN CURVE SET <cpu-points> <gpu-points>`, `FAN EMERGENCY GET`, `FAN EMERGENCY SET <cpu-limit> <gpu-limit>` |
| Lighting | `LIGHTING APPLY <device-id> <OFF\|STATIC\|BREATHING\|NEON> <brightness-0..100> <speed-0..9> <RRGGBB> <-\|RRGGBB,...>`, `LIGHTING POWER <device-id> <ON\|OFF>` |
| Platform toggles | `PLATFORM <BATTERY_LIMIT\|KEYBOARD_TIMEOUT\|BOOT_SOUND\|LCD_OVERRIDE> <ON\|OFF>` |
| Other platform controls | `PLATFORM BATTERY_CALIBRATION <START\|STOP>`, `PLATFORM USB_CHARGING <0\|10\|20\|30>`, `PLATFORM REAR_LOGO <RRGGBB> <brightness-0..100> <ON\|OFF>` |
<!-- markdownlint-enable MD013 -->

Commands are limited to 192 bytes excluding the newline; response content is
limited to 4096 bytes. A normal `ERR` rejects only that command and leaves the
session usable. There is no generic raw-call command or required client
library.

## Development and releases

Release packaging, checksums, CI gates and reproducibility are documented in
[`docs/RELEASING.md`](docs/RELEASING.md). Kernel-backed support follows the
upstream Linux
[`acer-wmi`](https://github.com/torvalds/linux/blob/master/drivers/platform/x86/acer-wmi.c)
driver.

## Donate

If ASense is useful to you, donations are optional:

- **PayPal:** [`paypal.me/fladirm`](https://paypal.me/fladirm) (`@fladirm`)
- **Bitcoin:** [`bc1qqdumr0umlaak7tyrrh0jx729z272fv2jr4t5zp`](bitcoin:bc1qqdumr0umlaak7tyrrh0jx729z272fv2jr4t5zp)

See [`DONATE.md`](DONATE.md) for the PayPal and Bitcoin QR codes.

## Author and license

**Fladirmacht** — <fladirmacht@gmail.com>

ASense is provided **AS IS** and is licensed **GPL-2.0-only**. See
[`LICENSE`](LICENSE). ENEK5130 wire-protocol research was independently
documented by
[`predator-sense`](https://github.com/cleyton1986/predator-sense); ASense uses
its own implementation and tests.

`FAN CURVE SET` uses comma-separated `temperature:percentage` lists, for example
`FAN CURVE SET 45:30,55:40,65:60,75:80,85:100 40:30,50:40,60:60,70:80,78:100`.
It validates, activates, and saves the curve. `FAN CURVE GET` returns JSON with
`config`, `state`, `requested` percentages, and `fault`; these commands are
additions to control protocol version 2.

`FAN EMERGENCY GET` reports saved limits, software protection state, latest
temperatures and sample age, trigger reason, and safe recovery sample count.
`FAN EMERGENCY SET` validates and atomically saves limits without changing curve
points or enabled state. Older daemons disable the Emergency editor gracefully.
