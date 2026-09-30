# Predator PHN16S-71 fan-control troubleshooting and changes

> **Note:** these are changes on top of
> [ASense](https://github.com/fladirm/asense) by fladirm. They are not from
> ASense's original author, and ASense is not Acer software. They were tested only on an
> Acer Predator PHN16S-71 running CachyOS with Hyprland (kernel
> `7.2.8-1-cachyos`). The performance-profile buttons still do not work on this
> machine.

## Why these changes were made

On this machine, the original **Auto** fan mode (firmware Auto) did not react to
load. The fans stayed at the same level. The only other option, **Maximum**,
keeps the fans at 100% all the time. **Auto Curve** was added so fan speed
follows temperature instead.

Some problems appeared only after Auto Curve was added. These included
`Too many open files (os error 24)` and emergencies triggered by missing sensor
readings. They are not bugs in the original ASense. Their causes and fixes are
listed in the table below.

## What is different from upstream

Based on upstream commit `cef7e2b` (ASense 0.3.0). Upstream had no newer
commits as of 2026-09-30. To see every change, run `git diff cef7e2b`.

- **Auto Curve (new):** a fourth fan mode with editable CPU and GPU temperature
  curves. It runs in the background service, so it keeps working after you close
  the window. It is restored after reboot and after sleep.
- **Emergency (extended):**
  - Upstream already switches to Maximum in Manual mode at CPU 92 °C / GPU 84 °C.
  - These changes add an **Emergency** tab to edit those limits.
  - They also add a rule for when fans go back to normal.
  - The same protection now also covers Auto Curve.
- **Simpler UI:**
  - The curve editor opens as a popup.
  - The Auto Curve button shows its status and current speeds.
  - Status messages are short and plain, for example "Sensor fault — holding speed".
- **Fan speed readings:** the kernel module can now report real fan RPM and
  temperatures. If RPM isn't available, the app says "unavailable" instead of
  guessing.
- **Fixes and tools:**
  - GPU temperature reading reuses one NVIDIA session. This addressed the likely
    cause of `os error 24`; the long-running check is still pending.
  - Fewer slow firmware calls, to reduce mouse stutter.
  - `build.sh`, a longer installer health check, and two monitoring scripts.
- **Not changed:** profiles, lighting, battery/USB settings, the hardware probe,
  and packaging, apart from starting the service at boot and the sleep hook.

## Session record

These sessions addressed fan control on an Acer Predator PHN16S-71 on Linux.
This document records the problems, implemented changes, and validation status
as of 2026-09-30. It is a troubleshooting record for this model, not a claim of
complete support for every Acer notebook. PHN16-72 remains the project's
reference-tested platform.

## Problems and results

| Reported problem | Finding and resulting change |
| --- | --- |
| Auto mode stayed at the same fan level during CPU stress; Maximum changed the speed. | Firmware Auto hands cooling back to Acer. Reapplying Auto alone did not resolve the reported behavior. Added daemon-managed **Auto Curve** with separate CPU/GPU temperature curves. The user subsequently confirmed that curve control worked. |
| Recompiling was inconvenient. | Added `build.sh` to build the desktop application and its matching daemon with the locked dependencies. Installation remains a separate step. |
| `Too many open files (os error 24)` appeared after running the controller. | Temperature sampling repeatedly opened and dropped NVML sessions. This was a suspected source of descriptor growth; the live descriptor types were not verified. The daemon now shares a retained session across curve and Manual sampling, with bounded initialization retries. Added a descriptor monitor and session-lifecycle regression tests. |
| Emergency appeared around a reported CPU temperature of 70°C and remained active. | The initial controller treated missing sensor readings as thermal emergencies. Sensor faults, thermal emergencies, and fan-control failures now have distinct states and recovery behavior. CPU utilization does not trigger thermal Emergency. |
| Selecting Auto Curve produced `GPU temperature unavailable: acer hwmon interface was not found; NVML idle release holdoff`. | The sampler deliberately released an idle NVML session, then treated the holdoff as missing temperature data. Active software control now retains its required NVML session instead of deliberately creating this gap. Missing optional Acer sensors do not produce a fault when NVML supplies a valid reading. |
| Fan RPM did not update in the UI. | This machine had no Acer hwmon RPM interface; the existing Gaming-WMI speed values represent percentages, not measured RPM. Added optional firmware RPM/temperature channels to ASense's kernel module, hwmon rediscovery, and live gauge updates. Physical RPM verification on the updated module remains pending. |
| Emergency temperatures needed an editable tab. | Added an **Emergency** tab with separate saved CPU/GPU limits, current readings and sample freshness, activation reason, and recovery progress. The updated tab and settings still require live verification after installation. |

Later journal inspection also found actual CPU readings of **100°C**. Those are
confirmed hot readings, separate from the earlier sensor-fault diagnosis.
Reaching 100% fan speed alone does not mean Emergency is active: a normal curve
can request 100% before its emergency threshold is reached.

## Implemented behavior

### Fan curves and ownership

- **Auto Curve** follows editable CPU/GPU temperature curves once per second.
  It continues after the GUI closes and restores enabled settings at boot.
- Heating increases the requested speed immediately. Cooling waits five seconds,
  then decreases by at most five percentage points per second.
- Default CPU points are `45:30,55:40,65:60,75:80,85:100`.
  Default GPU points are `40:30,50:40,60:60,70:80,78:100`.
- Selecting Firmware Auto, Manual, or Maximum disables the saved curve without
  deleting its points. Firmware Auto returns ownership to Acer.
- Suspend pauses software curves and returns ownership to firmware. Resume
  invalidates stale sampling state and reapplies an enabled curve from fresh
  readings.

Curve settings are stored privately in `/var/lib/asense/fan-curve.json`.

### Safety and recovery

| State | Behavior |
| --- | --- |
| Thermal Emergency | A confirmed CPU or GPU reading at its saved limit requests Maximum. A valid hot reading still counts if the other sensor fails. |
| Sensor hold | Missing required readings hold the last verified speeds for up to three seconds. |
| Sensor fault | After the hold, apply at least 80% to both fans; higher demand from an available valid temperature wins. Without previous verified speeds, attempt the fallback immediately. |
| Control fault | Report a failed fan write/readback separately from a temperature diagnosis. Attempt Maximum as a safety action; record new requested percentages only after confirmation. |

Default Emergency limits are **92°C CPU / 84°C GPU**. The editor accepts
**60–100°C CPU / 60–90°C GPU**, including higher limits selected during these
sessions. Recovery requires five fresh, complete safe samples below each saved
limit minus 5°C. Repeated cached samples do not advance recovery. Changing limits
does not immediately clear an active emergency.

Sensor-fault recovery requires five fresh complete valid samples. A positively
identified sleeping GPU can be treated as sleeping rather than as a missing
sensor. Manual emergency recovery restores its verified settings; disconnecting
a Manual session retains its firmware-Auto cleanup behavior.

Emergency settings are saved independently in
`/var/lib/asense/fan-emergency.json`. The Emergency tab identifies when firmware
owns cooling and software protection is inactive.

### GPU sampling and measured RPM

The daemon keeps one required NVML session during active software fan control.
It releases the session when control ends, the device is positively suspended,
or the session becomes invalid. Initialization retries back off to a maximum
30-second delay. Retaining NVML may keep the GPU awake while software control
needs its temperature; reliable sampling was chosen over deliberate idle
release during these sessions.

The kernel module exposes supported CPU/GPU RPM and temperature channels using
the [upstream Acer sensor protocol](https://github.com/torvalds/linux/blob/master/drivers/platform/x86/acer-wmi.c).
Optional sensor failures do not disable otherwise working fan control. The GUI
rediscovers hwmon interfaces and refreshes measured RPM while Auto Curve runs.
Unsupported or failed RPM readings display as unavailable; percentages are
never converted into estimated RPM.

The existing curve protocol remains unchanged. New commands are:

```text
FAN EMERGENCY GET
FAN EMERGENCY SET <cpu-limit> <gpu-limit>
```

Older daemons retain their existing controls and disable the unsupported
Emergency editor gracefully.

## Build, install, and inspect

Run from the repository root:

```bash
./build.sh
./install.sh ./target/release/asense
```

Installation requires local administrator authentication and also rebuilds the
optional kernel module. Close and reopen ASense afterward to load the new GUI.
Rebuilding alone does not replace the running daemon or kernel module.

Inspect full messages rather than terminal-truncated output:

```bash
systemctl status asense.service --no-pager --full
journalctl -u asense.service -b --no-pager --full
```

Monitor descriptors for one hour while alternating GPU activity and idle:

```bash
sudo python3 scripts/monitor-daemon-fds.py > asense-fds.csv
```

The CSV records total and NVIDIA descriptors. A daemon restart stops the
measurement so counts from different processes are not mixed. The monitor
reports counts; it does not automatically certify stability.

## Validation and remaining checks

The latest implementation passed **295 tests**, with **one ignored test**.
Formatting, whitespace checks, Clippy, release builds, kernel compilation using
`make -C kernel LLVM=1`, and systemd service/socket validation also passed.
Clippy used an allowance for an existing, unrelated
`chunks_exact_to_as_chunks` lint in the probe schema.

Regression coverage includes thousands of samples retaining one session,
idle/sleep transitions, initialization backoff, error 24 at 70°C, partial hot
readings, failed fallback writes, saved limits and recovery hysteresis, older
daemon protocol compatibility, and changing RPM reaching the UI. A test runs
the actual kernel sensor callbacks with a controlled transport to verify RPM
units, temperature units, unsupported sensors, and failed responses.

User confirmation established that Auto Curve began working after the earlier
controller fixes. The latest RPM and Emergency-tab changes were built and
tested, but were not installed from the development session because unattended
sudo authentication was unavailable. Remaining live checks are:

- Verify measured RPM changes during normal load and cooldown.
- Confirm the deliberate NVML idle-holdoff warning no longer appears.
- Save Emergency limits and verify their persistence after service restart.
- Check curve operation after closing the GUI and after reboot/resume.
- Complete the hour-long descriptor measurement.

Validate emergency transitions with injected test readings rather than
deliberately heating the machine to extreme temperatures. Automated tests and
successful compilation do not establish physical RPM support on this firmware.


## Mouse and touchpad stutter investigation

A later report described brief slowdowns every 3–6 seconds on both the USB mouse
and touchpad, only with Auto Curve active, regardless of whether the GUI was open.
The installed daemon matched the development release. A five-second baseline
measured approximately 0.4% userspace CPU and 12% kernel CPU on one core. Single
GPU-temperature and fan-RPM reads took about 0.15–0.21 ms. This suggests firmware
control/discovery work rather than optional RPM reads, but is not proof of which
operation causes the pointer stalls.

The controller now retains discovered interfaces between healthy ticks and
connections. Suspend/resume and control failures trigger rediscovery. Control
readback no longer reads optional RPM, and the NVML safety fallback requests
only temperature instead of full GPU telemetry. Temperature sampling remains at 1 Hz. Unchanged mode/speed settings are audited
every five seconds; changed settings still receive immediate verification. No public protocol or saved setting changes are required.

Build with `./build.sh`, then install with `./install.sh target/release/asense`.
Installation requires local sudo authentication. To enable optional diagnostics,
run `sudo systemctl edit asense.service` and add:

```ini
[Service]
Environment=ASENSE_TIMING=1
```

Restart with `sudo systemctl restart asense.service`. Timing summaries appear
roughly every 30 seconds while the controller runs:

```bash
journalctl -u asense.service -f -o short-monotonic
python3 scripts/monitor-controller-latency.py --duration 120 --label auto-curve > /tmp/asense-curve-cpu.csv
```

Summaries report operation count, total time, maximum time, controller state,
and requested fan percentages. Stages are nested; do not add their totals together.
Normal operation leaves timing disabled. After investigation remove only the
`Environment=ASENSE_TIMING=1` line from the override, then restart the service.

With the GUI closed during each measurement, compare Auto Curve, firmware Auto,
and Maximum for two minutes each using the same desktop activity. Select each
mode in the GUI before closing it and change `--label` and output filename for
each run. Note timestamps of pointer stalls and correlate them with the journal.
Keep temperatures stable and restore the original mode afterward. Do not disable
cooling during a stress workload. If system profiling tools are available, capture
kernel stacks to distinguish firmware execution from NVIDIA driver work.

Live confirmation of smoother pointer movement, reduced kernel CPU usage, and
correct temperature response requires the updated daemon to be installed. The
source changes alone do not establish that this reported problem is resolved.


### Installer handshake timeout

A reinstall completed DKMS module installation but its service smoke test timed
out waiting for `HELLO 2`, after the reload check. Rollback ran; a subsequent
inspection found no installed daemon or ASense system units. That system state
requires reinstalling before further live controller tests.

The daemon now answers the initial protocol handshake and `PING` before optional
hardware discovery and runtime lighting restoration. Cooling maintenance remains
active while probes wait or send fragmented commands. The installer uses one
30-second deadline for connect, handshake, and ping, closes its control socket,
and prints unit status and recent service logs before rollback on a failed probe.
These changes address health-check coupling and improve failure evidence; the
exact cause of the reported delay still needs confirmation during reinstall.


### Five-second control audits

Because lag persisted after the first optimization, Auto Curve now remembers
verified fan targets and audits unchanged firmware mode/speed settings every five
seconds. Temperature sampling, emergency decisions, and rising targets remain
at 1 Hz. Ordinary speed updates write and verify only changed channels, retaining
Manual mode checks before and after the transaction. Partial channel updates do
not postpone the periodic audit of the other channel.

An external firmware override can therefore take up to five seconds to detect.
Startup, resume, profile reset, and control errors discard verified state.
Confirmed emergency Maximum is also audited every five seconds, using modes
alone so missing PWM/RPM does not prevent protection. Failed transitions retry
on the next controller tick; five fresh safe readings still govern recovery.
No saved settings or control-protocol migration is needed.

Repeat the two-minute live comparisons above after installation, with timing
diagnostics enabled temporarily. This further reduction in firmware calls has
regression coverage, but pointer responsiveness still needs live confirmation.


### Five-second GUI fan snapshots and slow-operation diagnostics

The GUI now refreshes RPM, fan mode, and speed telemetry together every five
seconds using a monotonic deadline. Each RPM channel is read once per refresh;
primary RPM values in fan state reuse those readings. RPM text and gauges retain
the same snapshot between refreshes. Failed reads replace old RPM with unavailable
values on refresh. Rediscovery, changed interface identity, and interface removal
invalidate the snapshot immediately. CPU/GPU load and temperature updates retain
their existing cadence; daemon thermal protection is independent of this cache.

With `ASENSE_TIMING=1`, operations exceeding 20 ms also produce slow-operation
messages including stage, elapsed time, state, requested targets, and a suppressed
message count. These messages are limited to one per second; 30-second aggregate
summaries still include every measured operation. Enable timing temporarily using
the service override described above, then compare two-minute GUI-closed and
GUI-open Auto Curve runs. Note pointer-freeze times against monotonic journal
entries. Disable timing afterward by removing only its environment override.

The installed five-second-audit daemon still caused lag with the GUI closed, so
GUI RPM caching alone cannot establish a fix for that symptom. A Manual/Auto
readback mismatch occurred immediately after the installer's reload check. The
reload handler deliberately returns fan ownership to firmware Auto under the
shared mutation lock; startup/reload transitions must be assessed separately from
steady-state Auto Curve timing before diagnosing the remaining pauses.


### Live idle comparison, 2026-09-30

The latest daemon was verified installed, the GUI was closed, and two successive
120-second CPU captures compared Auto Curve with firmware Auto. CPU temperature
was 46°C and the GPU was positively asleep when preparing the comparison.
Original CPU/GPU curve points were saved before switching modes and restored
exactly afterward; verified status returned to running without a fault.

| Mode | Samples | Mean sampled kernel CPU | Mean sampled user CPU | Peak sampled kernel CPU |
| --- | ---: | ---: | ---: | ---: |
| Auto Curve | 117 | 3.368% | 0.091% | 18.867% |
| Firmware Auto | 120 | 0.000% | 0.000% | 0.000% |

Percentages refer to one CPU core. Samples use `/proc` scheduler ticks and roughly
one-second intervals; zero means no measurable daemon CPU ticks in that interval,
not absence of all firmware work. Control status was queried during the curve
capture. This short sequential comparison does not isolate every workload change
or establish that a CPU burst caused an observed pointer pause.

Early Auto Curve bursts were about five seconds apart, matching audits, but later
bursts also appeared between audits. Both periodic readback and target-change
writes/verification remain candidates. Firmware Auto removed the daemon CPU
bursts in this capture. Detailed timing remained disabled because unattended sudo
required local authentication; no individual expensive transaction or pointer
freeze correlation has yet been confirmed. No cooling thresholds or additional
polling intervals were changed for this investigation.

Capture artifacts were saved locally as `/tmp/asense-idle-auto-curve.csv`,
`/tmp/asense-idle-firmware-auto.csv`, and
`/tmp/asense-idle-comparison-summary.json`. They are temporary diagnostics and
are not committed to the repository. The remaining step is to enable the existing
timing override, capture two minutes of operation timings with pointer-pause
timestamps, then remove that override after analysis.


### Measured firmware costs and fewer target changes

With timing enabled, a two-minute summary recorded 24 fan audits averaging
62.488 ms and six single-channel updates averaging 122.717 ms. Individual mode
and speed reads averaged about 15–16 ms. The 120 temperature samples averaged
0.880 ms, showing that firmware control transactions dominated this capture.
Timing stage totals are nested and must not be added together. Pointer-pause
correlation remains unconfirmed.

Auto Curve targets now round upward to the nearest five percentage points,
capped at 100%. This never requests less cooling than the calculated curve and
can request up to four points more. Saved curve points and explicit Manual
percentages remain unchanged. Activation and subsequent ticks use identical
rounding; status reports the rounded requested percentages.

Each decrease of up to five percentage points now waits five seconds after the
previous decrease, rather than decreasing every second after an initial wait.
Rising targets still apply on the next one-second temperature tick. Emergency
limits, sensor fallback, write verification, five-second firmware audits, and
five-second GUI RPM snapshots remain unchanged.

After installation, repeat a two-minute idle capture with the GUI closed and
timing enabled. Compare update counts and total transaction time and record
pointer-pause timestamps. These changes reduce writes but do not remove the
measured audit latency. Remove the temporary timing override after investigation.


### Fewer firmware fan calls (pointer-stutter fix)

This section applies to the tested setup described at the top.

#### Cause

The measurements above showed that Gaming-WMI fan-behavior calls (mode and
speed) cost about 15 ms each. Sensor sys-info calls (temperature and RPM) cost
about 0.2 ms. Fan-behavior calls run firmware (ACPI/EC, possibly SMM) code that
can stall every CPU, which matches the pointer pauses seen only with Auto Curve.
Four sources of these calls were found:

- Each five-second audit read two modes and two speeds, about 62 ms.
- A one-channel speed change took about 122 ms and eight firmware calls:
  - userspace read both modes, wrote, read back the speed, then read both modes again;
  - the kernel store itself did a pre-read, the write, and a readback.
  The userspace reads repeated checks the kernel already made; it returns
  `EIO` on a mismatch.
- CPU package temperature jitters by a few °C at idle. That crossed 5% curve
  steps, so the target kept rising and falling, and each change cost a full
  update.
- The open GUI read fan mode and speed through WMI every five seconds.

#### Changes

- **`src/hardware.rs`:** on the ASense Gaming-WMI interface,
  `update_manual_fan_channels` writes only the changed channels. It does no
  userspace mode or speed reads, because the kernel store verifies each write.
  The upstream acer-wmi PWM backend keeps its full pair-wide Manual
  verification.
- **`src/fan_curve.rs`:**
  - Unchanged Manual targets are audited every 30 s instead of 5 s. The audit
    still reads mode and speed (4 calls), so it catches an external speed
    override.
  - A confirmed emergency Maximum is still audited every 5 s.
  - Decreases use the curve as if the temperature were 3 °C hotter, so jitter
    around a step cannot flip the target.
  - A rise of at least 15 points, or to 100%, applies on the next sample.
    Smaller rises apply once two consecutive fresh samples ask for them, which
    adds at most one second.
  - Cooldown begins after 15 s and drops at most 20 points per 15 s.
- **`kernel/asense_rgb.c`:**
  - The module caches the last confirmed speed for each fan.
  - A mode store, any store error, or resume invalidates the cache.
  - A speed store now makes two firmware calls: the write and a readback.
    Storing a value that is already set makes none.
  - A failed write rolls back to the cached value.
- **`src/telemetry.rs`:** the GUI refreshes RPM every 2 s and fan mode and speed
  every 15 s.
- **`src/timing.rs`:** the duplicated initialization code was merged. Behavior
  is unchanged.

| Path | Before | After |
| --- | --- | --- |
| One-channel speed change | 8 firmware calls, about 122 ms | 2 calls (write + kernel readback) |
| Unchanged-target audit | 4 calls every 5 s | 4 calls every 30 s |
| Idle ±2 °C jitter at a curve step | repeated rise/fall writes | no writes |
| Cooldown from 80% to 30% | 10 writes (5 points every 5 s) | 3 writes (up to 20 points every 15 s) |
| GUI mode/speed snapshot | every 5 s | every 15 s (RPM every 2 s) |

#### Unchanged safety behavior

These are unchanged and are still evaluated every second:

- the saved emergency limits and recovery hysteresis;
- the three-second sensor hold and the 80% sensor-fault fallback;
- control-fault handling, including the Maximum safety action;
- resetting all state on resume.

**Tradeoff:** if something outside ASense changes the Manual mode or speed (for
example, the Predator key), repairing it can take up to 30 seconds.

#### Tests and results

New or updated regression tests cover:

- no fan writes under ±2 °C idle jitter at a curve step;
- immediate large rises and two-sample small rises;
- few, large cooldown steps;
- the Gaming-WMI update touching only the changed speed file, with no mode reads;
- the GUI caching mode and speed across RPM refreshes;
- the real kernel speed-store code, compiled with a counting firmware transport.
  It checks the call counts, the idempotent store, rollback, and cache
  invalidation.

Results in the development session:

- 319 Rust tests pass, with one test ignored, and the 4 installer health-check
  tests pass.
- `cargo fmt --check` and `make -C kernel LLVM=1` succeed.
- Clippy reports only the existing, unrelated `chunks_exact_to_as_chunks` lint.
- `./build.sh` builds both release binaries.

#### Install and verify

The kernel module changed, so reinstall:

```bash
./build.sh
./install.sh target/release/asense
```

Then, with the GUI closed:

1. Enable `ASENSE_TIMING=1` using the service override described earlier. Run a
   two-minute idle Auto Curve capture:
   `python3 scripts/monitor-controller-latency.py --duration 120 --label curve-v2`.
   Expect about four `fan_readback` audits and only a few `fan_update`
   operations. Daemon kernel CPU should be well below the earlier 3.4% mean.
2. Optionally watch firmware interrupts:
   `sudo turbostat --quiet --show SMI --interval 1`. SMI bursts should become rare.
3. Check pointer smoothness by hand.
4. Run a short load such as `stress-ng --cpu 0 -t 60`. Confirm the fans rise
   quickly and then step down in a few large steps after the load ends.
5. Remove the timing override afterward.

**Pending:** these changes have not yet been installed or verified on hardware.
The tests and build results above do not by themselves confirm that the pointer
stutter is resolved. First use after installing suggests the stutter is fixed,
but mouse or input lag can still appear. Until this is confirmed, switch to
Maximum while gaming if lag appears.
