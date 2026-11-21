//! Release-only executable probe for the exported panic contract.

use std::ffi::c_void;

use nookery_runtime::{
    NookeryRuntimeCallbackStatus, NookeryRuntimeCallbacks, NookeryRuntimeConfig,
    NookeryRuntimeStatus, nookery_runtime_boot, nookery_runtime_create, nookery_runtime_destroy,
};

unsafe extern "C-unwind" fn panic_callback(
    _user_data: *mut c_void,
) -> NookeryRuntimeCallbackStatus {
    panic!("release boundary probe")
}

fn main() {
    let callbacks = NookeryRuntimeCallbacks {
        game_init: Some(panic_callback),
        ..NookeryRuntimeCallbacks::default()
    };
    let config = NookeryRuntimeConfig::default();
    let mut handle = std::ptr::null_mut();
    assert_eq!(
        unsafe { nookery_runtime_create(&callbacks, &config, &mut handle) },
        NookeryRuntimeStatus::Ok
    );
    assert_eq!(nookery_runtime_boot(handle), NookeryRuntimeStatus::Panic);
    assert_eq!(
        unsafe { nookery_runtime_destroy(handle) },
        NookeryRuntimeStatus::InvalidState
    );
}
