//! Platform detection for secure keyboard input.

/// Reports whether secure keyboard input is currently enabled.
pub trait SecureInput {
    fn enabled(&self) -> bool;
}

/// Queries native secure input state where supported.
#[derive(Debug, Default, Clone, Copy)]
pub struct NativeSecureInput;

impl SecureInput for NativeSecureInput {
    fn enabled(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            // SAFETY: This system query takes no arguments and has no caller
            // preconditions. Carbon's Boolean return type is an unsigned byte.
            unsafe { IsSecureEventInputEnabled() != 0 }
        }

        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }
}

#[cfg(target_os = "macos")]
#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn IsSecureEventInputEnabled() -> u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubSecureInput(bool);

    impl SecureInput for StubSecureInput {
        fn enabled(&self) -> bool {
            self.0
        }
    }

    #[test]
    fn stub_reports_enabled_and_disabled_states() {
        for enabled in [false, true] {
            let input: &dyn SecureInput = &StubSecureInput(enabled);
            assert_eq!(input.enabled(), enabled);
        }
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn native_secure_input_is_disabled_on_unsupported_platforms() {
        let input = NativeSecureInput;
        assert!(!input.enabled());
    }
}
