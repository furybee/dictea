//! Apple's on-device model (macOS 26+), reached through the Swift shim in
//! `swift/FoundationModelsFFI.swift`.
//!
//! Nothing leaves the machine: this is what makes "reformulate locally"
//! possible alongside the local Parakeet engine.

/// Why the on-device model cannot be used, if it cannot
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Available,
    /// Not macOS, or macOS older than 26
    UnsupportedOs,
    /// The user has not turned Apple Intelligence on
    AppleIntelligenceOff,
    /// Enabled, but the model is still downloading or otherwise not ready
    ModelNotReady,
    /// Hardware that Apple Intelligence does not support
    DeviceUnsupported,
    Unknown,
}

impl Availability {
    fn from_code(code: i32) -> Self {
        match code {
            0 => Availability::Available,
            1 => Availability::UnsupportedOs,
            2 => Availability::AppleIntelligenceOff,
            3 => Availability::ModelNotReady,
            4 => Availability::DeviceUnsupported,
            _ => Availability::Unknown,
        }
    }

    /// Message shown to the user when a dictation cannot be reformulated
    pub fn message(&self) -> &'static str {
        match self {
            Availability::Available => "Apple Intelligence is available",
            Availability::UnsupportedOs => "Apple Intelligence requires macOS 26 or later",
            Availability::AppleIntelligenceOff => {
                "Apple Intelligence is turned off in System Settings"
            }
            Availability::ModelNotReady => "The Apple Intelligence model is not ready yet",
            Availability::DeviceUnsupported => {
                "This device does not support Apple Intelligence"
            }
            Availability::Unknown => "Apple Intelligence is unavailable",
        }
    }
}

#[cfg(target_os = "macos")]
mod ffi {
    use super::Availability;
    use std::ffi::{c_char, CStr, CString};

    unsafe extern "C" {
        fn dictea_fm_availability() -> i32;
        fn dictea_fm_respond(
            instructions: *const c_char,
            prompt: *const c_char,
            error_out: *mut *mut c_char,
        ) -> *mut c_char;
        fn dictea_fm_free(ptr: *mut c_char);
    }

    /// Take ownership of a string produced by the shim
    unsafe fn take_string(ptr: *mut c_char) -> String {
        let text = unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned();
        unsafe { dictea_fm_free(ptr) };
        text
    }

    pub fn availability() -> Availability {
        Availability::from_code(unsafe { dictea_fm_availability() })
    }

    /// Blocking: parks the calling thread until the model answers.
    /// Callers on the async runtime must go through `spawn_blocking`.
    pub fn respond(instructions: &str, prompt: &str) -> Result<String, String> {
        let instructions =
            CString::new(instructions).map_err(|_| "Instructions contain a NUL byte".to_string())?;
        let prompt =
            CString::new(prompt).map_err(|_| "Prompt contains a NUL byte".to_string())?;

        let mut error: *mut c_char = std::ptr::null_mut();
        let answer =
            unsafe { dictea_fm_respond(instructions.as_ptr(), prompt.as_ptr(), &mut error) };

        if answer.is_null() {
            let message = if error.is_null() {
                "Apple Intelligence returned nothing".to_string()
            } else {
                unsafe { take_string(error) }
            };
            return Err(message);
        }
        Ok(unsafe { take_string(answer) })
    }
}

#[cfg(not(target_os = "macos"))]
mod ffi {
    use super::Availability;

    pub fn availability() -> Availability {
        Availability::UnsupportedOs
    }

    pub fn respond(_instructions: &str, _prompt: &str) -> Result<String, String> {
        Err("Apple Intelligence is only available on macOS".to_string())
    }
}

pub use ffi::{availability, respond};

#[cfg(test)]
mod tests {
    use super::*;

    /// The FFI round-trip must hold whatever the machine reports: a bad string
    /// handoff would show up as a crash or garbage rather than a clean enum.
    #[test]
    fn availability_is_reported() {
        let state = availability();
        println!("availability: {:?} ({})", state, state.message());
        assert!(!state.message().is_empty());
    }

    /// Only meaningful where Apple Intelligence is actually on; elsewhere it
    /// asserts that the failure path returns an error instead of hanging.
    #[test]
    fn respond_round_trip() {
        let instructions = "Reply with exactly one word: OK. No punctuation.";
        match availability() {
            Availability::Available => {
                let answer = respond(instructions, "Say OK").expect("model should answer");
                println!("model answered: {:?}", answer);
                assert!(!answer.trim().is_empty());
            }
            state => {
                let err = respond(instructions, "Say OK").unwrap_err();
                println!("unavailable ({:?}), error: {}", state, err);
                assert!(!err.is_empty());
            }
        }
    }
}
