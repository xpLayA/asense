//! Shared curve configuration and daemon-owned cooling control.
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::hardware::{AcerHardware, FanMode, FanSetting, HardwareError};

/// Unchanged Manual targets are re-read from firmware this often. Each audit
/// costs four ~15 ms Gaming-WMI calls, so an external Fn/Predator-key override
/// can take up to this long to be repaired; thermal checks still run at 1 Hz.
const CONTROL_AUDIT_INTERVAL: Duration = Duration::from_secs(30);
/// Confirmed emergency Maximum is rare and safety-critical: audit it sooner.
const EMERGENCY_AUDIT_INTERVAL: Duration = Duration::from_secs(5);
use crate::mutation_lock::MutationGuard;
use crate::nvidia::{NvidiaController, NvidiaRuntimeStatus, discover_nvidia_pci_device};

pub const SETTINGS_PATH: &str = "/var/lib/asense/fan-curve.json";
pub const PAUSE_PATH: &str = "/run/asense-curve-paused";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurvePoint {
    pub temperature: u8,
    pub percent: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurveConfig {
    pub schema: u8,
    pub enabled: bool,
    pub cpu: Vec<CurvePoint>,
    pub gpu: Vec<CurvePoint>,
}

impl Default for CurveConfig {
    fn default() -> Self {
        let points = |values: &[(u8, u8)]| {
            values
                .iter()
                .map(|&(temperature, percent)| CurvePoint {
                    temperature,
                    percent,
                })
                .collect()
        };
        Self {
            schema: 1,
            enabled: false,
            cpu: points(&[(45, 30), (55, 40), (65, 60), (75, 80), (85, 100)]),
            gpu: points(&[(40, 30), (50, 40), (60, 60), (70, 80), (78, 100)]),
        }
    }
}

impl CurveConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 1 {
            return Err("unsupported fan curve schema".into());
        }
        for (name, points, limit) in [("CPU", &self.cpu, 85), ("GPU", &self.gpu, 78)] {
            if !(2..=8).contains(&points.len()) {
                return Err(format!("{name}: use 2–8 points"));
            }
            if points
                .iter()
                .any(|p| !(20..=limit).contains(&p.temperature) || !(20..=100).contains(&p.percent))
            {
                return Err(format!(
                    "{name}: temperatures must be 20–{limit}°C and speeds 20–100%"
                ));
            }
            if points
                .windows(2)
                .any(|p| p[0].temperature >= p[1].temperature || p[0].percent > p[1].percent)
            {
                return Err(format!(
                    "{name}: temperatures must increase and speeds must not decrease"
                ));
            }
            if points.last().is_none_or(|p| p.percent != 100) {
                return Err(format!("{name}: final point must be 100%"));
            }
        }
        Ok(())
    }

    pub fn from_tokens(cpu: &str, gpu: &str) -> Result<Self, String> {
        let parse = |text: &str| -> Result<Vec<CurvePoint>, String> {
            if text.len() > 80 {
                return Err("curve list too long".into());
            }
            text.split(',')
                .map(|pair| {
                    let (t, p) = pair
                        .split_once(':')
                        .ok_or("use temperature:percentage pairs")?;
                    Ok(CurvePoint {
                        temperature: t.parse().map_err(|_| "invalid temperature")?,
                        percent: p.parse().map_err(|_| "invalid percentage")?,
                    })
                })
                .collect()
        };
        let result = Self {
            schema: 1,
            enabled: true,
            cpu: parse(cpu)?,
            gpu: parse(gpu)?,
        };
        result.validate()?;
        Ok(result)
    }

    pub fn token(points: &[CurvePoint]) -> String {
        points
            .iter()
            .map(|p| format!("{}:{}", p.temperature, p.percent))
            .collect::<Vec<_>>()
            .join(",")
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.to_string()),
        };
        let mut bytes = Vec::new();
        file.take(4097)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 4096 {
            return Err("fan curve settings exceed size limit".into());
        }
        let config: Self = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        save_settings(self, path)
    }
}

fn save_settings(config: &impl Serialize, path: &Path) -> Result<(), String> {
    let parent = path.parent().ok_or("missing settings directory")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    let mut created = false;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        created = true;
        file.write_all(&serde_json::to_vec(config).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&tmp, path).map_err(|e| e.to_string())?;
        fs::File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())
    })();
    if result.is_err() && created {
        let _ = fs::remove_file(tmp);
    }
    result
}

pub const EMERGENCY_SETTINGS_PATH: &str = "/var/lib/asense/fan-emergency.json";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmergencyConfig {
    pub schema: u8,
    pub cpu_limit: u8,
    pub gpu_limit: u8,
}
impl Default for EmergencyConfig {
    fn default() -> Self {
        Self {
            schema: 1,
            cpu_limit: 92,
            gpu_limit: 84,
        }
    }
}
impl EmergencyConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 1
            || !(60..=100).contains(&self.cpu_limit)
            || !(60..=90).contains(&self.gpu_limit)
        {
            return Err("emergency limits: CPU 60–100°C, GPU 60–90°C, schema 1 required".into());
        }
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.to_string()),
        };
        let mut bytes = Vec::new();
        file.take(4097)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 4096 {
            return Err("emergency settings exceed size limit".into());
        }
        let config: Self = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        config.validate()?;
        Ok(config)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        save_settings(self, path)
    }
    pub(crate) fn safe(&self, reading: &TemperatureReading) -> bool {
        reading.complete().is_ok_and(|(cpu, gpu)| {
            cpu < f32::from(self.cpu_limit - 5)
                && gpu.is_none_or(|t| t < f32::from(self.gpu_limit - 5))
        })
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EmergencyStatus {
    pub config: EmergencyConfig,
    pub state: String,
    pub cpu_temperature: Option<f32>,
    pub gpu_temperature: Option<f32>,
    pub gpu_sleeping: bool,
    pub sample_age_seconds: Option<u64>,
    pub reason: Option<String>,
    pub safe_samples: u8,
}
impl EmergencyStatus {
    pub(crate) fn snapshot(
        config: EmergencyConfig,
        state: String,
        reading: Option<&TemperatureReading>,
        reason: Option<String>,
        safe_samples: u8,
    ) -> Self {
        Self {
            config,
            state,
            cpu_temperature: reading.and_then(|r| r.cpu.as_ref().ok().copied()),
            gpu_temperature: reading.and_then(|r| r.gpu.as_ref().ok().copied().flatten()),
            gpu_sleeping: reading.is_some_and(|r| matches!(r.gpu, Ok(None))),
            sample_age_seconds: reading
                .and_then(|r| r.sampled_at)
                .map(|t| t.elapsed().as_secs()),
            reason,
            safe_samples,
        }
    }
}

pub fn interpolate(points: &[CurvePoint], temperature: f32) -> u8 {
    if temperature <= points[0].temperature as f32 {
        return points[0].percent;
    }
    for pair in points.windows(2) {
        if temperature <= pair[1].temperature as f32 {
            let fraction = (temperature - pair[0].temperature as f32)
                / (pair[1].temperature - pair[0].temperature) as f32;
            return (pair[0].percent as f32 + fraction * (pair[1].percent - pair[0].percent) as f32)
                .ceil() as u8;
        }
    }
    points.last().unwrap().percent
}

/// Independent results preserve a hot reading even when the other sensor fails.
#[derive(Clone, Debug)]
pub(crate) struct TemperatureReading {
    pub cpu: Result<f32, String>,
    pub gpu: Result<Option<f32>, String>,
    pub(crate) sampled_at: Option<Instant>,
}
impl TemperatureReading {
    pub fn complete(&self) -> Result<(f32, Option<f32>), String> {
        match (&self.cpu, &self.gpu) {
            (Ok(cpu), Ok(gpu)) => Ok((*cpu, *gpu)),
            _ => Err(self.errors()),
        }
    }
    fn errors(&self) -> String {
        self.cpu
            .as_ref()
            .err()
            .into_iter()
            .chain(self.gpu.as_ref().err())
            .cloned()
            .collect::<Vec<_>>()
            .join("; ")
    }
    pub fn thermal_fault(&self, limits: &EmergencyConfig) -> Option<String> {
        if let Ok(cpu) = self.cpu
            && cpu >= f32::from(limits.cpu_limit)
        {
            return Some(format!(
                "CPU {cpu:.1}°C reached emergency limit {}°C",
                limits.cpu_limit
            ));
        }
        if let Ok(Some(gpu)) = self.gpu
            && gpu >= f32::from(limits.gpu_limit)
        {
            return Some(format!(
                "GPU {gpu:.1}°C reached emergency limit {}°C",
                limits.gpu_limit
            ));
        }
        None
    }
}
impl From<Result<(f32, Option<f32>), String>> for TemperatureReading {
    fn from(value: Result<(f32, Option<f32>), String>) -> Self {
        match value {
            Ok((cpu, gpu)) => Self {
                cpu: Ok(cpu),
                gpu: Ok(gpu),
                sampled_at: None,
            },
            Err(error) => Self {
                cpu: Err(error.clone()),
                gpu: Err(error),
                sampled_at: None,
            },
        }
    }
}

trait TemperatureSession {
    fn sample(&self) -> (Result<f32, String>, bool, bool);
}
impl TemperatureSession for NvidiaController {
    fn sample(&self) -> (Result<f32, String>, bool, bool) {
        let temperature = self.temperature();
        let lost = temperature
            .as_ref()
            .err()
            .is_some_and(|e| e.invalidates_session());
        let result = temperature.map_err(|e| e.to_string()).and_then(|value| {
            value
                .filter(|v| *v <= 120)
                .map(|v| v as f32)
                .ok_or_else(|| "NVML temperature missing or invalid".into())
        });
        (result, lost, false)
    }
}
type OpenTemperatureSession =
    fn(&crate::nvidia::NvidiaPciDevice, &Path) -> Result<Box<dyn TemperatureSession>, String>;
fn open_temperature_session(
    pci: &crate::nvidia::NvidiaPciDevice,
    root: &Path,
) -> Result<Box<dyn TemperatureSession>, String> {
    NvidiaController::discover_telemetry(pci, root, false)
        .map(|v| Box::new(v) as Box<dyn TemperatureSession>)
        .map_err(|e| e.to_string())
}

/// Owned by the daemon, never by a tick or a client. NVML stays loaded while
/// sampling; repeatedly unloading its library can leak driver descriptors.
pub(crate) struct TemperatureSampler {
    nvidia: Option<Box<dyn TemperatureSession>>,
    open: OpenTemperatureSession,
    cached: Option<TemperatureReading>,
    device: Option<(String, crate::nvidia::PciIdentity)>,
    retry_at: Instant,
    retry_delay: u64,
    last_error: String,
    last_sample: Option<Instant>,
}
impl Default for TemperatureSampler {
    fn default() -> Self {
        Self {
            nvidia: None,
            open: open_temperature_session,
            cached: None,
            device: None,
            retry_at: Instant::now(),
            retry_delay: 1,
            last_error: String::new(),
            last_sample: None,
        }
    }
}
impl TemperatureSampler {
    pub fn invalidate(&mut self) {
        self.nvidia = None;
        self.cached = None;
        self.device = None;
        self.retry_at = Instant::now();
        self.retry_delay = 1;
    }
    fn failed(&mut self, error: String, now: Instant) {
        self.last_error = error;
        self.retry_at = now + Duration::from_secs(self.retry_delay);
        self.retry_delay = (self.retry_delay * 2).min(30);
    }
    pub fn read(&mut self, hardware: &AcerHardware) -> TemperatureReading {
        self.read_at(hardware, Instant::now())
    }
    fn read_at(&mut self, hardware: &AcerHardware, now: Instant) -> TemperatureReading {
        if self
            .last_sample
            .is_some_and(|last| now.duration_since(last) < Duration::from_secs(1))
            && let Some(reading) = &self.cached
        {
            return reading.clone();
        }
        if self
            .last_sample
            .is_some_and(|last| now.duration_since(last) > Duration::from_secs(3))
        {
            self.invalidate();
        }
        self.last_sample = Some(now);
        let _timing = crate::timing::scope("temperature_sample");
        let root = hardware.root();
        let valid = |v: f32| v.is_finite() && (0.0..=120.0).contains(&v);
        let acer = |channel| -> Result<f32, String> {
            let value = hardware
                .read_acer_temp_millidegrees(channel)
                .map_err(|e| e.to_string())? as f32
                / 1000.0;
            if valid(value) {
                Ok(value)
            } else {
                Err(format!("Acer temp{channel}: invalid temperature {value}"))
            }
        };
        let package = crate::telemetry::find_labeled_temperature(root, "coretemp", "Package id 0")
            .map_err(|e| e.to_string())
            .and_then(|v| {
                v.filter(|v| valid(*v))
                    .ok_or_else(|| "coretemp Package id 0 missing or invalid".into())
            });
        let cpu = package.or_else(|package_error| {
            acer(1).map_err(|e| format!("CPU temperature unavailable: {package_error}; {e}"))
        });
        let pci = discover_nvidia_pci_device(root);
        let identity = pci.as_ref().map(|p| (p.bus_id.clone(), p.identity));
        if self.device != identity {
            self.invalidate();
            self.retry_at = now;
            self.device = identity;
        }
        let gpu = if pci
            .as_ref()
            .is_some_and(|p| p.runtime_status == NvidiaRuntimeStatus::Suspended)
        {
            self.nvidia = None;
            Ok(None)
        } else {
            match acer(2) {
                Ok(value) => Ok(Some(value)),
                Err(acer_error) => self
                    .read_gpu(pci.as_ref(), root, now)
                    .map(Some)
                    .map_err(|e| format!("GPU temperature unavailable: {acer_error}; {e}")),
            }
        };
        let reading = TemperatureReading {
            cpu,
            gpu,
            sampled_at: Some(now),
        };
        self.cached = Some(reading.clone());
        reading
    }

    fn read_gpu(
        &mut self,
        pci: Option<&crate::nvidia::NvidiaPciDevice>,
        root: &Path,
        now: Instant,
    ) -> Result<f32, String> {
        let pci = pci.ok_or("NVIDIA PCI device unavailable")?;
        if !pci.runtime_status.permits_live_nvml() {
            self.nvidia = None;
            self.retry_at = now;
            return Err(format!("NVIDIA runtime state {:?}", pci.runtime_status));
        }
        if self.nvidia.is_none() {
            if now < self.retry_at {
                return Err(format!("NVML retry pending: {}", self.last_error));
            }
            match (self.open)(pci, root) {
                Ok(controller) => {
                    self.nvidia = Some(controller);
                    self.retry_delay = 1;
                }
                Err(e) => {
                    self.failed(e.to_string(), now);
                    return Err(self.last_error.clone());
                }
            }
        }
        let (temperature, lost, _idle) = self.nvidia.as_ref().unwrap().sample();
        // Safety sampling owns its session until control ends or the device sleeps.
        // Releasing an idle session deliberately must not look like sensor failure.
        if lost {
            self.nvidia = None;
        }
        if lost {
            let error = temperature
                .err()
                .unwrap_or_else(|| "NVML session lost".into());
            self.failed(error.clone(), now);
            Err(error)
        } else {
            temperature
        }
    }
}
#[cfg(test)]
fn temperatures(hardware: &AcerHardware) -> Result<(f32, Option<f32>), String> {
    TemperatureSampler::default().read(hardware).complete()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurveStatus {
    pub config: CurveConfig,
    pub state: String,
    pub requested: Option<[u8; 2]>,
    pub fault: Option<String>,
}

/// Round software-curve targets upward; explicit Manual requests remain exact.
fn curve_speed_step(percent: u8) -> u8 {
    percent.min(100).div_ceil(5) * 5
}

/// Decreases are computed as if the sensor were this much hotter, so idle
/// jitter around a curve step cannot toggle the fan target.
const DECREASE_HYSTERESIS_C: f32 = 3.0;
/// Rises of at least this many points (or to 100%) apply on the first sample;
/// smaller rises must be requested by two consecutive fresh samples.
const IMMEDIATE_RISE_POINTS: u8 = 15;
const DECREASE_DELAY: Duration = Duration::from_secs(15);
const MAX_DECREASE_POINTS: u8 = 20;

/// Every target change costs several ~15 ms firmware calls that can stall
/// input handling, so the ramp favors few, larger, cooling-biased changes.
#[derive(Default)]
struct Ramp {
    current: Option<u8>,
    rise_pending: bool,
    lower_since: Option<Instant>,
}

impl Ramp {
    /// `up` is the curve target at the measured temperature; `down` is the
    /// target with decrease hysteresis applied (always `>= up`).
    fn target(&mut self, up: u8, down: u8, now: Instant) -> u8 {
        let Some(current) = self.current else {
            self.current = Some(up);
            return up;
        };
        let next = if up > current {
            self.lower_since = None;
            if self.rise_pending || up >= 100 || up - current >= IMMEDIATE_RISE_POINTS {
                self.rise_pending = false;
                up
            } else {
                self.rise_pending = true;
                current
            }
        } else if down < current {
            self.rise_pending = false;
            let since = *self.lower_since.get_or_insert(now);
            if now.duration_since(since) >= DECREASE_DELAY {
                self.lower_since = Some(now);
                down.max(current.saturating_sub(MAX_DECREASE_POINTS))
            } else {
                current
            }
        } else {
            self.rise_pending = false;
            self.lower_since = None;
            current
        };
        self.current = Some(next);
        next
    }
}

pub(crate) struct CurveRuntime {
    pub status: CurveStatus,
    pub limits: EmergencyConfig,
    last_reading: Option<TemperatureReading>,
    emergency_reason: Option<String>,
    logged_fault: Option<String>,
    next_tick: Instant,
    last_tick: Instant,
    ramps: [Ramp; 2],
    emergency: bool,
    safe_samples: u8,
    paused: bool,
    sensor_fault_since: Option<Instant>,
    sensor_fallback: bool,
    last_processed_sample: Option<Instant>,
    verified_manual: Option<[u8; 2]>,
    maximum_verified: bool,
    next_audit: Instant,
}

impl CurveRuntime {
    pub fn new(config: CurveConfig) -> Self {
        Self {
            limits: EmergencyConfig::default(),
            last_reading: None,
            emergency_reason: None,
            logged_fault: None,
            status: CurveStatus {
                state: if config.enabled {
                    "starting"
                } else {
                    "disabled"
                }
                .into(),
                config,
                requested: None,
                fault: None,
            },
            next_tick: Instant::now(),
            last_tick: Instant::now(),
            ramps: [Ramp::default(), Ramp::default()],
            emergency: false,
            safe_samples: 0,
            paused: false,
            sensor_fault_since: None,
            sensor_fallback: false,
            last_processed_sample: None,
            verified_manual: None,
            maximum_verified: false,
            next_audit: Instant::now(),
        }
    }

    pub fn hardware_unavailable(&mut self, error: String) {
        if self.status.fault.as_deref() != Some(&error) {
            eprintln!("asense fan curve: {error}");
        }
        self.status.state = "unavailable".into();
        self.status.fault = Some(error);
        self.status.requested = None;
        self.invalidate_control();
    }

    pub fn reset(&mut self) {
        self.next_tick = Instant::now();
        self.status.requested = None;
        self.ramps = [Ramp::default(), Ramp::default()];
        self.invalidate_control();
    }

    fn invalidate_control(&mut self) {
        self.verified_manual = None;
        self.maximum_verified = false;
        self.next_audit = Instant::now();
    }

    fn ensure_maximum(
        &mut self,
        hardware: &AcerHardware,
        now: Instant,
    ) -> Result<(), HardwareError> {
        let result = (|| {
            if self.maximum_verified {
                if now < self.next_audit {
                    return Ok(());
                }
                if hardware
                    .read_fan_modes()
                    .is_ok_and(|modes| modes == [FanMode::Maximum; 2])
                {
                    self.next_audit = now + EMERGENCY_AUDIT_INTERVAL;
                    return Ok(());
                }
            }
            hardware.apply_maximum_failsafe()?;
            self.verified_manual = None;
            self.maximum_verified = true;
            self.next_audit = now + EMERGENCY_AUDIT_INTERVAL;
            Ok(())
        })();
        if result.is_err() {
            self.invalidate_control();
        }
        result
    }

    pub fn disable(&mut self, path: &Path) -> Result<(), String> {
        if self.status.config.enabled {
            let mut config = self.status.config.clone();
            config.enabled = false;
            config.save(path)?;
            let limits = self.limits.clone();
            *self = Self::new(config);
            self.limits = limits;
        }
        Ok(())
    }

    pub fn activate(
        &mut self,
        config: CurveConfig,
        hardware: &AcerHardware,
        path: &Path,
        sampler: &mut TemperatureSampler,
    ) -> Result<(), String> {
        config.validate()?;
        let reading = sampler.read(hardware);
        if let Some(error) = reading.thermal_fault(&self.limits) {
            return Err(error);
        }
        let (cpu, gpu) = reading.complete()?;
        let targets = [
            interpolate(&config.cpu, cpu),
            gpu.map_or(config.gpu[0].percent, |t| interpolate(&config.gpu, t)),
        ]
        .map(curve_speed_step);
        hardware
            .apply_fan_setting(FanSetting::Manual {
                cpu_percent: targets[0],
                gpu_percent: targets[1],
            })
            .map_err(|e| e.to_string())?;
        if let Err(error) = config.save(path) {
            let recovery = hardware.apply_fan_setting(FanSetting::Automatic);
            self.reset();
            return Err(format!(
                "cannot save curve: {error}; firmware reset: {recovery:?}"
            ));
        }
        let limits = self.limits.clone();
        *self = Self::new(config);
        self.limits = limits;
        self.last_reading = Some(reading);
        self.status.requested = Some(targets);
        self.verified_manual = Some(targets);
        self.next_audit = Instant::now() + CONTROL_AUDIT_INTERVAL;
        self.ramps[0].current = Some(targets[0]);
        self.ramps[1].current = Some(targets[1]);
        self.status.state = "running".into();
        self.next_tick = Instant::now() + Duration::from_secs(1);
        Ok(())
    }

    pub fn tick(&mut self, hardware: &AcerHardware, sampler: &mut TemperatureSampler) {
        let now = Instant::now();
        if !self.status.config.enabled || now < self.next_tick {
            return;
        }
        let _timing = crate::timing::scope("curve_tick");
        self.next_tick = now + Duration::from_secs(1);
        if Path::new(PAUSE_PATH).exists() {
            self.invalidate_control();
            sampler.invalidate();
            self.paused = true;
            self.status.state = "paused".into();
            return;
        }
        if self.paused || now.duration_since(self.last_tick) > Duration::from_secs(3) {
            self.reset();
            sampler.invalidate();
        }
        self.paused = false;
        self.last_tick = now;
        let result = MutationGuard::acquire()
            .inspect_err(|_| {
                self.invalidate_control();
                self.status.state = "control-fault".into();
            })
            .and_then(|_guard| {
                // Recheck under the lock: suspend may have begun while waiting.
                if Path::new(PAUSE_PATH).exists() {
                    self.invalidate_control();
                    self.paused = true;
                    self.status.state = "paused".into();
                    return Ok(());
                }
                self.step(hardware, sampler.read(hardware), now)
            });
        self.next_tick = now + Duration::from_secs(1);
        if let Err(error) = result {
            if self.logged_fault.as_deref() != Some(&self.status.state) {
                eprintln!("asense fan curve: {error}");
            }
            self.logged_fault = Some(self.status.state.clone());
            self.status.fault = Some(error);
        } else if self.status.state == "running" {
            self.logged_fault = None;
        }
        crate::timing::context(&self.status.state, self.status.requested);
    }

    pub(crate) fn set_limits(&mut self, limits: EmergencyConfig) {
        self.limits = limits;
        self.safe_samples = 0;
        self.next_tick = Instant::now();
    }
    pub(crate) fn emergency_status(&self) -> EmergencyStatus {
        EmergencyStatus::snapshot(
            self.limits.clone(),
            if self.status.config.enabled {
                self.status.state.clone()
            } else {
                "firmware-managed".into()
            },
            self.last_reading.as_ref(),
            if self.emergency {
                self.emergency_reason.clone()
            } else {
                self.status.fault.clone()
            },
            if self.emergency { self.safe_samples } else { 0 },
        )
    }

    fn step(
        &mut self,
        hardware: &AcerHardware,
        reading: impl Into<TemperatureReading>,
        now: Instant,
    ) -> Result<(), String> {
        let reading = reading.into();
        if reading.sampled_at.is_some() && reading.sampled_at == self.last_processed_sample {
            return Ok(());
        }
        self.last_processed_sample = reading.sampled_at;
        self.last_reading = Some(reading.clone());
        if let Some(error) = reading.thermal_fault(&self.limits) {
            self.emergency = true;
            self.emergency_reason = Some(error.clone());
            self.safe_samples = 0;
            self.status.state = "emergency".into();
            crate::timing::context("emergency", Some([100, 100]));
            match self.ensure_maximum(hardware, now) {
                Ok(()) => self.status.requested = Some([100, 100]),
                Err(e) => {
                    self.status.state = "control-fault".into();
                    return Err(format!("{error}; Maximum failed: {e}"));
                }
            }
            self.status.fault = Some(error.clone());
            return Err(error);
        }
        let complete = reading.complete();
        if self.emergency {
            if self.limits.safe(&reading) {
                self.safe_samples += 1;
            } else {
                self.safe_samples = 0;
            }
            if self.safe_samples < 5 {
                self.ensure_maximum(hardware, now).map_err(|e| {
                    self.status.state = "control-fault".into();
                    format!("Maximum failed: {e}")
                })?;
                self.status.requested = Some([100, 100]);
                self.status.state = "emergency".into();
                return Ok(());
            }
            self.emergency = false;
            self.emergency_reason = None;
            self.sensor_fault_since = None;
            self.sensor_fallback = false;
            self.reset();
        }
        let mut fallback = false;
        if complete.is_err() {
            self.safe_samples = 0;
            let since = *self.sensor_fault_since.get_or_insert(now);
            let error = reading.errors();
            self.status.fault = Some(error.clone());
            if !self.sensor_fallback
                && now.duration_since(since) < Duration::from_secs(3)
                && self.status.requested.is_some()
            {
                self.status.state = "sensor-hold".into();
                return Err(format!(
                    "{error}; holding last verified speeds (up to 3 seconds)"
                ));
            }
            self.sensor_fallback = true;
            fallback = true;
        } else if self.sensor_fault_since.is_some() {
            self.safe_samples += 1;
            if self.safe_samples < 5 {
                if !self.sensor_fallback
                    && self
                        .sensor_fault_since
                        .is_some_and(|since| now.duration_since(since) < Duration::from_secs(3))
                    && self.status.requested.is_some()
                {
                    self.status.state = "sensor-hold".into();
                    return Ok(());
                }
                self.sensor_fallback = true;
                fallback = true;
            } else {
                self.sensor_fault_since = None;
                self.sensor_fallback = false;
                self.safe_samples = 0;
                self.ramps = [Ramp::default(), Ramp::default()];
            }
        }
        let curve_targets = |offset: f32| {
            [
                reading.cpu.as_ref().map_or(80, |cpu| {
                    interpolate(&self.status.config.cpu, *cpu + offset)
                }),
                reading.gpu.as_ref().map_or(80, |gpu| {
                    gpu.map_or(self.status.config.gpu[0].percent, |t| {
                        interpolate(&self.status.config.gpu, t + offset)
                    })
                }),
            ]
            .map(curve_speed_step)
        };
        let desired = curve_targets(0.0);
        let mut targets = if fallback {
            [desired[0].max(80), desired[1].max(80)]
        } else {
            let down = curve_targets(DECREASE_HYSTERESIS_C);
            [
                self.ramps[0].target(desired[0], down[0], now),
                self.ramps[1].target(desired[1], down[1], now),
            ]
        };
        crate::timing::context(&self.status.state, Some(targets));
        let applied = (|| -> Result<(), HardwareError> {
            if self.verified_manual.is_none() || now >= self.next_audit {
                let state = hardware.read_fan_control_state()?;
                self.next_audit = now + CONTROL_AUDIT_INTERVAL;
                self.verified_manual = if state.cpu.mode == Some(FanMode::Manual)
                    && state.gpu.mode == Some(FanMode::Manual)
                {
                    Some([
                        crate::hardware::pwm_to_percent(state.cpu.pwm_raw),
                        crate::hardware::pwm_to_percent(state.gpu.pwm_raw),
                    ])
                } else {
                    None
                };
            }
            if let Some(previous) = self.verified_manual {
                if previous != targets {
                    hardware.update_manual_fan_channels(
                        (previous[0] != targets[0]).then_some(targets[0]),
                        (previous[1] != targets[1]).then_some(targets[1]),
                    )?;
                }
            } else {
                if !fallback {
                    targets = desired;
                }
                hardware.apply_fan_setting(FanSetting::Manual {
                    cpu_percent: targets[0],
                    gpu_percent: targets[1],
                })?;
                self.next_audit = now + CONTROL_AUDIT_INTERVAL;
            }
            self.verified_manual = Some(targets);
            self.maximum_verified = false;
            Ok(())
        })();
        if let Err(error) = applied {
            self.invalidate_control();
            self.status.state = "control-fault".into();
            // Maximum is an independent best-effort hardware safety action,
            // not a temperature diagnosis. Only record it after mode readback.
            if hardware.apply_maximum_failsafe().is_ok() {
                self.status.requested = Some([100, 100]);
            }
            return Err(format!("fan control failed: {error}"));
        }
        self.status.requested = Some(targets);
        self.ramps[0].current = Some(targets[0]);
        self.ramps[1].current = Some(targets[1]);
        if fallback {
            self.status.state = "sensor-fault".into();
            let error = if complete.is_err() {
                reading.errors()
            } else {
                "waiting for five fresh valid samples".into()
            };
            self.status.fault = Some(format!(
                "{error}; verified fallback CPU {}%, GPU {}%",
                targets[0], targets[1]
            ));
        } else {
            self.status.state = "running".into();
            self.status.fault = None;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        root: std::path::PathBuf,
        hardware: AcerHardware,
    }
    impl Fixture {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "asense-curve-hw-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let dmi = root.join("sys/class/dmi/id");
            let hwmon = root.join("sys/class/hwmon/hwmon9");
            fs::create_dir_all(&dmi).unwrap();
            fs::create_dir_all(&hwmon).unwrap();
            fs::write(dmi.join("sys_vendor"), "Acer").unwrap();
            fs::write(dmi.join("product_name"), "Predator PHN16S-71").unwrap();
            for (name, value) in [
                ("name", "acer"),
                ("fan1_input", "3100"),
                ("fan2_input", "2800"),
                ("pwm1", "128"),
                ("pwm2", "128"),
                ("pwm1_enable", "2"),
                ("pwm2_enable", "2"),
                ("temp1_input", "65000"),
                ("temp2_input", "55000"),
            ] {
                fs::write(hwmon.join(name), value).unwrap();
            }
            let hardware = AcerHardware::discover_at(&root).unwrap();
            Self { root, hardware }
        }
        fn hwmon(&self) -> std::path::PathBuf {
            self.root.join("sys/class/hwmon/hwmon9")
        }
        fn config_path(&self) -> std::path::PathBuf {
            self.root.join("state/fan-curve.json")
        }
        fn runtime(&self) -> CurveRuntime {
            CurveRuntime::new(CurveConfig {
                enabled: true,
                ..CurveConfig::default()
            })
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn explicit_manual_percentages_are_not_quantized() {
        let f = Fixture::new();
        let state = f
            .hardware
            .apply_fan_setting(FanSetting::Manual {
                cpu_percent: 31,
                gpu_percent: 43,
            })
            .unwrap();
        assert_eq!(crate::hardware::pwm_to_percent(state.cpu.pwm_raw), 31);
        assert_eq!(crate::hardware::pwm_to_percent(state.gpu.pwm_raw), 43);
    }

    #[test]
    fn curve_speed_steps_round_up_without_exceeding_maximum() {
        for percent in 0..=100 {
            let stepped = curve_speed_step(percent);
            assert!(stepped >= percent && stepped <= 100);
            assert!(stepped - percent < 5);
            assert_eq!(stepped % 5, 0);
        }
        assert_eq!(curve_speed_step(31), 35);
        assert_eq!(curve_speed_step(96), 100);
    }

    #[test]
    fn activation_and_ticks_use_same_rounded_targets_and_preserve_points() {
        let f = Fixture::new();
        fs::write(f.hwmon().join("temp1_input"), "66000").unwrap();
        fs::write(f.hwmon().join("temp2_input"), "51000").unwrap();
        let mut runtime = f.runtime();
        let mut config = runtime.status.config.clone();
        for point in &mut config.cpu[..4] {
            point.percent += 1;
        }
        runtime
            .activate(
                config.clone(),
                &f.hardware,
                &f.config_path(),
                &mut TemperatureSampler::default(),
            )
            .unwrap();
        assert_eq!(runtime.status.requested, Some([65, 45]));
        assert_eq!(CurveConfig::load(&f.config_path()).unwrap(), config);
        let now = Instant::now();
        runtime
            .step(&f.hardware, Ok((66.0, Some(51.0))), now)
            .unwrap();
        assert_eq!(runtime.status.requested, Some([65, 45]));
        runtime
            .step(
                &f.hardware,
                Ok((67.0, Some(52.0))),
                now + Duration::from_secs(1),
            )
            .unwrap();
        assert_eq!(runtime.status.requested, Some([65, 45]));
        runtime
            .step(
                &f.hardware,
                Ok((68.0, Some(53.0))),
                now + Duration::from_secs(2),
            )
            .unwrap();
        // A five-point rise is debounced for one sample.
        assert_eq!(runtime.status.requested, Some([65, 45]));
        runtime
            .step(
                &f.hardware,
                Ok((68.0, Some(53.0))),
                now + Duration::from_secs(3),
            )
            .unwrap();
        assert_eq!(runtime.status.requested, Some([70, 50]));
    }

    #[test]
    fn cooldown_makes_few_large_decreases_and_large_rises_are_immediate() {
        let now = Instant::now();
        let mut ramp = Ramp::default();
        assert_eq!(ramp.target(80, 80, now), 80);
        let mut changes = 0;
        let mut previous = 80;
        for second in 0..=45 {
            let target = ramp.target(30, 30, now + Duration::from_secs(second));
            let expected = 80 - (second / 15) as u8 * MAX_DECREASE_POINTS;
            assert_eq!(target, expected.max(30));
            changes += u32::from(target != previous);
            previous = target;
        }
        assert_eq!(previous, 30);
        assert_eq!(changes, 3);
        assert_eq!(ramp.target(95, 95, now + Duration::from_secs(46)), 95);
        assert_eq!(ramp.target(30, 30, now + Duration::from_secs(47)), 95);
        assert_eq!(ramp.target(30, 30, now + Duration::from_secs(61)), 95);
        assert_eq!(ramp.target(30, 30, now + Duration::from_secs(62)), 75);
    }

    #[test]
    fn small_rises_need_two_consecutive_samples_and_maximum_is_immediate() {
        let now = Instant::now();
        let mut ramp = Ramp::default();
        assert_eq!(ramp.target(40, 40, now), 40);
        assert_eq!(ramp.target(45, 50, now + Duration::from_secs(1)), 40);
        assert_eq!(ramp.target(40, 45, now + Duration::from_secs(2)), 40);
        assert_eq!(ramp.target(45, 50, now + Duration::from_secs(3)), 40);
        assert_eq!(ramp.target(50, 50, now + Duration::from_secs(4)), 50);
        assert_eq!(ramp.target(55, 60, now + Duration::from_secs(5)), 50);
        assert_eq!(ramp.target(100, 100, now + Duration::from_secs(6)), 100);
    }

    #[test]
    fn idle_jitter_around_a_curve_step_causes_no_fan_writes() {
        let f = Fixture::new();
        let mut runtime = f.runtime();
        let now = Instant::now();
        runtime
            .step(&f.hardware, Ok((52.0, Some(47.0))), now)
            .unwrap();
        let requested = runtime.status.requested;
        let pwm = [1, 2].map(|channel| {
            fs::metadata(f.hwmon().join(format!("pwm{channel}")))
                .unwrap()
                .modified()
                .unwrap()
        });
        // ±2 °C around the 50/55 °C boundaries, alternating every second.
        for second in 1..CONTROL_AUDIT_INTERVAL.as_secs() {
            let offset = if second % 2 == 0 { 2.0 } else { -2.0 };
            runtime
                .step(
                    &f.hardware,
                    Ok((52.0 + offset, Some(47.0 + offset))),
                    now + Duration::from_secs(second),
                )
                .unwrap();
            assert_eq!(runtime.status.requested, requested);
        }
        for (channel, modified) in [1, 2].into_iter().zip(pwm) {
            assert_eq!(
                fs::metadata(f.hwmon().join(format!("pwm{channel}")))
                    .unwrap()
                    .modified()
                    .unwrap(),
                modified
            );
        }
    }

    #[test]
    fn changed_channel_write_failure_triggers_control_fault_before_audit() {
        let f = Fixture::new();
        let mut runtime = f.runtime();
        let now = Instant::now();
        runtime
            .step(&f.hardware, Ok((60.0, Some(50.0))), now)
            .unwrap();
        fs::remove_file(f.hwmon().join("pwm1")).unwrap();
        fs::create_dir(f.hwmon().join("pwm1")).unwrap();
        assert!(
            runtime
                .step(
                    &f.hardware,
                    Ok((70.0, Some(50.0))),
                    now + Duration::from_secs(1)
                )
                .is_err()
        );
        assert_eq!(runtime.status.state, "control-fault");
        assert!(runtime.verified_manual.is_none());
        assert_eq!(f.hardware.read_fan_modes().unwrap(), [FanMode::Maximum; 2]);
    }

    #[test]
    fn emergency_audit_repairs_external_mode_override() {
        let f = Fixture::new();
        let mut runtime = f.runtime();
        let now = Instant::now();
        assert!(
            runtime
                .step(&f.hardware, Ok((95.0, Some(50.0))), now)
                .is_err()
        );
        fs::write(f.hwmon().join("pwm2_enable"), "2").unwrap();
        assert!(
            runtime
                .step(
                    &f.hardware,
                    Ok((95.0, Some(50.0))),
                    now + Duration::from_secs(4)
                )
                .is_err()
        );
        assert_eq!(f.hardware.read_fan_modes().unwrap()[1], FanMode::Automatic);
        assert!(
            runtime
                .step(
                    &f.hardware,
                    Ok((95.0, Some(50.0))),
                    now + Duration::from_secs(5)
                )
                .is_err()
        );
        assert_eq!(f.hardware.read_fan_modes().unwrap(), [FanMode::Maximum; 2]);
    }

    #[test]
    fn unchanged_ticks_skip_readback_until_audit() {
        let f = Fixture::new();
        let mut runtime = f.runtime();
        let now = Instant::now();
        runtime
            .step(&f.hardware, Ok((60.0, Some(50.0))), now)
            .unwrap();
        fs::write(f.hwmon().join("pwm1"), "invalid").unwrap();
        for second in 1..CONTROL_AUDIT_INTERVAL.as_secs() {
            runtime
                .step(
                    &f.hardware,
                    Ok((60.0, Some(50.0))),
                    now + Duration::from_secs(second),
                )
                .unwrap();
            assert_eq!(runtime.status.state, "running");
        }
        assert!(
            runtime
                .step(
                    &f.hardware,
                    Ok((60.0, Some(50.0))),
                    now + CONTROL_AUDIT_INTERVAL
                )
                .is_err()
        );
        assert_eq!(runtime.status.state, "control-fault");
        assert!(runtime.verified_manual.is_none());
    }

    #[test]
    fn rising_targets_update_only_changed_channel_before_audit() {
        let f = Fixture::new();
        let mut runtime = f.runtime();
        let now = Instant::now();
        runtime
            .step(&f.hardware, Ok((60.0, Some(50.0))), now)
            .unwrap();
        // If the GPU speed were read, verified, or rewritten, this sentinel would fail or change.
        fs::write(f.hwmon().join("pwm2"), "external-override").unwrap();
        runtime
            .step(
                &f.hardware,
                Ok((70.0, Some(50.0))),
                now + Duration::from_secs(1),
            )
            .unwrap();
        assert_eq!(runtime.status.requested, Some([70, 40]));
        assert_eq!(
            fs::read_to_string(f.hwmon().join("pwm2")).unwrap(),
            "external-override"
        );
        assert_eq!(runtime.next_audit, now + CONTROL_AUDIT_INTERVAL);
    }

    #[test]
    fn external_mode_override_is_repaired_at_audit_and_reset_forces_immediate_check() {
        let f = Fixture::new();
        let mut runtime = f.runtime();
        let now = Instant::now();
        runtime
            .step(&f.hardware, Ok((60.0, Some(50.0))), now)
            .unwrap();
        f.hardware.apply_fan_setting(FanSetting::Automatic).unwrap();
        runtime
            .step(
                &f.hardware,
                Ok((60.0, Some(50.0))),
                now + CONTROL_AUDIT_INTERVAL - Duration::from_secs(1),
            )
            .unwrap();
        assert_eq!(
            f.hardware.read_fan_modes().unwrap(),
            [FanMode::Automatic; 2]
        );
        runtime
            .step(
                &f.hardware,
                Ok((60.0, Some(50.0))),
                now + CONTROL_AUDIT_INTERVAL,
            )
            .unwrap();
        assert_eq!(f.hardware.read_fan_modes().unwrap(), [FanMode::Manual; 2]);
        f.hardware.apply_fan_setting(FanSetting::Automatic).unwrap();
        runtime.reset();
        runtime
            .step(
                &f.hardware,
                Ok((60.0, Some(50.0))),
                now + CONTROL_AUDIT_INTERVAL + Duration::from_secs(1),
            )
            .unwrap();
        assert_eq!(f.hardware.read_fan_modes().unwrap(), [FanMode::Manual; 2]);
    }

    #[test]
    fn partial_writes_do_not_postpone_external_speed_audits() {
        let f = Fixture::new();
        let mut runtime = f.runtime();
        let now = Instant::now();
        runtime
            .step(&f.hardware, Ok((60.0, Some(50.0))), now)
            .unwrap();
        fs::write(f.hwmon().join("pwm2"), "0").unwrap();
        for second in 1..5 {
            runtime
                .step(
                    &f.hardware,
                    Ok((60.0 + second as f32, Some(50.0))),
                    now + Duration::from_secs(second),
                )
                .unwrap();
            assert_eq!(fs::read_to_string(f.hwmon().join("pwm2")).unwrap(), "0");
        }
        runtime
            .step(
                &f.hardware,
                Ok((65.0, Some(50.0))),
                now + CONTROL_AUDIT_INTERVAL,
            )
            .unwrap();
        assert_eq!(
            crate::hardware::pwm_to_percent(
                f.hardware.read_fan_control_state().unwrap().gpu.pwm_raw
            ),
            40
        );
    }

    #[test]
    fn emergency_is_immediate_and_confirmed_maximum_is_audited_without_pwm() {
        let f = Fixture::new();
        let mut runtime = f.runtime();
        let now = Instant::now();
        runtime
            .step(&f.hardware, Ok((60.0, Some(50.0))), now)
            .unwrap();
        assert!(
            runtime
                .step(
                    &f.hardware,
                    Ok((95.0, Some(50.0))),
                    now + Duration::from_secs(1)
                )
                .is_err()
        );
        assert_eq!(f.hardware.read_fan_modes().unwrap(), [FanMode::Maximum; 2]);
        // No redundant Maximum commands between audits, even with an unavailable mode node.
        fs::remove_file(f.hwmon().join("pwm2_enable")).unwrap();
        for second in 2..6 {
            assert!(
                runtime
                    .step(
                        &f.hardware,
                        Ok((95.0, Some(50.0))),
                        now + Duration::from_secs(second)
                    )
                    .is_err()
            );
            assert_eq!(runtime.status.state, "emergency");
        }
        assert!(
            runtime
                .step(
                    &f.hardware,
                    Ok((95.0, Some(50.0))),
                    now + Duration::from_secs(6)
                )
                .is_err()
        );
        assert_eq!(runtime.status.state, "control-fault");
        assert!(!runtime.maximum_verified);
        fs::write(f.hwmon().join("pwm2_enable"), "2").unwrap();
        fs::remove_file(f.hwmon().join("pwm1")).unwrap();
        fs::remove_file(f.hwmon().join("pwm2")).unwrap();
        assert!(
            runtime
                .step(
                    &f.hardware,
                    Ok((95.0, Some(50.0))),
                    now + Duration::from_secs(7)
                )
                .is_err()
        );
        assert_eq!(runtime.status.state, "emergency");
        assert!(runtime.maximum_verified);
        assert!(
            runtime
                .step(
                    &f.hardware,
                    Ok((95.0, Some(50.0))),
                    now + Duration::from_secs(12)
                )
                .is_err()
        );
        assert_eq!(runtime.status.state, "emergency");
        assert_eq!(f.hardware.read_fan_modes().unwrap(), [FanMode::Maximum; 2]);
    }

    #[test]
    fn error_24_at_70_degrees_holds_then_falls_back_and_recovers() {
        let f = Fixture::new();
        let mut runtime = f.runtime();
        let now = Instant::now();
        runtime
            .step(&f.hardware, Ok((70.0, Some(55.0))), now)
            .unwrap();
        let missing = || TemperatureReading {
            cpu: Ok(70.0),
            gpu: Err("/dev/nvidiactl: Too many open files (os error 24)".into()),
            sampled_at: None,
        };
        for second in 1..4 {
            assert!(
                runtime
                    .step(&f.hardware, missing(), now + Duration::from_secs(second))
                    .is_err()
            );
            assert_eq!(runtime.status.state, "sensor-hold");
            assert_eq!(runtime.status.requested, Some([70, 50]));
            assert!(!runtime.emergency);
        }
        runtime
            .step(&f.hardware, missing(), now + Duration::from_secs(4))
            .unwrap();
        assert_eq!(runtime.status.state, "sensor-fault");
        assert_eq!(runtime.status.requested, Some([80, 80]));
        assert!(
            runtime
                .status
                .fault
                .as_ref()
                .unwrap()
                .contains("os error 24")
        );
        for second in 5..9 {
            runtime
                .step(
                    &f.hardware,
                    Ok((70.0, Some(55.0))),
                    now + Duration::from_secs(second),
                )
                .unwrap();
            assert_eq!(runtime.status.requested, Some([80, 80]));
        }
        runtime
            .step(
                &f.hardware,
                Ok((70.0, Some(55.0))),
                now + Duration::from_secs(9),
            )
            .unwrap();
        assert_eq!(runtime.status.state, "running");
        assert_eq!(runtime.status.requested, Some([70, 50]));
        assert_eq!(runtime.status.fault, None);
    }

    #[test]
    fn partial_hot_readings_trigger_thermal_emergency() {
        let f = Fixture::new();
        for reading in [
            TemperatureReading {
                cpu: Ok(92.0),
                gpu: Err("GPU read failed".into()),
                sampled_at: None,
            },
            TemperatureReading {
                cpu: Err("CPU read failed".into()),
                gpu: Ok(Some(84.0)),
                sampled_at: None,
            },
        ] {
            let mut runtime = f.runtime();
            assert!(runtime.step(&f.hardware, reading, Instant::now()).is_err());
            assert_eq!(runtime.status.state, "emergency");
            assert_eq!(runtime.status.requested, Some([100, 100]));
        }
    }

    #[test]
    fn sampler_preserves_hot_gpu_when_cpu_read_fails() {
        let f = Fixture::new();
        fs::write(f.hwmon().join("temp1_input"), "invalid").unwrap();
        fs::write(f.hwmon().join("temp2_input"), "84000").unwrap();
        let reading = TemperatureSampler::default().read(&f.hardware);
        assert!(reading.cpu.is_err());
        assert_eq!(reading.gpu, Ok(Some(84.0)));
        let mut runtime = f.runtime();
        assert!(runtime.step(&f.hardware, reading, Instant::now()).is_err());
        assert!(runtime.emergency);
    }

    #[test]
    fn sensor_fallback_preserves_higher_valid_demand_and_rejects_stale_recovery() {
        let f = Fixture::new();
        let mut runtime = f.runtime();
        let now = Instant::now();
        runtime
            .step(
                &f.hardware,
                TemperatureReading {
                    cpu: Ok(85.0),
                    gpu: Err("GPU missing".into()),
                    sampled_at: Some(now),
                },
                now,
            )
            .unwrap();
        assert_eq!(runtime.status.requested, Some([100, 80]));
        let fresh = TemperatureReading {
            cpu: Ok(70.0),
            gpu: Ok(Some(55.0)),
            sampled_at: Some(now + Duration::from_secs(1)),
        };
        for _ in 0..100 {
            runtime
                .step(&f.hardware, fresh.clone(), now + Duration::from_secs(1))
                .unwrap();
        }
        assert_eq!(runtime.safe_samples, 1);
        assert_eq!(runtime.status.state, "sensor-fault");
        runtime
            .step(
                &f.hardware,
                TemperatureReading {
                    cpu: Ok(70.0),
                    gpu: Err("GPU missing again".into()),
                    sampled_at: None,
                },
                now + Duration::from_secs(2),
            )
            .unwrap();
        assert_eq!(runtime.safe_samples, 0);
    }

    #[test]
    fn failed_fallback_does_not_claim_unverified_percentages_or_change_settings() {
        let f = Fixture::new();
        let mut runtime = f.runtime();
        let now = Instant::now();
        runtime
            .step(&f.hardware, Ok((70.0, Some(55.0))), now)
            .unwrap();
        let config = runtime.status.config.clone();
        config.save(&f.config_path()).unwrap();
        // Both percent readback and mode writes fail, so even Maximum cannot be confirmed.
        fs::remove_file(f.hwmon().join("pwm1_enable")).unwrap();
        fs::create_dir(f.hwmon().join("pwm1_enable")).unwrap();
        assert!(
            runtime
                .step(&f.hardware, Err("os error 24".into()), now)
                .is_err()
        );
        assert!(
            runtime
                .step(
                    &f.hardware,
                    Err("os error 24".into()),
                    now + Duration::from_secs(3)
                )
                .is_err()
        );
        assert_eq!(runtime.status.state, "control-fault");
        assert_eq!(runtime.status.requested, Some([70, 50]));
        assert!(!runtime.emergency);
        assert_eq!(CurveConfig::load(&f.config_path()).unwrap(), config);
    }

    thread_local! {
        static OPENED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
        static CLOSED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
        static IDLE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        static LOST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    struct FakeSession {
        _descriptor: fs::File,
    }
    impl TemperatureSession for FakeSession {
        fn sample(&self) -> (Result<f32, String>, bool, bool) {
            (Ok(55.0), LOST.get(), IDLE.get())
        }
    }
    impl Drop for FakeSession {
        fn drop(&mut self) {
            CLOSED.set(CLOSED.get() + 1);
        }
    }
    fn fake_open(
        _: &crate::nvidia::NvidiaPciDevice,
        root: &Path,
    ) -> Result<Box<dyn TemperatureSession>, String> {
        OPENED.set(OPENED.get() + 1);
        Ok(Box::new(FakeSession {
            _descriptor: fs::File::open(root.join("sys/class/dmi/id/sys_vendor")).unwrap(),
        }))
    }
    fn nvidia_fixture(f: &Fixture) -> std::path::PathBuf {
        OPENED.set(0);
        CLOSED.set(0);
        IDLE.set(false);
        LOST.set(false);
        fs::remove_file(f.hwmon().join("temp2_input")).unwrap();
        let pci = f.root.join("sys/bus/pci/devices/0000:01:00.0");
        fs::create_dir_all(pci.join("power")).unwrap();
        for (name, value) in [
            ("vendor", "0x10de"),
            ("class", "0x030000"),
            ("device", "0x1234"),
            ("subsystem_vendor", "0x1025"),
            ("subsystem_device", "0x1234"),
            ("power/runtime_status", "active"),
        ] {
            fs::write(pci.join(name), value).unwrap();
        }
        pci
    }
    #[test]
    fn thousands_of_samples_and_curve_edits_retain_one_session() {
        let f = Fixture::new();
        nvidia_fixture(&f);
        let mut sampler = TemperatureSampler {
            open: fake_open,
            ..Default::default()
        };
        let now = Instant::now();
        let mut runtime = f.runtime();
        for second in 0..4000 {
            let reading = sampler.read_at(&f.hardware, now + Duration::from_secs(second));
            assert_eq!(reading.complete().unwrap(), (65.0, Some(55.0)));
            if second % 100 == 0 {
                // Replacing curve settings/runtime must leave the daemon's sampler alone.
                runtime = CurveRuntime::new(runtime.status.config.clone());
            }
        }
        for _ in 0..3 {
            runtime
                .activate(
                    runtime.status.config.clone(),
                    &f.hardware,
                    &f.config_path(),
                    &mut sampler,
                )
                .unwrap();
        }
        assert_eq!(OPENED.get(), 1);
        assert_eq!(CLOSED.get(), 0);
        drop(sampler);
        assert_eq!(CLOSED.get(), 1);
    }
    #[test]
    fn idle_sampling_retains_session_and_sleep_resume_reopens_it() {
        let f = Fixture::new();
        let pci = nvidia_fixture(&f);
        IDLE.set(true);
        let mut sampler = TemperatureSampler {
            open: fake_open,
            ..Default::default()
        };
        let now = Instant::now();
        for second in 0..1000 {
            sampler.read_at(&f.hardware, now + Duration::from_secs(second));
        }
        assert_eq!(OPENED.get(), 1);
        assert_eq!(CLOSED.get(), 0);
        fs::write(pci.join("power/runtime_status"), "suspended").unwrap();
        assert_eq!(
            sampler
                .read_at(&f.hardware, now + Duration::from_secs(1000))
                .gpu,
            Ok(None)
        );
        assert_eq!(CLOSED.get(), 1);
        fs::write(pci.join("power/runtime_status"), "active").unwrap();
        sampler.read_at(&f.hardware, now + Duration::from_secs(1001));
        assert_eq!(OPENED.get(), 2);
        fs::write(pci.join("device"), "0x1235").unwrap();
        sampler.read_at(&f.hardware, now + Duration::from_secs(1002));
        assert_eq!(OPENED.get(), 3); // Changed PCI identity releases the old session.
        sampler.invalidate(); // Explicit suspend/resume notification.
        sampler.read_at(&f.hardware, now + Duration::from_secs(1003));
        assert_eq!(OPENED.get(), 4);
    }
    #[test]
    fn initialization_failures_back_off_and_session_loss_retries() {
        fn fail_open(
            _: &crate::nvidia::NvidiaPciDevice,
            _: &Path,
        ) -> Result<Box<dyn TemperatureSession>, String> {
            OPENED.set(OPENED.get() + 1);
            Err("/dev/nvidiactl: Too many open files (os error 24)".into())
        }
        let f = Fixture::new();
        nvidia_fixture(&f);
        let mut sampler = TemperatureSampler {
            open: fail_open,
            ..Default::default()
        };
        let now = Instant::now();
        for second in 0..1000 {
            let sample = sampler.read_at(&f.hardware, now + Duration::from_secs(second));
            assert!(sample.gpu.unwrap_err().contains("os error 24"));
        }
        assert!(OPENED.get() < 40);
        assert_eq!(sampler.retry_delay, 30);
        sampler.invalidate();
        sampler.open = fake_open;
        LOST.set(true);
        assert!(
            sampler
                .read_at(&f.hardware, now + Duration::from_secs(1000))
                .gpu
                .is_err()
        );
        assert!(sampler.nvidia.is_none());
        LOST.set(false);
        assert_eq!(
            sampler
                .read_at(&f.hardware, now + Duration::from_secs(1001))
                .gpu,
            Ok(Some(55.0))
        );
    }

    #[test]
    fn emergency_settings_are_private_bounded_and_independent_of_curves() {
        let f = Fixture::new();
        let path = f.root.join("state/fan-emergency.json");
        assert_eq!(
            EmergencyConfig::load(&path).unwrap(),
            EmergencyConfig::default()
        );
        let limits = EmergencyConfig {
            schema: 1,
            cpu_limit: 100,
            gpu_limit: 90,
        };
        limits.save(&path).unwrap();
        assert_eq!(EmergencyConfig::load(&path).unwrap(), limits);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let curve = f.runtime().status.config;
        curve.save(&f.config_path()).unwrap();
        for (cpu_limit, gpu_limit) in [(59, 84), (101, 84), (92, 59), (92, 91)] {
            assert!(
                EmergencyConfig {
                    schema: 1,
                    cpu_limit,
                    gpu_limit
                }
                .save(&path)
                .is_err()
            );
        }
        assert_eq!(EmergencyConfig::load(&path).unwrap(), limits);
        assert_eq!(CurveConfig::load(&f.config_path()).unwrap(), curve);
        fs::write(&path, "{bad JSON}").unwrap();
        assert!(EmergencyConfig::load(&path).is_err());
    }

    #[test]
    fn configured_limits_and_hysteresis_require_fresh_samples() {
        let f = Fixture::new();
        let mut runtime = f.runtime();
        runtime.set_limits(EmergencyConfig {
            schema: 1,
            cpu_limit: 100,
            gpu_limit: 90,
        });
        let now = Instant::now();
        runtime
            .step(&f.hardware, Ok((99.0, Some(89.0))), now)
            .unwrap();
        assert!(!runtime.emergency);
        assert!(
            runtime
                .step(&f.hardware, Ok((100.0, Some(55.0))), now)
                .is_err()
        );
        assert_eq!(runtime.emergency_status().config.cpu_limit, 100);
        assert!(runtime.emergency_status().reason.unwrap().contains("100°C"));
        runtime
            .step(&f.hardware, Ok((95.0, Some(55.0))), now)
            .unwrap();
        assert_eq!(runtime.safe_samples, 0); // Must be strictly below limit minus five.
        let reading = TemperatureReading {
            cpu: Ok(94.0),
            gpu: Ok(Some(84.0)),
            sampled_at: Some(now),
        };
        for _ in 0..20 {
            runtime.step(&f.hardware, reading.clone(), now).unwrap();
        }
        assert_eq!(runtime.safe_samples, 1);
        runtime.set_limits(EmergencyConfig {
            schema: 1,
            cpu_limit: 100,
            gpu_limit: 90,
        });
        assert!(runtime.emergency);
        assert_eq!(runtime.safe_samples, 0);
        for second in 1..=5 {
            runtime
                .step(
                    &f.hardware,
                    TemperatureReading {
                        cpu: Ok(94.0),
                        gpu: Ok(Some(84.0)),
                        sampled_at: Some(now + Duration::from_secs(second)),
                    },
                    now + Duration::from_secs(second),
                )
                .unwrap();
        }
        assert!(!runtime.emergency);
        assert_eq!(runtime.status.state, "running");
    }

    #[test]
    fn changing_limits_and_curve_ownership_preserves_saved_emergency_limits() {
        let f = Fixture::new();
        let mut runtime = f.runtime();
        let limits = EmergencyConfig {
            schema: 1,
            cpu_limit: 80,
            gpu_limit: 75,
        };
        runtime.set_limits(limits.clone());
        runtime
            .activate(
                runtime.status.config.clone(),
                &f.hardware,
                &f.config_path(),
                &mut TemperatureSampler::default(),
            )
            .unwrap();
        assert_eq!(runtime.limits, limits);
        assert!(
            runtime
                .step(&f.hardware, Ok((80.0, Some(55.0))), Instant::now())
                .is_err()
        );
        runtime.disable(&f.config_path()).unwrap();
        assert_eq!(runtime.limits, limits);
        assert_eq!(runtime.emergency_status().state, "firmware-managed");
    }

    #[test]
    fn sustained_heating_updates_manual_without_mode_transitions() {
        let f = Fixture::new();
        let mut runtime = f.runtime();
        let now = Instant::now();
        runtime
            .step(&f.hardware, Ok((60.0, Some(50.0))), now)
            .unwrap();
        assert_eq!(runtime.status.requested, Some([50, 40]));
        let cpu_mode = fs::metadata(f.hwmon().join("pwm1_enable"))
            .unwrap()
            .modified()
            .unwrap();
        let gpu_mode = fs::metadata(f.hwmon().join("pwm2_enable"))
            .unwrap()
            .modified()
            .unwrap();
        runtime
            .step(
                &f.hardware,
                Ok((70.0, Some(65.0))),
                now + Duration::from_secs(1),
            )
            .unwrap();
        assert_eq!(runtime.status.requested, Some([70, 70]));
        assert_eq!(
            fs::metadata(f.hwmon().join("pwm1_enable"))
                .unwrap()
                .modified()
                .unwrap(),
            cpu_mode
        );
        assert_eq!(
            fs::metadata(f.hwmon().join("pwm2_enable"))
                .unwrap()
                .modified()
                .unwrap(),
            gpu_mode
        );
        // Firmware may round the raw PWM down while preserving 70%.
        fs::write(f.hwmon().join("pwm1"), "178").unwrap();
        let pwm = fs::metadata(f.hwmon().join("pwm1"))
            .unwrap()
            .modified()
            .unwrap();
        runtime
            .step(
                &f.hardware,
                Ok((70.0, Some(65.0))),
                now + Duration::from_secs(2),
            )
            .unwrap();
        assert_eq!(
            fs::metadata(f.hwmon().join("pwm1"))
                .unwrap()
                .modified()
                .unwrap(),
            pwm
        );
    }

    #[test]
    fn thermal_limits_require_five_safe_samples() {
        for reading in [Ok((92.0, Some(55.0))), Ok((65.0, Some(84.0)))] {
            let f = Fixture::new();
            let mut runtime = f.runtime();
            let now = Instant::now();
            assert!(runtime.step(&f.hardware, reading, now).is_err());
            assert_eq!(
                f.hardware.read_fan_state().unwrap().cpu.mode,
                Some(FanMode::Maximum)
            );
            for second in 1..5 {
                runtime
                    .step(
                        &f.hardware,
                        Ok((65.0, Some(55.0))),
                        now + Duration::from_secs(second),
                    )
                    .unwrap();
                assert!(runtime.emergency);
            }
            runtime
                .step(
                    &f.hardware,
                    Ok((65.0, Some(55.0))),
                    now + Duration::from_secs(5),
                )
                .unwrap();
            assert!(!runtime.emergency);
            assert_eq!(runtime.status.state, "running");
            assert_eq!(
                f.hardware.read_fan_state().unwrap().cpu.mode,
                Some(FanMode::Manual)
            );
        }
    }

    #[test]
    fn failed_readback_reports_control_fault() {
        let f = Fixture::new();
        let mut runtime = f.runtime();
        let now = Instant::now();
        runtime
            .step(&f.hardware, Ok((60.0, Some(50.0))), now)
            .unwrap();
        fs::write(f.hwmon().join("pwm1"), "invalid").unwrap();
        assert!(
            runtime
                .step(
                    &f.hardware,
                    Ok((60.0, Some(50.0))),
                    now + CONTROL_AUDIT_INTERVAL
                )
                .is_err()
        );
        assert!(!runtime.emergency);
        assert_eq!(runtime.status.state, "control-fault");
        assert_eq!(
            fs::read_to_string(f.hwmon().join("pwm1_enable")).unwrap(),
            "0"
        );
    }

    #[test]
    fn disabled_curve_and_early_ticks_do_not_touch_hardware() {
        let f = Fixture::new();
        let mut runtime = CurveRuntime::new(CurveConfig::default());
        runtime.tick(&f.hardware, &mut TemperatureSampler::default());
        assert_eq!(
            f.hardware.read_fan_state().unwrap().cpu.mode,
            Some(FanMode::Automatic)
        );
        runtime.status.config.enabled = true;
        runtime.next_tick = Instant::now() + Duration::from_secs(60);
        runtime.tick(&f.hardware, &mut TemperatureSampler::default());
        assert_eq!(
            f.hardware.read_fan_state().unwrap().cpu.mode,
            Some(FanMode::Automatic)
        );
    }

    #[test]
    fn saved_curve_restarts_without_a_client_and_disable_keeps_points() {
        let f = Fixture::new();
        let path = f.config_path();
        let mut runtime = f.runtime();
        runtime
            .activate(
                runtime.status.config.clone(),
                &f.hardware,
                &path,
                &mut TemperatureSampler::default(),
            )
            .unwrap();
        let saved = CurveConfig::load(&path).unwrap();
        assert!(saved.enabled);
        let mut restarted = CurveRuntime::new(saved.clone());
        f.hardware.apply_fan_setting(FanSetting::Automatic).unwrap();
        restarted
            .step(&f.hardware, Ok((70.0, Some(60.0))), Instant::now())
            .unwrap();
        assert_eq!(restarted.status.requested, Some([70, 60]));
        restarted.disable(&path).unwrap();
        let disabled = CurveConfig::load(&path).unwrap();
        assert!(!disabled.enabled);
        assert_eq!(disabled.cpu, saved.cpu);
        assert_eq!(disabled.gpu, saved.gpu);
    }

    #[test]
    fn failed_save_restores_auto_and_preserves_runtime_configuration() {
        let f = Fixture::new();
        let path = f.config_path();
        fs::create_dir_all(&path).unwrap(); // An existing directory cannot be replaced with a settings file.
        let mut runtime = CurveRuntime::new(CurveConfig::default());
        let config = CurveConfig {
            enabled: true,
            ..CurveConfig::default()
        };
        assert!(
            runtime
                .activate(
                    config,
                    &f.hardware,
                    &path,
                    &mut TemperatureSampler::default()
                )
                .is_err()
        );
        assert!(!runtime.status.config.enabled);
        assert_eq!(
            f.hardware.read_fan_state().unwrap().cpu.mode,
            Some(FanMode::Automatic)
        );
    }

    #[test]
    fn profile_or_resume_mode_reset_reenters_manual_from_fresh_readings() {
        let f = Fixture::new();
        let mut runtime = f.runtime();
        runtime
            .step(&f.hardware, Ok((70.0, Some(65.0))), Instant::now())
            .unwrap();
        f.hardware.apply_fan_setting(FanSetting::Automatic).unwrap();
        runtime.reset();
        runtime
            .step(&f.hardware, Ok((50.0, Some(45.0))), Instant::now())
            .unwrap();
        assert_eq!(runtime.status.requested, Some([35, 35]));
        assert_eq!(
            f.hardware.read_fan_state().unwrap().gpu.mode,
            Some(FanMode::Manual)
        );
    }

    #[test]
    fn package_temperature_precedes_acer_and_rejects_invalid_readings() {
        let f = Fixture::new();
        let core = f.root.join("sys/class/hwmon/hwmon17");
        fs::create_dir_all(&core).unwrap();
        fs::write(core.join("name"), "coretemp").unwrap();
        fs::write(core.join("temp1_label"), "Package id 0").unwrap();
        fs::write(core.join("temp1_input"), "71000").unwrap();
        assert_eq!(temperatures(&f.hardware).unwrap(), (71.0, Some(55.0)));
        fs::write(core.join("temp1_input"), "999999").unwrap();
        assert_eq!(temperatures(&f.hardware).unwrap().0, 65.0);
        fs::remove_file(f.hwmon().join("temp2_input")).unwrap();
        assert!(temperatures(&f.hardware).is_err());
    }

    #[test]
    fn positively_suspended_gpu_uses_minimum_without_nvml() {
        let f = Fixture::new();
        fs::remove_file(f.hwmon().join("temp2_input")).unwrap();
        let pci = f.root.join("sys/bus/pci/devices/0000:01:00.0");
        fs::create_dir_all(pci.join("power")).unwrap();
        for (name, value) in [
            ("vendor", "0x10de"),
            ("class", "0x030000"),
            ("device", "0x1234"),
            ("subsystem_vendor", "0x1025"),
            ("subsystem_device", "0x1234"),
            ("power/runtime_status", "suspended"),
        ] {
            fs::write(pci.join(name), value).unwrap();
        }
        assert_eq!(temperatures(&f.hardware).unwrap(), (65.0, None));
        let mut runtime = f.runtime();
        runtime
            .step(&f.hardware, temperatures(&f.hardware), Instant::now())
            .unwrap();
        assert_eq!(runtime.status.requested, Some([60, 30]));
        fs::write(pci.join("power/runtime_status"), "resuming").unwrap();
        assert!(temperatures(&f.hardware).is_err());
    }

    #[test]
    fn maximum_point_lists_fit_existing_command_limit() {
        let cpu = "20:20,30:30,40:40,50:50,60:60,70:70,80:80,85:100";
        let gpu = "20:20,30:30,40:40,50:50,60:60,70:70,75:80,78:100";
        let config = CurveConfig::from_tokens(cpu, gpu).unwrap();
        let command = format!(
            "FAN CURVE SET {} {}",
            CurveConfig::token(&config.cpu),
            CurveConfig::token(&config.gpu)
        );
        assert!(command.len() <= crate::control::MAX_CONTROL_COMMAND_BYTES);
    }

    #[test]
    fn defaults_interpolate_and_clamp() {
        let c = CurveConfig::default();
        c.validate().unwrap();
        assert_eq!(interpolate(&c.cpu, 20.0), 30);
        assert_eq!(interpolate(&c.cpu, 60.0), 50);
        assert_eq!(interpolate(&c.cpu, 65.1), 61);
        assert_eq!(interpolate(&c.cpu, 100.0), 100);
    }
    #[test]
    fn rejects_unsafe_or_unordered_curves() {
        for cpu in [
            "45:30",
            "45:30,45:100",
            "45:60,65:50,85:100",
            "45:10,85:100",
            "45:30,90:100",
            "45:30,85:90",
        ] {
            assert!(
                CurveConfig::from_tokens(cpu, "40:30,78:100").is_err(),
                "{cpu}"
            );
        }
        assert!(CurveConfig::from_tokens("45:30,85:100", "40:30,84:100").is_err());
    }
    #[test]
    fn cooling_waits_then_limits_each_decrease() {
        let now = Instant::now();
        let mut ramp = Ramp::default();
        assert_eq!(ramp.target(80, 80, now), 80);
        assert_eq!(ramp.target(30, 30, now), 80);
        assert_eq!(ramp.target(30, 30, now + Duration::from_secs(14)), 80);
        assert_eq!(ramp.target(30, 30, now + Duration::from_secs(15)), 60);
        assert_eq!(ramp.target(90, 90, now + Duration::from_secs(16)), 90);
        assert_eq!(ramp.target(30, 30, now + Duration::from_secs(17)), 90);
        // Hysteresis: a lower measured target alone does not start cooldown.
        assert_eq!(ramp.target(85, 90, now + Duration::from_secs(40)), 90);
        assert_eq!(ramp.target(85, 90, now + Duration::from_secs(60)), 90);
    }
    #[test]
    fn persistence_round_trip_and_invalid_file_preservation() {
        let directory = std::env::temp_dir().join(format!("asense-curve-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("curve.json");
        let c = CurveConfig::from_tokens("45:30,85:100", "40:30,78:100").unwrap();
        c.save(&path).unwrap();
        assert_eq!(CurveConfig::load(&path).unwrap(), c);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let mut invalid = c.clone();
        invalid.cpu.clear();
        assert!(invalid.save(&path).is_err());
        assert_eq!(CurveConfig::load(&path).unwrap(), c);
        fs::remove_dir_all(directory).unwrap();
    }
}
