# ASense (PHN16S-71 build)

A Linux control panel for fans, performance profiles, and lighting on Acer
Predator laptops. It is a modified version of
[ASense by fladirm](https://github.com/fladirm/asense).

> **Please read first:** this is not Acer software, and these changes are not
> from ASense's original author. It changes firmware fan settings, installs a
> kernel module, and runs a background service as root.
> The fixes and new features here were vibe-coded (written with AI help) and
> tested only on one laptop, so expect rough edges.
> **Use it at your own risk**, and check your temperatures after installing.

## Tested on

- **Laptop:** Acer Predator PHN16S-71
- **System:** CachyOS with Hyprland, kernel `7.2.8-1-cachyos`

Other models and distributions have not been tested.
**Known issue (this setup only):** the performance-profile buttons are
unavailable on this laptop. This was seen only with the setup above. It does
not mean the original ASense or other laptops and systems have the same problem.

## Why these changes

On this laptop, the normal **Auto** fan mode (controlled by the laptop's
firmware) did not react to load. The fans stayed at the same speed even when the
CPU got hot. The only other choice, **Maximum**, keeps the fans at 100% all the
time, which is too loud for daily use.

**Auto Curve** was added so fan speed follows temperature. Later fixes addressed:
- the `Too many open files (os error 24)` error;
- false emergencies;
- mouse stutter while Auto Curve was running (this seems fixed, but needs more
  testing; lag can still appear).

## What is different from upstream

- **Auto Curve:** a fan mode where fan speed follows CPU and GPU temperature
  using curves you can edit. It keeps running after you close the window, and
  it comes back after a reboot.
- **Emergency limits:** a tab where you set the CPU and GPU temperatures at
  which the fans go to maximum.
- **Simpler UI:** the curve editor opens as a popup, and status messages are
  short and plain.

For details, see [`docs/PHN16S-71_FAN_CONTROL.md`](docs/PHN16S-71_FAN_CONTROL.md).

## 1. Install dependencies (CachyOS)

```bash
sudo pacman -S --needed base-devel git rust pkgconf dkms clang llvm lld \
  linux-cachyos-headers gtk3 webkit2gtk-4.1 libsoup3 xdotool openssl python
```

- Install the headers package that matches your running kernel (check with
  `uname -r`). `linux-cachyos-headers` is for the default CachyOS kernel.
- If you use `rustup`, leave out `rust`.

## 2. Build

```bash
git clone <this repository URL> asense
cd asense
./build.sh
```

## 3. Install

Run this as your normal user, **not** with `sudo`. It asks for your password
when it needs it:

```bash
./install.sh ./target/release/asense
```

The installer:
- installs the app, the background service, and the kernel module;
- checks that the service responds.

Afterward, open ASense from your app launcher or run `asense`.

## Using the fan controls

- **Auto Curve:** click it in the fan panel, edit the points, then click
  **Apply and enable**.
- **Emergency** tab: set the CPU and GPU temperature limits.
- **Firmware Auto:** gives fan control back to the laptop.
- **Maximum:** runs both fans at full speed.

> **Tip for gaming:** if you notice mouse or input lag while Auto Curve is
> running, switch the fans to **Maximum**. Auto Curve adjusts the fans through
> the laptop firmware, and those adjustments can cause short input pauses on
> this laptop. The fix for this seems to work, but it still needs more testing.

## Update

```bash
git pull
./build.sh
./install.sh ./target/release/asense
```

## Uninstall

```bash
/usr/libexec/asense/uninstall.sh
```

This returns the fans to firmware Auto and removes the ASense service and kernel
module. Other firmware settings, such as the profile and lighting, stay as they
are.

## Troubleshooting

- **Check the service:**

  ```bash
  systemctl status asense.service --no-pager --full
  journalctl -u asense.service -b --no-pager
  ```

- **Secure Boot:** if loading the module fails with
  `Key was rejected by service`, enroll the DKMS signing key that DKMS printed,
  then reboot. For example:

  ```bash
  sudo mokutil --import /var/lib/shim-signed/mok/MOK.der
  ```

- **Fan problems and past fixes:** see
  [`docs/PHN16S-71_FAN_CONTROL.md`](docs/PHN16S-71_FAN_CONTROL.md).

## License and credits

Provided **AS IS** under **GPL-2.0-only**. See [`LICENSE`](LICENSE).

- **Original project:** [ASense by fladirm](https://github.com/fladirm/asense).
- **ENEK5130 wire protocol:** research was independently documented by
  [`predator-sense`](https://github.com/cleyton1986/predator-sense). ASense
  uses its own implementation and tests.
