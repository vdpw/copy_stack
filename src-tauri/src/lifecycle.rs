use tauri::{AppHandle, Manager, Runtime, WebviewWindow};

const MAIN_WINDOW_LABEL: &str = "main";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LifecycleError {
    MainWindowUnavailable,
    WindowShowFailed,
    WindowHideFailed,
    WindowUnminimizeFailed,
    WindowFocusFailed,
    AutostartReadFailed,
    AutostartWriteFailed,
    AutostartVerificationFailed,
}

impl LifecycleError {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::MainWindowUnavailable => "MAIN_WINDOW_UNAVAILABLE",
            Self::WindowShowFailed => "MAIN_WINDOW_SHOW_FAILED",
            Self::WindowHideFailed => "MAIN_WINDOW_HIDE_FAILED",
            Self::WindowUnminimizeFailed => "MAIN_WINDOW_UNMINIMIZE_FAILED",
            Self::WindowFocusFailed => "MAIN_WINDOW_FOCUS_FAILED",
            Self::AutostartReadFailed => "AUTOSTART_READ_FAILED",
            Self::AutostartWriteFailed => "AUTOSTART_WRITE_FAILED",
            Self::AutostartVerificationFailed => "AUTOSTART_VERIFICATION_FAILED",
        }
    }
}

trait MainWindowActions {
    fn show(&self) -> Result<(), ()>;
    fn hide(&self) -> Result<(), ()>;
    fn unminimize(&self) -> Result<(), ()>;
    fn focus(&self) -> Result<(), ()>;
}

impl<R: Runtime> MainWindowActions for WebviewWindow<R> {
    fn show(&self) -> Result<(), ()> {
        WebviewWindow::show(self).map_err(|_| ())
    }

    fn hide(&self) -> Result<(), ()> {
        WebviewWindow::hide(self).map_err(|_| ())
    }

    fn unminimize(&self) -> Result<(), ()> {
        WebviewWindow::unminimize(self).map_err(|_| ())
    }

    fn focus(&self) -> Result<(), ()> {
        self.set_focus().map_err(|_| ())
    }
}

fn activate_window(window: &impl MainWindowActions) -> Result<(), LifecycleError> {
    let mut first_error = None;

    if window.show().is_err() {
        first_error = Some(LifecycleError::WindowShowFailed);
    }
    if window.unminimize().is_err() && first_error.is_none() {
        first_error = Some(LifecycleError::WindowUnminimizeFailed);
    }
    if window.focus().is_err() && first_error.is_none() {
        first_error = Some(LifecycleError::WindowFocusFailed);
    }

    first_error.map_or(Ok(()), Err)
}

pub(crate) fn activate_main_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), LifecycleError> {
    let window = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or(LifecycleError::MainWindowUnavailable)?;
    activate_window(&window)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InitialWindowVisibility {
    Visible,
    Hidden,
}

fn initial_window_visibility(launched_at_login: bool) -> InitialWindowVisibility {
    if launched_at_login {
        InitialWindowVisibility::Hidden
    } else {
        InitialWindowVisibility::Visible
    }
}

fn apply_window_visibility(
    window: &impl MainWindowActions,
    visibility: InitialWindowVisibility,
) -> Result<(), LifecycleError> {
    match visibility {
        InitialWindowVisibility::Visible => {
            window.show().map_err(|_| LifecycleError::WindowShowFailed)
        }
        InitialWindowVisibility::Hidden => {
            window.hide().map_err(|_| LifecycleError::WindowHideFailed)
        }
    }
}

pub(crate) fn apply_initial_window_visibility<R: Runtime>(
    app: &AppHandle<R>,
    launched_at_login: bool,
) -> Result<(), LifecycleError> {
    let window = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or(LifecycleError::MainWindowUnavailable)?;
    apply_window_visibility(&window, initial_window_visibility(launched_at_login))
}

pub(crate) trait AutostartBackend {
    fn is_enabled(&self) -> Result<bool, ()>;
    fn enable(&self) -> Result<(), ()>;
    fn disable(&self) -> Result<(), ()>;
}

pub(crate) fn read_autostart_enabled(
    backend: &impl AutostartBackend,
) -> Result<bool, LifecycleError> {
    backend
        .is_enabled()
        .map_err(|_| LifecycleError::AutostartReadFailed)
}

pub(crate) fn set_autostart_enabled(
    backend: &impl AutostartBackend,
    enabled: bool,
) -> Result<bool, LifecycleError> {
    let write_result = if enabled {
        backend.enable()
    } else {
        backend.disable()
    };
    write_result.map_err(|_| LifecycleError::AutostartWriteFailed)?;

    let actual = backend
        .is_enabled()
        .map_err(|_| LifecycleError::AutostartReadFailed)?;
    if actual != enabled {
        return Err(LifecycleError::AutostartVerificationFailed);
    }

    Ok(actual)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    struct FakeWindow {
        calls: RefCell<Vec<&'static str>>,
        fail_show: bool,
        fail_hide: bool,
        fail_unminimize: bool,
        fail_focus: bool,
    }

    impl MainWindowActions for FakeWindow {
        fn show(&self) -> Result<(), ()> {
            self.calls.borrow_mut().push("show");
            (!self.fail_show).then_some(()).ok_or(())
        }

        fn hide(&self) -> Result<(), ()> {
            self.calls.borrow_mut().push("hide");
            (!self.fail_hide).then_some(()).ok_or(())
        }

        fn unminimize(&self) -> Result<(), ()> {
            self.calls.borrow_mut().push("unminimize");
            (!self.fail_unminimize).then_some(()).ok_or(())
        }

        fn focus(&self) -> Result<(), ()> {
            self.calls.borrow_mut().push("focus");
            (!self.fail_focus).then_some(()).ok_or(())
        }
    }

    #[test]
    fn second_instance_activation_shows_unminimizes_and_focuses() {
        let window = FakeWindow::default();

        assert_eq!(activate_window(&window), Ok(()));
        assert_eq!(
            window.calls.borrow().as_slice(),
            ["show", "unminimize", "focus"]
        );
    }

    #[test]
    fn activation_attempts_every_action_and_returns_first_failure() {
        let window = FakeWindow {
            fail_show: true,
            fail_focus: true,
            ..Default::default()
        };

        assert_eq!(
            activate_window(&window),
            Err(LifecycleError::WindowShowFailed)
        );
        assert_eq!(
            window.calls.borrow().as_slice(),
            ["show", "unminimize", "focus"]
        );
    }

    #[test]
    fn login_launch_is_hidden_and_manual_launch_is_visible() {
        let login_window = FakeWindow::default();
        let manual_window = FakeWindow::default();

        apply_window_visibility(&login_window, initial_window_visibility(true))
            .expect("login visibility should apply");
        apply_window_visibility(&manual_window, initial_window_visibility(false))
            .expect("manual visibility should apply");

        assert_eq!(login_window.calls.borrow().as_slice(), ["hide"]);
        assert_eq!(manual_window.calls.borrow().as_slice(), ["show"]);
    }

    struct FakeAutostart {
        enabled: Cell<bool>,
        fail_read: bool,
        fail_write: bool,
        ignore_write: bool,
    }

    impl FakeAutostart {
        fn new(enabled: bool) -> Self {
            Self {
                enabled: Cell::new(enabled),
                fail_read: false,
                fail_write: false,
                ignore_write: false,
            }
        }
    }

    impl AutostartBackend for FakeAutostart {
        fn is_enabled(&self) -> Result<bool, ()> {
            (!self.fail_read).then_some(self.enabled.get()).ok_or(())
        }

        fn enable(&self) -> Result<(), ()> {
            if self.fail_write {
                return Err(());
            }
            if !self.ignore_write {
                self.enabled.set(true);
            }
            Ok(())
        }

        fn disable(&self) -> Result<(), ()> {
            if self.fail_write {
                return Err(());
            }
            if !self.ignore_write {
                self.enabled.set(false);
            }
            Ok(())
        }
    }

    #[test]
    fn autostart_writes_are_verified_with_authoritative_state() {
        let backend = FakeAutostart::new(false);

        assert_eq!(set_autostart_enabled(&backend, true), Ok(true));
        assert_eq!(read_autostart_enabled(&backend), Ok(true));
        assert_eq!(set_autostart_enabled(&backend, false), Ok(false));
    }

    #[test]
    fn autostart_verification_rejects_an_ignored_write() {
        let mut backend = FakeAutostart::new(false);
        backend.ignore_write = true;

        assert_eq!(
            set_autostart_enabled(&backend, true),
            Err(LifecycleError::AutostartVerificationFailed)
        );
    }

    #[test]
    fn autostart_write_failure_preserves_the_previous_state() {
        let mut backend = FakeAutostart::new(false);
        backend.fail_write = true;

        assert_eq!(
            set_autostart_enabled(&backend, true),
            Err(LifecycleError::AutostartWriteFailed)
        );
        assert!(!backend.enabled.get());
    }

    #[test]
    fn autostart_read_failure_has_a_stable_error() {
        let mut backend = FakeAutostart::new(false);
        backend.fail_read = true;

        assert_eq!(
            read_autostart_enabled(&backend),
            Err(LifecycleError::AutostartReadFailed)
        );
    }
}
