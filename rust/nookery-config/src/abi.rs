//! C ABI for the settings document and the load-bearing `PCSettings` mirror.

use std::ffi::{CStr, c_char};
use std::mem::{align_of, offset_of, size_of};
use std::path::PathBuf;

use crate::codec::{ConfigDocument, ConfigError, ConfigFileStatus};
use crate::enums::TravelMoveOutChance;
use crate::settings::Settings;

/// ABI version for the settings header.
pub const ABI_VERSION: u32 = 1;

/// Status returned by every settings ABI operation.
pub type RawStatus = u32;

/// Operation succeeded.
pub const STATUS_OK: RawStatus = 0;
/// The config path does not exist; defaults were loaded.
pub const STATUS_FILE_MISSING: RawStatus = 1;
/// The config path is empty; defaults were loaded.
pub const STATUS_EMPTY_FILE: RawStatus = 2;
/// The config path contains malformed JSON.
pub const STATUS_MALFORMED_JSON: RawStatus = 3;
/// The JSON root is not an object.
pub const STATUS_WRONG_ROOT_TYPE: RawStatus = 4;
/// An ABI pointer or path argument is invalid.
pub const STATUS_INVALID_ARGUMENT: RawStatus = 5;
/// The filesystem operation failed.
pub const STATUS_IO_ERROR: RawStatus = 6;
/// Saving failed.
pub const STATUS_SAVE_ERROR: RawStatus = 7;
/// At least one recognized key had a wrong type and was reset to default.
pub const STATUS_WRONG_TYPE: RawStatus = 8;
/// The resolution direction was zero.
pub const STATUS_INVALID_DIRECTION: RawStatus = 9;
/// Highest defined status.
pub const STATUS_MAX: RawStatus = STATUS_INVALID_DIRECTION;

/// C mirror of `pc/include/pc_settings.h`; do not reorder fields.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PCSettings {
    /// Window width.
    pub window_width: i32,
    /// Window height.
    pub window_height: i32,
    /// 0 windowed, 1 fullscreen, 2 borderless.
    pub fullscreen: i32,
    /// 0 off, 1 on.
    pub vsync: i32,
    /// 0 uncapped, otherwise the frame target.
    pub max_fps: i32,
    /// Texture preload mode.
    pub preload_textures: i32,
    /// Disable Resetti penalty.
    pub disable_resetti: i32,
    /// Skip the foreign-town shop visitor requirement.
    pub disable_shop_visitor_req: i32,
    /// Continuous acre transitions.
    pub borderless_acres: i32,
    /// NES pillarbox mode.
    pub nes_aspect: i32,
    /// Master volume percentage.
    pub master_volume: i32,
    /// Main-stick deadzone percentage.
    pub stick_deadzone: i32,
    /// C-stick deadzone percentage.
    pub cstick_deadzone: i32,
    /// Always use fast text.
    pub fast_text: i32,
    /// Villager travel move-out chance.
    pub travel_move_out_chance: i32,
    /// Anchor the HUD to widescreen edges.
    pub hud_at_edges: i32,
}

// Oracle layout copied from pc/include/pc_settings.h. The C harness asserts
// the same literals against that header, so editing either side is detected.
const ORACLE_PC_SETTINGS_SIZE: usize = 64; // pc_settings.h:8-25
const ORACLE_PC_SETTINGS_ALIGN: usize = 4; // pc_settings.h:8-25
const ORACLE_PC_SETTINGS_FIELDS: [(usize, usize); 16] = [
    (0, 4),  // window_width: pc_settings.h:9
    (4, 4),  // window_height: pc_settings.h:10
    (8, 4),  // fullscreen: pc_settings.h:11
    (12, 4), // vsync: pc_settings.h:12
    (16, 4), // max_fps: pc_settings.h:13
    (20, 4), // preload_textures: pc_settings.h:14
    (24, 4), // disable_resetti: pc_settings.h:15
    (28, 4), // disable_shop_visitor_req: pc_settings.h:16
    (32, 4), // borderless_acres: pc_settings.h:17
    (36, 4), // nes_aspect: pc_settings.h:18
    (40, 4), // master_volume: pc_settings.h:19
    (44, 4), // stick_deadzone: pc_settings.h:20
    (48, 4), // cstick_deadzone: pc_settings.h:21
    (52, 4), // fast_text: pc_settings.h:22
    (56, 4), // travel_move_out_chance: pc_settings.h:23
    (60, 4), // hud_at_edges: pc_settings.h:24
];

const _: () = {
    assert!(size_of::<PCSettings>() == ORACLE_PC_SETTINGS_SIZE);
    assert!(align_of::<PCSettings>() == ORACLE_PC_SETTINGS_ALIGN);
    assert!(size_of::<i32>() == ORACLE_PC_SETTINGS_FIELDS[0].1);
    assert!(size_of::<i32>() == ORACLE_PC_SETTINGS_FIELDS[1].1);
    assert!(size_of::<i32>() == ORACLE_PC_SETTINGS_FIELDS[2].1);
    assert!(size_of::<i32>() == ORACLE_PC_SETTINGS_FIELDS[3].1);
    assert!(size_of::<i32>() == ORACLE_PC_SETTINGS_FIELDS[4].1);
    assert!(size_of::<i32>() == ORACLE_PC_SETTINGS_FIELDS[5].1);
    assert!(size_of::<i32>() == ORACLE_PC_SETTINGS_FIELDS[6].1);
    assert!(size_of::<i32>() == ORACLE_PC_SETTINGS_FIELDS[7].1);
    assert!(size_of::<i32>() == ORACLE_PC_SETTINGS_FIELDS[8].1);
    assert!(size_of::<i32>() == ORACLE_PC_SETTINGS_FIELDS[9].1);
    assert!(size_of::<i32>() == ORACLE_PC_SETTINGS_FIELDS[10].1);
    assert!(size_of::<i32>() == ORACLE_PC_SETTINGS_FIELDS[11].1);
    assert!(size_of::<i32>() == ORACLE_PC_SETTINGS_FIELDS[12].1);
    assert!(size_of::<i32>() == ORACLE_PC_SETTINGS_FIELDS[13].1);
    assert!(size_of::<i32>() == ORACLE_PC_SETTINGS_FIELDS[14].1);
    assert!(size_of::<i32>() == ORACLE_PC_SETTINGS_FIELDS[15].1);
    assert!(offset_of!(PCSettings, window_width) == ORACLE_PC_SETTINGS_FIELDS[0].0);
    assert!(offset_of!(PCSettings, window_height) == ORACLE_PC_SETTINGS_FIELDS[1].0);
    assert!(offset_of!(PCSettings, fullscreen) == ORACLE_PC_SETTINGS_FIELDS[2].0);
    assert!(offset_of!(PCSettings, vsync) == ORACLE_PC_SETTINGS_FIELDS[3].0);
    assert!(offset_of!(PCSettings, max_fps) == ORACLE_PC_SETTINGS_FIELDS[4].0);
    assert!(offset_of!(PCSettings, preload_textures) == ORACLE_PC_SETTINGS_FIELDS[5].0);
    assert!(offset_of!(PCSettings, disable_resetti) == ORACLE_PC_SETTINGS_FIELDS[6].0);
    assert!(offset_of!(PCSettings, disable_shop_visitor_req) == ORACLE_PC_SETTINGS_FIELDS[7].0);
    assert!(offset_of!(PCSettings, borderless_acres) == ORACLE_PC_SETTINGS_FIELDS[8].0);
    assert!(offset_of!(PCSettings, nes_aspect) == ORACLE_PC_SETTINGS_FIELDS[9].0);
    assert!(offset_of!(PCSettings, master_volume) == ORACLE_PC_SETTINGS_FIELDS[10].0);
    assert!(offset_of!(PCSettings, stick_deadzone) == ORACLE_PC_SETTINGS_FIELDS[11].0);
    assert!(offset_of!(PCSettings, cstick_deadzone) == ORACLE_PC_SETTINGS_FIELDS[12].0);
    assert!(offset_of!(PCSettings, fast_text) == ORACLE_PC_SETTINGS_FIELDS[13].0);
    assert!(offset_of!(PCSettings, travel_move_out_chance) == ORACLE_PC_SETTINGS_FIELDS[14].0);
    assert!(offset_of!(PCSettings, hud_at_edges) == ORACLE_PC_SETTINGS_FIELDS[15].0);
};

/// Owned ABI handle. The document retains unknown JSON fields between load and save.
pub struct NookerySettingsHandle {
    path: PathBuf,
    document: ConfigDocument,
}

/// Fixed resolution presets from `pc/include/acgc/settings.hpp`.
pub const WINDOW_RESOLUTIONS: &[(i32, i32)] = &[
    (640, 480),
    (800, 600),
    (960, 720),
    (1024, 768),
    (1152, 864),
    (1280, 960),
    (1400, 1050),
    (1600, 1200),
    (1280, 720),
    (1366, 768),
    (1600, 900),
    (1920, 1080),
    (2560, 1440),
    (3840, 2160),
];

/// Convert the C mirror to the Rust settings model.
pub fn settings_from_pc(value: &PCSettings) -> Settings {
    let mut settings = Settings::default();
    settings.video.window_width = i64::from(value.window_width);
    settings.video.window_height = i64::from(value.window_height);
    settings.video.window_mode = i64::from(value.fullscreen);
    settings.video.vsync = value.vsync != 0;
    settings.video.max_frame_rate = i64::from(value.max_fps);
    settings.enhancements.preload_textures = i64::from(value.preload_textures);
    settings.enhancements.disable_resetti = value.disable_resetti != 0;
    settings.enhancements.disable_shop_visitor_requirement = value.disable_shop_visitor_req != 0;
    settings.enhancements.borderless_acres = value.borderless_acres != 0;
    settings.enhancements.pillarbox_nes = value.nes_aspect != 0;
    settings.enhancements.fast_text = value.fast_text != 0;
    settings.enhancements.travel_move_out_chance = match value.travel_move_out_chance {
        10 => TravelMoveOutChance::Ten,
        50 => TravelMoveOutChance::Fifty,
        _ => TravelMoveOutChance::Default,
    };
    settings.audio.master_volume = i64::from(value.master_volume);
    settings.input.stick_deadzone = i64::from(value.stick_deadzone);
    settings.input.cstick_deadzone = i64::from(value.cstick_deadzone);
    settings.video.hud_at_edges = value.hud_at_edges != 0;
    settings
}

/// Convert the Rust settings model to the C mirror after its repair rules run.
pub fn pc_from_settings(mut settings: Settings) -> PCSettings {
    settings.sanitize();
    PCSettings {
        window_width: settings.video.window_width as i32,
        window_height: settings.video.window_height as i32,
        fullscreen: settings.video.window_mode as i32,
        vsync: i32::from(settings.video.vsync),
        max_fps: settings.video.max_frame_rate as i32,
        preload_textures: settings.enhancements.preload_textures as i32,
        disable_resetti: i32::from(settings.enhancements.disable_resetti),
        disable_shop_visitor_req: i32::from(settings.enhancements.disable_shop_visitor_requirement),
        borderless_acres: i32::from(settings.enhancements.borderless_acres),
        nes_aspect: i32::from(settings.enhancements.pillarbox_nes),
        master_volume: settings.audio.master_volume as i32,
        stick_deadzone: settings.input.stick_deadzone as i32,
        cstick_deadzone: settings.input.cstick_deadzone as i32,
        fast_text: i32::from(settings.enhancements.fast_text),
        travel_move_out_chance: match settings.enhancements.travel_move_out_chance {
            TravelMoveOutChance::Ten => 10,
            TravelMoveOutChance::Fifty => 50,
            TravelMoveOutChance::Default => 100,
        },
        hud_at_edges: i32::from(settings.video.hud_at_edges),
    }
}

/// Return a stable name for a status code.
pub fn status_name(status: RawStatus) -> &'static [u8] {
    match status {
        STATUS_OK => b"OK\0",
        STATUS_FILE_MISSING => b"FILE_MISSING\0",
        STATUS_EMPTY_FILE => b"EMPTY_FILE\0",
        STATUS_MALFORMED_JSON => b"MALFORMED_JSON\0",
        STATUS_WRONG_ROOT_TYPE => b"WRONG_ROOT_TYPE\0",
        STATUS_INVALID_ARGUMENT => b"INVALID_ARGUMENT\0",
        STATUS_IO_ERROR => b"IO_ERROR\0",
        STATUS_SAVE_ERROR => b"SAVE_ERROR\0",
        STATUS_WRONG_TYPE => b"WRONG_TYPE\0",
        STATUS_INVALID_DIRECTION => b"INVALID_DIRECTION\0",
        _ => b"UNKNOWN\0",
    }
}

fn path_from_c(path: *const c_char) -> Result<PathBuf, RawStatus> {
    if path.is_null() {
        return Err(STATUS_INVALID_ARGUMENT);
    }
    // SAFETY: the C ABI requires a valid NUL-terminated path pointer.
    let bytes = unsafe { CStr::from_ptr(path) }.to_bytes();
    let text = std::str::from_utf8(bytes).map_err(|_| STATUS_INVALID_ARGUMENT)?;
    if text.is_empty() {
        return Err(STATUS_INVALID_ARGUMENT);
    }
    Ok(PathBuf::from(text))
}

fn map_config_error(error: &ConfigError) -> RawStatus {
    match error {
        ConfigError::RootType => STATUS_WRONG_ROOT_TYPE,
        ConfigError::Json(_) => STATUS_MALFORMED_JSON,
        ConfigError::Io(_) => STATUS_IO_ERROR,
    }
}

/// Return the ABI version expected by the C header.
#[unsafe(no_mangle)]
pub extern "C" fn nookery_settings_abi_version() -> u32 {
    ABI_VERSION
}

/// Return the number of defined status codes.
#[unsafe(no_mangle)]
pub extern "C" fn nookery_settings_status_count() -> u32 {
    STATUS_MAX + 1
}

/// Return a process-lifetime status name.
#[unsafe(no_mangle)]
pub extern "C" fn nookery_settings_status_name(status: RawStatus) -> *const c_char {
    status_name(status).as_ptr().cast()
}

/// Create a handle for a config path. No file is opened by this call.
///
/// # Safety
/// `path` must be null or a valid NUL-terminated UTF-8 string, and
/// `out_handle` must point to writable storage for one handle pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nookery_settings_create(
    path: *const c_char,
    out_handle: *mut *mut NookerySettingsHandle,
) -> RawStatus {
    if out_handle.is_null() {
        return STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: validated above; initialize the caller's out pointer on every path.
    unsafe { *out_handle = std::ptr::null_mut() };
    let path = match path_from_c(path) {
        Ok(path) => path,
        Err(status) => return status,
    };
    let handle = Box::new(NookerySettingsHandle {
        path,
        document: ConfigDocument::default(),
    });
    // SAFETY: Box ownership is transferred to the caller.
    unsafe { *out_handle = Box::into_raw(handle) };
    STATUS_OK
}

/// Destroy a handle; null is accepted.
///
/// # Safety
/// `handle` must be null or a pointer returned by
/// [`nookery_settings_create`] that has not already been destroyed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nookery_settings_destroy(handle: *mut NookerySettingsHandle) {
    if !handle.is_null() {
        // SAFETY: handle was returned by `nookery_settings_create` and is consumed once.
        drop(unsafe { Box::from_raw(handle) });
    }
}

/// Load a document into a handle and return its PC mirror.
///
/// # Safety
/// `handle` must be a live handle returned by [`nookery_settings_create`],
/// and `out_settings` must point to writable [`PCSettings`] storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nookery_settings_load(
    handle: *mut NookerySettingsHandle,
    out_settings: *mut PCSettings,
) -> RawStatus {
    if handle.is_null() || out_settings.is_null() {
        return STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: pointers were checked and are caller-owned valid objects.
    let handle = unsafe { &mut *handle };
    let loaded = match ConfigDocument::load_path(&handle.path) {
        Ok(loaded) => loaded,
        Err(error) => return map_config_error(&error),
    };
    let mut document = loaded.document;
    document.settings.sanitize_renderer(&["auto"]);
    // SAFETY: pointer was checked above.
    unsafe { *out_settings = pc_from_settings(document.settings.clone()) };
    let issues = !document.issues.is_empty();
    handle.document = document;
    if issues {
        STATUS_WRONG_TYPE
    } else {
        match loaded.status {
            ConfigFileStatus::Loaded => STATUS_OK,
            ConfigFileStatus::FileMissing => STATUS_FILE_MISSING,
            ConfigFileStatus::Empty => STATUS_EMPTY_FILE,
        }
    }
}

/// Save a PC mirror while retaining the handle's unknown JSON fields.
///
/// # Safety
/// `handle` must be a live handle returned by [`nookery_settings_create`],
/// and `settings` must point to readable [`PCSettings`] storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nookery_settings_save(
    handle: *mut NookerySettingsHandle,
    settings: *const PCSettings,
) -> RawStatus {
    if handle.is_null() || settings.is_null() {
        return STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: pointers were checked and are caller-owned valid objects.
    let handle = unsafe { &mut *handle };
    let settings = unsafe { &*settings };
    handle.document.settings = settings_from_pc(settings);
    match handle.document.save_path(&handle.path) {
        Ok(()) => STATUS_OK,
        Err(_) => STATUS_SAVE_ERROR,
    }
}

/// Apply all PC mirror clamps and normalize booleans/enums in place.
///
/// # Safety
/// `settings` must point to writable [`PCSettings`] storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nookery_settings_apply(settings: *mut PCSettings) -> RawStatus {
    if settings.is_null() {
        return STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: pointer was checked above.
    let settings = unsafe { &mut *settings };
    *settings = pc_from_settings(settings_from_pc(settings));
    STATUS_OK
}

/// Cycle through the fixed upstream resolution presets, wrapping at either end.
///
/// # Safety
/// `width` and `height` must point to writable `i32` storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nookery_settings_cycle_resolution(
    width: *mut i32,
    height: *mut i32,
    direction: i32,
) -> RawStatus {
    if width.is_null() || height.is_null() || direction == 0 {
        return if direction == 0 {
            STATUS_INVALID_DIRECTION
        } else {
            STATUS_INVALID_ARGUMENT
        };
    }
    // SAFETY: pointers were checked above.
    let width_ref = unsafe { &mut *width };
    let height_ref = unsafe { &mut *height };
    let current = WINDOW_RESOLUTIONS
        .iter()
        .position(|&(w, h)| w == *width_ref && h == *height_ref);
    let index = match current {
        Some(index) if direction > 0 => (index + 1) % WINDOW_RESOLUTIONS.len(),
        Some(index) => {
            if index == 0 {
                WINDOW_RESOLUTIONS.len() - 1
            } else {
                index - 1
            }
        }
        None if direction > 0 => 0,
        None => WINDOW_RESOLUTIONS.len() - 1,
    };
    let (new_width, new_height) = WINDOW_RESOLUTIONS[index];
    *width_ref = new_width;
    *height_ref = new_height;
    STATUS_OK
}
