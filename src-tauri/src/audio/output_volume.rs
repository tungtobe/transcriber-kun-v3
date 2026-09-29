//! System output volume ducking (Story 5.4).
//!
//! While the model speaks, the default output device is lowered to
//! [`DUCK_FACTOR`] of its original level and restored afterwards. The
//! backend is a private trait of `audio/` (no fourth architecture port):
//! Core Audio on macOS, an unverified stub on Windows, a fake in tests.
//!
//! Crash safety: before the first volume change a durable marker
//! (`state/ducking.json`, written temp-then-rename) records the device, the
//! original level and the level the app applied. Boot (and app exit) calls
//! [`restore_from_marker`], which only touches the recorded device and only
//! while its volume is still the level the app applied.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Ducked level as a fraction of the original volume.
pub const DUCK_FACTOR: f32 = 0.3;
/// Reads back within this distance of the applied level count as "unchanged"
/// (hardware quantizes volume steps).
const LEVEL_TOLERANCE: f32 = 0.02;

/// Volume backend. Levels are scalars in `0.0..=1.0`; device ids are stable
/// across reboots (Core Audio UID).
pub trait OutputVolume: Send {
    fn default_device(&mut self) -> Result<String, String>;
    /// `Err` when the device is absent or has no volume control.
    fn get(&mut self, device: &str) -> Result<f32, String>;
    fn set(&mut self, device: &str, level: f32) -> Result<(), String>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Marker {
    device_id: String,
    original: f32,
    applied: f32,
    /// `false` once the user changed the volume: nothing may be restored.
    active: bool,
}

fn same_level(a: f32, b: f32) -> bool {
    (a - b).abs() <= LEVEL_TOLERANCE
}

fn write_marker(path: &Path, marker: &Marker) -> Result<(), String> {
    let dir = path.parent().ok_or("marker path has no parent")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_vec(marker).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn remove_marker(path: &Path) {
    if let Err(error) = std::fs::remove_file(path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(error = %error, "could not remove ducking marker");
        }
    }
}

/// Restores the volume recorded in the marker (boot and exit), then removes
/// it. A corrupt, inactive or stale marker is dropped without touching the
/// device. Never panics; backend errors are logged.
pub fn restore_from_marker(volume: &mut dyn OutputVolume, path: &Path) {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(error = %error, "could not read ducking marker");
                remove_marker(path);
            }
            return;
        }
    };
    if let Ok(marker) = serde_json::from_slice::<Marker>(&bytes) {
        if marker.active {
            // Absent device or user-changed volume: leave it alone.
            if let Ok(current) = volume.get(&marker.device_id) {
                if same_level(current, marker.applied) {
                    if let Err(error) = volume.set(&marker.device_id, marker.original) {
                        tracing::warn!(error = %error, "could not restore output volume from marker");
                    }
                }
            }
        }
    } else {
        tracing::warn!("ducking marker is corrupt; dropping it");
    }
    remove_marker(path);
}

#[derive(Debug, Clone, PartialEq)]
enum State {
    Idle,
    Ducked {
        device: String,
        original: f32,
        applied: f32,
    },
    /// The user took over the volume; stay hands-off until the model is silent.
    Overridden,
}

/// The ducking state machine. Backend failures are logged and never block TTS.
pub struct Ducker {
    volume: Box<dyn OutputVolume>,
    marker: PathBuf,
    state: State,
}

impl Ducker {
    pub fn new(volume: Box<dyn OutputVolume>, marker: PathBuf) -> Self {
        Self {
            volume,
            marker,
            state: State::Idle,
        }
    }

    pub fn is_ducked(&self) -> bool {
        matches!(self.state, State::Ducked { .. })
    }

    /// The model started (or keeps) speaking.
    pub fn duck(&mut self) {
        match self.state.clone() {
            State::Overridden => {}
            State::Ducked { device, .. } => {
                // Overlapping turn: keep the original level. Follow a device
                // change or a manual adjustment, though.
                if !self.check_user_override() && self.current_device().as_deref() != Some(&device)
                {
                    self.restore();
                    self.start_duck();
                }
            }
            State::Idle => self.start_duck(),
        }
    }

    /// The model is silent, TTS was turned off, the session stopped or the
    /// app is closing.
    pub fn restore(&mut self) {
        match std::mem::replace(&mut self.state, State::Idle) {
            State::Ducked {
                device,
                original,
                applied,
            } => {
                match self.volume.get(&device) {
                    Ok(current) if same_level(current, applied) => {
                        if let Err(error) = self.volume.set(&device, original) {
                            tracing::warn!(error = %error, "could not restore output volume");
                        }
                    }
                    // User changed it, or the device vanished: do not force.
                    _ => {}
                }
                remove_marker(&self.marker);
            }
            State::Overridden | State::Idle => {}
        }
    }

    /// Periodic check while speaking: detects a manual change or an output
    /// device change (old device restored, new baseline taken).
    pub fn sync(&mut self, speaking: bool) {
        if self.is_ducked() && speaking {
            self.duck();
        }
    }

    fn current_device(&mut self) -> Option<String> {
        self.volume.default_device().ok()
    }

    /// Returns `true` when the user changed the volume while ducked.
    fn check_user_override(&mut self) -> bool {
        let State::Ducked {
            device, applied, ..
        } = self.state.clone()
        else {
            return false;
        };
        match self.volume.get(&device) {
            Ok(current) if !same_level(current, applied) => {
                // Invalidate the marker at once so a crash cannot undo the
                // user's choice.
                let dead = Marker {
                    device_id: device,
                    original: 0.0,
                    applied: 0.0,
                    active: false,
                };
                if write_marker(&self.marker, &dead).is_err() {
                    remove_marker(&self.marker);
                }
                self.state = State::Overridden;
                true
            }
            _ => false,
        }
    }

    fn start_duck(&mut self) {
        let device = match self.volume.default_device() {
            Ok(device) => device,
            Err(error) => {
                tracing::warn!(error = %error, "no output device to duck");
                return;
            }
        };
        let original = match self.volume.get(&device) {
            Ok(level) => level,
            Err(error) => {
                tracing::warn!(error = %error, "could not read output volume");
                return;
            }
        };
        let applied = original * DUCK_FACTOR;
        let marker = Marker {
            device_id: device.clone(),
            original,
            applied,
            active: true,
        };
        // Marker first: a crash after the change must still be recoverable.
        if let Err(error) = write_marker(&self.marker, &marker) {
            tracing::warn!(error = %error, "could not write ducking marker; not ducking");
            return;
        }
        if let Err(error) = self.volume.set(&device, applied) {
            tracing::warn!(error = %error, "could not lower output volume");
            remove_marker(&self.marker);
            return;
        }
        self.state = State::Ducked {
            device,
            original,
            applied,
        };
    }
}

/// Backend for the real machine.
pub fn platform_volume() -> Box<dyn OutputVolume> {
    #[cfg(target_os = "macos")]
    {
        Box::new(mac::CoreAudioVolume)
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(win::UnverifiedWindowsVolume)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Box::new(NoVolume)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
struct NoVolume;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl OutputVolume for NoVolume {
    fn default_device(&mut self) -> Result<String, String> {
        Err("output volume is unsupported on this platform".to_owned())
    }
    fn get(&mut self, _device: &str) -> Result<f32, String> {
        Err("unsupported".to_owned())
    }
    fn set(&mut self, _device: &str, _level: f32) -> Result<(), String> {
        Err("unsupported".to_owned())
    }
}

/// Windows backend. UNVERIFIED and intentionally inert: the pinned `wasapi`
/// crate exposes no endpoint-volume interface and adding the `windows` crate
/// is out of scope, so every call reports an error and the Ducker simply
/// never ducks (TTS is unaffected). See ADR 0006.
#[cfg(target_os = "windows")]
mod win {
    use super::OutputVolume;

    pub struct UnverifiedWindowsVolume;

    const MSG: &str = "Windows output volume ducking is not implemented (unverified)";

    impl OutputVolume for UnverifiedWindowsVolume {
        fn default_device(&mut self) -> Result<String, String> {
            Err(MSG.to_owned())
        }
        fn get(&mut self, _device: &str) -> Result<f32, String> {
            Err(MSG.to_owned())
        }
        fn set(&mut self, _device: &str, _level: f32) -> Result<(), String> {
            Err(MSG.to_owned())
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::c_void;
    use std::ptr::NonNull;

    use objc2_core_audio::{
        kAudioDevicePropertyDeviceUID, kAudioHardwarePropertyDefaultOutputDevice,
        kAudioHardwarePropertyTranslateUIDToDevice, kAudioObjectPropertyElementMain,
        kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyScopeOutput,
        kAudioObjectSystemObject, kAudioObjectUnknown, AudioObjectGetPropertyData,
        AudioObjectHasProperty, AudioObjectID, AudioObjectPropertyAddress,
        AudioObjectSetPropertyData,
    };
    use objc2_core_foundation::{CFRetained, CFString};

    use super::OutputVolume;

    /// `kAudioHardwareServiceDeviceProperty_VirtualMainVolume` ('vmvc'): the
    /// device-level scalar the system volume slider drives.
    const VIRTUAL_MAIN_VOLUME: u32 = 0x766D_7663;
    const SYSTEM: AudioObjectID = kAudioObjectSystemObject as AudioObjectID;

    pub struct CoreAudioVolume;

    fn address(selector: u32, scope: u32) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress {
            mSelector: selector,
            mScope: scope,
            mElement: kAudioObjectPropertyElementMain,
        }
    }

    fn check(status: i32, what: &str) -> Result<(), String> {
        if status == 0 {
            Ok(())
        } else {
            Err(format!("Core Audio could not {what} (status {status})"))
        }
    }

    fn device_uid(device: AudioObjectID) -> Result<String, String> {
        let mut addr = address(kAudioDevicePropertyDeviceUID, kAudioObjectPropertyScopeGlobal);
        let mut raw: *const CFString = std::ptr::null();
        let mut size = std::mem::size_of::<*const CFString>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                device,
                NonNull::from(&mut addr),
                0,
                std::ptr::null(),
                NonNull::from(&mut size),
                NonNull::from(&mut raw).cast(),
            )
        };
        check(status, "read the device UID")?;
        let raw = NonNull::new(raw as *mut CFString).ok_or("device has no UID")?;
        // The property follows the Create rule: we own one retain.
        let uid = unsafe { CFRetained::from_raw(raw) };
        Ok(uid.to_string())
    }

    fn device_for_uid(uid: &str) -> Result<AudioObjectID, String> {
        let cf = CFString::from_str(uid);
        let qualifier: *const CFString = &*cf;
        let mut addr = address(
            kAudioHardwarePropertyTranslateUIDToDevice,
            kAudioObjectPropertyScopeGlobal,
        );
        let mut device = kAudioObjectUnknown;
        let mut size = std::mem::size_of::<AudioObjectID>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                SYSTEM,
                NonNull::from(&mut addr),
                std::mem::size_of::<*const CFString>() as u32,
                (&qualifier as *const *const CFString).cast::<c_void>(),
                NonNull::from(&mut size),
                NonNull::from(&mut device).cast(),
            )
        };
        check(status, "translate the device UID")?;
        if device == kAudioObjectUnknown {
            return Err("output device is absent".to_owned());
        }
        Ok(device)
    }

    impl OutputVolume for CoreAudioVolume {
        fn default_device(&mut self) -> Result<String, String> {
            let mut addr = address(
                kAudioHardwarePropertyDefaultOutputDevice,
                kAudioObjectPropertyScopeGlobal,
            );
            let mut device = kAudioObjectUnknown;
            let mut size = std::mem::size_of::<AudioObjectID>() as u32;
            let status = unsafe {
                AudioObjectGetPropertyData(
                    SYSTEM,
                    NonNull::from(&mut addr),
                    0,
                    std::ptr::null(),
                    NonNull::from(&mut size),
                    NonNull::from(&mut device).cast(),
                )
            };
            check(status, "read the default output device")?;
            if device == kAudioObjectUnknown {
                return Err("no default output device".to_owned());
            }
            device_uid(device)
        }

        fn get(&mut self, uid: &str) -> Result<f32, String> {
            let device = device_for_uid(uid)?;
            let mut addr = address(VIRTUAL_MAIN_VOLUME, kAudioObjectPropertyScopeOutput);
            if !unsafe { AudioObjectHasProperty(device, NonNull::from(&mut addr)) } {
                return Err("device has no volume control".to_owned());
            }
            let mut level = 0f32;
            let mut size = std::mem::size_of::<f32>() as u32;
            let status = unsafe {
                AudioObjectGetPropertyData(
                    device,
                    NonNull::from(&mut addr),
                    0,
                    std::ptr::null(),
                    NonNull::from(&mut size),
                    NonNull::from(&mut level).cast(),
                )
            };
            check(status, "read the output volume")?;
            Ok(level)
        }

        fn set(&mut self, uid: &str, level: f32) -> Result<(), String> {
            let device = device_for_uid(uid)?;
            let mut addr = address(VIRTUAL_MAIN_VOLUME, kAudioObjectPropertyScopeOutput);
            let level = level.clamp(0.0, 1.0);
            let status = unsafe {
                AudioObjectSetPropertyData(
                    device,
                    NonNull::from(&mut addr),
                    0,
                    std::ptr::null(),
                    std::mem::size_of::<f32>() as u32,
                    NonNull::from(&level).cast(),
                )
            };
            check(status, "set the output volume")
        }
    }
}

#[cfg(test)]
pub mod fake {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use super::OutputVolume;

    #[derive(Default)]
    struct Inner {
        default: Option<String>,
        levels: HashMap<String, f32>,
    }

    /// Shared fake: clones observe the same devices.
    #[derive(Clone, Default)]
    pub struct FakeVolume(Arc<Mutex<Inner>>);

    impl FakeVolume {
        pub fn with_device(id: &str, level: f32) -> Self {
            let fake = Self::default();
            fake.add(id, level);
            fake.set_default(Some(id));
            fake
        }
        pub fn add(&self, id: &str, level: f32) {
            self.0.lock().unwrap().levels.insert(id.to_owned(), level);
        }
        pub fn remove(&self, id: &str) {
            self.0.lock().unwrap().levels.remove(id);
        }
        pub fn set_default(&self, id: Option<&str>) {
            self.0.lock().unwrap().default = id.map(str::to_owned);
        }
        pub fn level(&self, id: &str) -> Option<f32> {
            self.0.lock().unwrap().levels.get(id).copied()
        }
        pub fn user_sets(&self, id: &str, level: f32) {
            self.add(id, level);
        }
    }

    impl OutputVolume for FakeVolume {
        fn default_device(&mut self) -> Result<String, String> {
            self.0.lock().unwrap().default.clone().ok_or("none".into())
        }
        fn get(&mut self, device: &str) -> Result<f32, String> {
            self.level(device).ok_or_else(|| "absent".to_owned())
        }
        fn set(&mut self, device: &str, level: f32) -> Result<(), String> {
            let mut inner = self.0.lock().unwrap();
            match inner.levels.get_mut(device) {
                Some(slot) => {
                    *slot = level;
                    Ok(())
                }
                None => Err("absent".to_owned()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeVolume;
    use super::*;

    fn setup() -> (tempfile::TempDir, PathBuf, FakeVolume, Ducker) {
        let dir = tempfile::tempdir().unwrap();
        let marker = crate::core::paths::ducking_marker_path(dir.path());
        let fake = FakeVolume::with_device("spk", 0.8);
        let ducker = Ducker::new(Box::new(fake.clone()), marker.clone());
        (dir, marker, fake, ducker)
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn ducks_to_thirty_percent_and_restores_with_marker_lifecycle() {
        let (_dir, marker, fake, mut ducker) = setup();
        ducker.duck();
        assert!(close(fake.level("spk").unwrap(), 0.24));
        let saved: Marker = serde_json::from_slice(&std::fs::read(&marker).unwrap()).unwrap();
        assert_eq!(saved.device_id, "spk");
        assert!(close(saved.original, 0.8) && saved.active);
        ducker.restore();
        assert!(close(fake.level("spk").unwrap(), 0.8));
        assert!(!marker.exists());
    }

    #[test]
    fn overlapping_turn_keeps_the_original_level() {
        let (_dir, _marker, fake, mut ducker) = setup();
        ducker.duck();
        ducker.duck();
        ducker.restore();
        assert!(close(fake.level("spk").unwrap(), 0.8));
    }

    #[test]
    fn manual_change_while_ducked_is_kept_and_marker_invalidated() {
        let (_dir, marker, fake, mut ducker) = setup();
        ducker.duck();
        fake.user_sets("spk", 0.6);
        ducker.sync(true);
        let saved: Marker = serde_json::from_slice(&std::fs::read(&marker).unwrap()).unwrap();
        assert!(!saved.active);
        // Still speaking: no fight, no re-duck.
        ducker.duck();
        assert!(close(fake.level("spk").unwrap(), 0.6));
        ducker.restore();
        assert!(close(fake.level("spk").unwrap(), 0.6));
        // Next turn ducks from the user's level.
        ducker.duck();
        assert!(close(fake.level("spk").unwrap(), 0.18));
    }

    #[test]
    fn manual_change_is_kept_on_restore_without_a_sync() {
        let (_dir, marker, fake, mut ducker) = setup();
        ducker.duck();
        fake.user_sets("spk", 0.5);
        ducker.restore();
        assert!(close(fake.level("spk").unwrap(), 0.5));
        assert!(!marker.exists());
    }

    #[test]
    fn output_change_restores_old_device_and_ducks_the_new_one() {
        let (_dir, _marker, fake, mut ducker) = setup();
        fake.add("bt", 0.5);
        ducker.duck();
        fake.set_default(Some("bt"));
        ducker.sync(true);
        assert!(close(fake.level("spk").unwrap(), 0.8));
        assert!(close(fake.level("bt").unwrap(), 0.15));
        ducker.restore();
        assert!(close(fake.level("bt").unwrap(), 0.5));
    }

    #[test]
    fn crash_recovery_restores_the_recorded_device() {
        let (_dir, marker, fake, mut ducker) = setup();
        ducker.duck();
        drop(ducker); // crash: no restore
        let mut volume = fake.clone();
        restore_from_marker(&mut volume, &marker);
        assert!(close(fake.level("spk").unwrap(), 0.8));
        assert!(!marker.exists());
    }

    #[test]
    fn crash_recovery_does_not_force_when_user_changed_or_device_absent() {
        let (_dir, marker, fake, mut ducker) = setup();
        ducker.duck();
        fake.user_sets("spk", 0.9);
        let mut volume = fake.clone();
        restore_from_marker(&mut volume, &marker);
        assert!(close(fake.level("spk").unwrap(), 0.9));
        assert!(!marker.exists());

        let (_dir, marker, fake, mut ducker) = setup();
        ducker.duck();
        fake.remove("spk");
        fake.add("other", 0.4);
        fake.set_default(Some("other"));
        let mut volume = fake.clone();
        restore_from_marker(&mut volume, &marker);
        assert!(close(fake.level("other").unwrap(), 0.4));
        assert!(!marker.exists());
    }

    #[test]
    fn crash_recovery_ignores_an_invalidated_marker() {
        let (_dir, marker, fake, mut ducker) = setup();
        ducker.duck();
        fake.user_sets("spk", 0.6);
        ducker.sync(true);
        // The user later lands on exactly the old ducked level by chance.
        fake.user_sets("spk", 0.24);
        let mut volume = fake.clone();
        restore_from_marker(&mut volume, &marker);
        assert!(close(fake.level("spk").unwrap(), 0.24));
        assert!(!marker.exists());
    }

    #[test]
    fn corrupt_marker_is_dropped() {
        let (_dir, marker, fake, _ducker) = setup();
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        std::fs::write(&marker, b"{not json").unwrap();
        let mut volume = fake.clone();
        restore_from_marker(&mut volume, &marker);
        assert!(!marker.exists());
        assert!(close(fake.level("spk").unwrap(), 0.8));
    }

    #[test]
    fn missing_marker_is_a_no_op() {
        let (_dir, marker, fake, _ducker) = setup();
        let mut volume = fake.clone();
        restore_from_marker(&mut volume, &marker);
        assert!(close(fake.level("spk").unwrap(), 0.8));
    }

    #[test]
    fn backend_errors_never_panic_or_leave_a_marker() {
        let dir = tempfile::tempdir().unwrap();
        let marker = crate::core::paths::ducking_marker_path(dir.path());
        let mut ducker = Ducker::new(Box::new(FakeVolume::default()), marker.clone());
        ducker.duck();
        ducker.restore();
        assert!(!ducker.is_ducked() && !marker.exists());
    }
}
