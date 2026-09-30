# Predator PHN16S-71 fan-control troubleshooting and changes

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
