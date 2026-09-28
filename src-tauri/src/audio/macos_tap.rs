//! macOS system-audio capture backed by a self-excluding Core Audio process tap.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::thread;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait};
use objc2::AnyThread;
use objc2_core_audio::{
    kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceNameKey,
    kAudioAggregateDeviceTapAutoStartKey, kAudioAggregateDeviceTapListKey,
    kAudioAggregateDeviceUIDKey, kAudioHardwarePropertyTranslatePIDToProcessObject,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
    kAudioObjectUnknown, kAudioSubTapDriftCompensationKey, kAudioSubTapUIDKey,
    AudioHardwareCreateAggregateDevice, AudioHardwareCreateProcessTap,
    AudioHardwareDestroyAggregateDevice, AudioHardwareDestroyProcessTap,
    AudioObjectGetPropertyData, AudioObjectID, AudioObjectPropertyAddress, CATapDescription,
    CATapMuteBehavior,
};
use objc2_core_foundation::{
    kCFAllocatorDefault, kCFTypeArrayCallBacks, kCFTypeDictionaryKeyCallBacks,
    kCFTypeDictionaryValueCallBacks, CFArray, CFDictionary, CFMutableDictionary, CFRetained,
    CFString,
};
use objc2_foundation::{NSArray, NSNumber, NSProcessInfo, NSString};

use crate::core::error::{AppError, Code};

const DEVICE_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) fn supported_by_os() -> bool {
    let version = NSProcessInfo::processInfo().operatingSystemVersion();
    is_supported_version(version.majorVersion, version.minorVersion)
}

fn is_supported_version(major: isize, minor: isize) -> bool {
    major > 14 || (major == 14 && minor >= 4)
}

/// Private aggregate input backed by a process tap. It owns both Core Audio
/// objects; dropping it destroys the aggregate first, then the tap.
pub(super) struct TapAggregate {
    tap_id: AudioObjectID,
    aggregate_device_id: AudioObjectID,
}

impl TapAggregate {
    pub(super) fn create() -> Result<(cpal::Device, Self), AppError> {
        let host = cpal::default_host();
        let process_object_id = current_process_object_id()?;
        let processes = NSArray::from_retained_slice(&[NSNumber::new_u32(process_object_id)]);
        let description = unsafe {
            CATapDescription::initStereoGlobalTapButExcludeProcesses(
                CATapDescription::alloc(),
                &processes,
            )
        };
        unsafe {
            description.setMuteBehavior(CATapMuteBehavior::Unmuted);
            description.setName(&NSString::from_str("trans-kun system audio"));
            description.setPrivate(true);
            description.setExclusive(true);
        }

        let mut tap_id = kAudioObjectUnknown;
        let status =
            unsafe { AudioHardwareCreateProcessTap(Some(description.as_ref()), &mut tap_id) };
        check_status(status, "create the Core Audio process tap")?;
        let mut cleanup = TapCleanup {
            tap_id: Some(tap_id),
            aggregate_device_id: None,
        };

        let tap_uid = unsafe { description.UUID().UUIDString() };
        let aggregate_uid = format!("com.transkun.systemtap.{}", uuid::Uuid::now_v7());
        let aggregate_name = format!("trans-kun system capture {}", uuid::Uuid::now_v7());
        let properties = aggregate_device_properties(&tap_uid, &aggregate_uid, &aggregate_name)?;
        let mut aggregate_device_id = kAudioObjectUnknown;
        let status = unsafe {
            AudioHardwareCreateAggregateDevice(
                properties.as_ref(),
                NonNull::from(&mut aggregate_device_id),
            )
        };
        check_status(status, "create the Core Audio aggregate device")?;
        cleanup.aggregate_device_id = Some(aggregate_device_id);

        let device = find_input_device(&host, &aggregate_uid)?;
        let resources = cleanup.into_resources();
        Ok((device, resources))
    }
}

impl Drop for TapAggregate {
    fn drop(&mut self) {
        destroy_core_audio_objects(
            Some(self.aggregate_device_id),
            Some(self.tap_id),
            |kind, id| unsafe { destroy_core_audio_object(kind, id) },
        );
    }
}

struct TapCleanup {
    tap_id: Option<AudioObjectID>,
    aggregate_device_id: Option<AudioObjectID>,
}

impl TapCleanup {
    fn into_resources(mut self) -> TapAggregate {
        let resources = TapAggregate {
            tap_id: self.tap_id.take().expect("created tap is tracked"),
            aggregate_device_id: self
                .aggregate_device_id
                .take()
                .expect("created aggregate is tracked"),
        };
        resources
    }
}

impl Drop for TapCleanup {
    fn drop(&mut self) {
        destroy_core_audio_objects(
            self.aggregate_device_id.take(),
            self.tap_id.take(),
            |kind, id| unsafe { destroy_core_audio_object(kind, id) },
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CoreAudioObjectKind {
    Aggregate,
    Tap,
}

fn destroy_core_audio_objects(
    aggregate_device_id: Option<AudioObjectID>,
    tap_id: Option<AudioObjectID>,
    mut destroy: impl FnMut(CoreAudioObjectKind, AudioObjectID),
) {
    if let Some(aggregate_device_id) = aggregate_device_id {
        destroy(CoreAudioObjectKind::Aggregate, aggregate_device_id);
    }
    if let Some(tap_id) = tap_id {
        destroy(CoreAudioObjectKind::Tap, tap_id);
    }
}

unsafe fn destroy_core_audio_object(kind: CoreAudioObjectKind, id: AudioObjectID) {
    match kind {
        CoreAudioObjectKind::Aggregate => {
            let _ = AudioHardwareDestroyAggregateDevice(id);
        }
        CoreAudioObjectKind::Tap => {
            let _ = AudioHardwareDestroyProcessTap(id);
        }
    }
}

fn current_process_object_id() -> Result<AudioObjectID, AppError> {
    let pid = i32::try_from(std::process::id())
        .map_err(|_| system_audio_error("the application PID is outside Core Audio's range"))?;
    let mut address = AudioObjectPropertyAddress {
        mSelector: kAudioHardwarePropertyTranslatePIDToProcessObject,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };
    let mut data_size = std::mem::size_of::<AudioObjectID>() as u32;
    let mut process_object_id = kAudioObjectUnknown;
    let status = unsafe {
        AudioObjectGetPropertyData(
            kAudioObjectSystemObject as AudioObjectID,
            NonNull::from(&mut address),
            std::mem::size_of_val(&pid) as u32,
            (&pid as *const i32).cast(),
            NonNull::from(&mut data_size),
            NonNull::from(&mut process_object_id).cast(),
        )
    };
    check_status(
        status,
        "translate the application PID to a Core Audio process object",
    )?;
    if process_object_id == kAudioObjectUnknown {
        return Err(system_audio_error(
            "Core Audio did not return a process object for the application",
        ));
    }
    Ok(process_object_id)
}

fn find_input_device(host: &cpal::Host, uid: &str) -> Result<cpal::Device, AppError> {
    let deadline = Instant::now() + DEVICE_DISCOVERY_TIMEOUT;
    let mut last_error = None;
    loop {
        match host.input_devices() {
            Ok(devices) => {
                for device in devices {
                    match device.id() {
                        Ok(device_id) if device_id.id() == uid => return Ok(device),
                        Ok(_) => {}
                        Err(error) => last_error = Some(error.to_string()),
                    }
                }
            }
            Err(error) => last_error = Some(error.to_string()),
        }
        if Instant::now() >= deadline {
            let detail = last_error.unwrap_or_else(|| {
                "Core Audio did not publish the new aggregate input device".to_owned()
            });
            return Err(system_audio_error(format!(
                "could not open the aggregate input device: {detail}"
            )));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn aggregate_device_properties(
    tap_uid: &NSString,
    aggregate_uid: &str,
    aggregate_name: &str,
) -> Result<CFRetained<CFDictionary>, AppError> {
    let tap_properties = unsafe {
        CFMutableDictionary::new(
            kCFAllocatorDefault,
            2,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        )
    }
    .ok_or_else(|| system_audio_error("could not allocate Core Audio tap properties"))?;
    set_cf_value(
        tap_properties.as_ref(),
        kAudioSubTapUIDKey,
        tap_uid as *const NSString as *const c_void,
    );
    let drift_compensation = NSNumber::new_bool(true);
    set_cf_value(
        tap_properties.as_ref(),
        kAudioSubTapDriftCompensationKey,
        &*drift_compensation as *const NSNumber as *const c_void,
    );

    let tap_values = [tap_properties];
    let tap_list = unsafe {
        CFArray::new(
            kCFAllocatorDefault,
            tap_values.as_ptr() as *mut *const c_void,
            tap_values.len() as _,
            &kCFTypeArrayCallBacks,
        )
    }
    .ok_or_else(|| system_audio_error("could not allocate the Core Audio tap list"))?;

    let properties = unsafe {
        CFMutableDictionary::new(
            kCFAllocatorDefault,
            5,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        )
    }
    .ok_or_else(|| system_audio_error("could not allocate Core Audio aggregate properties"))?;
    let name = CFString::from_str(aggregate_name);
    let uid = CFString::from_str(aggregate_uid);
    let auto_start_tap = NSNumber::new_bool(true);
    let is_private = NSNumber::new_bool(true);
    set_cf_value(
        properties.as_ref(),
        kAudioAggregateDeviceNameKey,
        &*name as *const CFString as *const c_void,
    );
    set_cf_value(
        properties.as_ref(),
        kAudioAggregateDeviceUIDKey,
        &*uid as *const CFString as *const c_void,
    );
    set_cf_value(
        properties.as_ref(),
        kAudioAggregateDeviceTapListKey,
        &*tap_list as *const CFArray as *const c_void,
    );
    set_cf_value(
        properties.as_ref(),
        kAudioAggregateDeviceTapAutoStartKey,
        &*auto_start_tap as *const NSNumber as *const c_void,
    );
    set_cf_value(
        properties.as_ref(),
        kAudioAggregateDeviceIsPrivateKey,
        &*is_private as *const NSNumber as *const c_void,
    );
    Ok(unsafe { CFRetained::cast_unchecked(properties) })
}

fn set_cf_value(dictionary: &CFMutableDictionary, key: &std::ffi::CStr, value: *const c_void) {
    let key = CFString::from_str(key.to_str().expect("Core Audio keys are UTF-8"));
    unsafe {
        CFMutableDictionary::set_value(
            Some(dictionary),
            &*key as *const CFString as *const c_void,
            value,
        );
    }
}

fn check_status(status: i32, action: &str) -> Result<(), AppError> {
    if status == 0 {
        Ok(())
    } else {
        Err(system_audio_error(format!(
            "could not {action} (Core Audio status {status})"
        )))
    }
}

fn system_audio_error(detail: impl AsRef<str>) -> AppError {
    AppError::new(
        Code::Permission,
        format!(
            "System audio capture failed: {}. If macOS denied access, allow it in System Settings > Privacy & Security > Screen & System Audio Recording, then retry.",
            detail.as_ref()
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::{destroy_core_audio_objects, is_supported_version, CoreAudioObjectKind};
    use objc2::AnyThread;
    use objc2_core_audio::{AudioObjectID, CATapDescription};
    use objc2_foundation::{NSArray, NSNumber};

    #[test]
    fn core_audio_taps_require_macos_14_4_or_newer() {
        assert!(!is_supported_version(13, 7));
        assert!(!is_supported_version(14, 3));
        assert!(is_supported_version(14, 4));
        assert!(is_supported_version(15, 0));
    }

    #[test]
    fn global_tap_description_excludes_the_supplied_process_object() {
        let process_object_id: AudioObjectID = 0x1020;
        let processes = NSArray::from_retained_slice(&[NSNumber::new_u32(process_object_id)]);
        let description = unsafe {
            CATapDescription::initStereoGlobalTapButExcludeProcesses(
                CATapDescription::alloc(),
                &processes,
            )
        };
        unsafe { description.setExclusive(true) };

        let excluded = unsafe { description.processes() };
        assert_eq!(excluded.firstObject().unwrap().as_u32(), process_object_id);
        assert!(unsafe { description.isExclusive() });
    }

    #[test]
    fn cleanup_destroys_aggregate_before_tap_and_cleans_partial_setup() {
        let mut destroyed = Vec::new();
        destroy_core_audio_objects(Some(7), Some(8), |kind, id| destroyed.push((kind, id)));
        assert_eq!(
            destroyed,
            vec![
                (CoreAudioObjectKind::Aggregate, 7),
                (CoreAudioObjectKind::Tap, 8)
            ]
        );

        destroyed.clear();
        destroy_core_audio_objects(None, Some(8), |kind, id| destroyed.push((kind, id)));
        assert_eq!(destroyed, vec![(CoreAudioObjectKind::Tap, 8)]);
    }
}
