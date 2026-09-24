//! Everything that touches Win32 lives here, behind traits the engine can fake.
//!
//! Game detection is limited to the process list, `SHQueryUserNotificationState`
//! and foreground-window geometry. No injection and no hooking, ever — that is
//! what keeps anti-cheat out of the picture.

use std::mem::size_of;
use std::time::Duration;

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::Shell::{
    SHQueryUserNotificationState, QUERY_USER_NOTIFICATION_STATE, QUNS_BUSY, QUNS_PRESENTATION_MODE,
    QUNS_RUNNING_D3D_FULL_SCREEN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId, SetWindowPos, HWND_TOPMOST,
    SWP_NOACTIVATE, SWP_SHOWWINDOW,
};

use crate::engine::Context;

/// Where the observed context comes from.
///
/// A trait because the caller should not care, and because a fake one is the
/// obvious way to drive the app without a real game running. The clock and the
/// idle reading need no equivalent: the engine takes both as plain arguments,
/// which is simpler and is what the tests already exercise.
pub trait ContextSource {
    fn current(&self) -> Context;
}

pub struct WindowsContext {
    /// Executable names from `[detection].games`, lowercased once up front.
    games: Vec<String>,
}

impl WindowsContext {
    pub fn new(games: &[String]) -> Self {
        Self {
            games: games.iter().map(|name| name.to_ascii_lowercase()).collect(),
        }
    }
}

impl ContextSource for WindowsContext {
    fn current(&self) -> Context {
        detect_context(&self.games)
    }
}

/// How long since the last keyboard or mouse input.
pub fn idle_duration() -> Duration {
    let mut info = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };

    // SAFETY: `info` is a correctly sized LASTINPUTINFO living on our stack.
    let reported = unsafe { GetLastInputInfo(&mut info) };
    if !reported.as_bool() {
        return Duration::ZERO;
    }

    // SAFETY: no arguments, no state.
    let now = unsafe { GetTickCount() };

    // Both counters are 32-bit milliseconds and wrap after about 49 days, so
    // the subtraction has to wrap with them.
    Duration::from_millis(now.wrapping_sub(info.dwTime) as u64)
}

/// What is in front, as far as it concerns us.
///
/// Three signals, because no single one covers the ground. The notification
/// state knows about exclusive fullscreen and presentations. Window geometry
/// catches borderless fullscreen, which sets no flag of its own and is what
/// most modern games actually run in. The process list is the manual override
/// for anything the first two miss.
pub fn detect_context(games: &[String]) -> Context {
    let state = notification_state();

    if state == Some(QUNS_PRESENTATION_MODE) {
        return Context::Presentation;
    }

    // Exclusive fullscreen Direct3D. A window put over this does not draw over
    // the game, it alt-tabs out of it, so this has to stay distinguishable
    // from the borderless case.
    if state == Some(QUNS_RUNNING_D3D_FULL_SCREEN) {
        return Context::ExclusiveGame;
    }

    // QUNS_BUSY is Windows saying a full-screen app is running and it has
    // stopped showing notifications. Worth trusting on its own, because it is
    // also the reason a toast would go unseen.
    if state == Some(QUNS_BUSY) {
        return Context::Game;
    }

    let listed = foreground_process_name()
        .map(|name| games.contains(&name))
        .unwrap_or(false);

    // Covering the monitor exactly is the borderless signature: a merely
    // maximised window stops at the work area and leaves the taskbar showing.
    if listed || foreground_covers_monitor() {
        return Context::Game;
    }

    Context::Free
}

fn notification_state() -> Option<QUERY_USER_NOTIFICATION_STATE> {
    // SAFETY: no arguments; the call only reads shell state.
    unsafe { SHQueryUserNotificationState().ok() }
}

/// Lowercased file name of whatever owns the foreground window.
fn foreground_process_name() -> Option<String> {
    // SAFETY: the window handle is checked before use, the process handle is
    // closed on every path, and the buffer length is passed by size.
    unsafe {
        let window = GetForegroundWindow();
        if window.is_invalid() {
            return None;
        }

        let mut pid = 0u32;
        GetWindowThreadProcessId(window, Some(&mut pid));
        if pid == 0 {
            return None;
        }

        // Limited rights on purpose: we only ever want the name.
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;

        let mut buffer = [0u16; 260];
        let mut length = buffer.len() as u32;
        let queried = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        );
        let _ = CloseHandle(process);
        queried.ok()?;

        let path = String::from_utf16_lossy(&buffer[..length as usize]);
        path.rsplit(['\\', '/'])
            .next()
            .map(|name| name.to_ascii_lowercase())
    }
}

/// Whether the foreground window covers its whole monitor.
///
/// Tells borderless fullscreen apart from a merely large window, which decides
/// whether an overlay stands a chance of being seen.
pub fn foreground_covers_monitor() -> bool {
    // SAFETY: every handle is checked, and both structs are stack-allocated
    // with their size fields set where the API requires it.
    unsafe {
        let window = GetForegroundWindow();
        if window.is_invalid() {
            return false;
        }

        let mut bounds = RECT::default();
        if GetWindowRect(window, &mut bounds).is_err() {
            return false;
        }

        let monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return false;
        }

        bounds.left <= info.rcMonitor.left
            && bounds.top <= info.rcMonitor.top
            && bounds.right >= info.rcMonitor.right
            && bounds.bottom >= info.rcMonitor.bottom
    }
}

/// The primary monitor's work area: the screen minus the taskbar.
///
/// Returned as `(left, top, right, bottom)` in physical pixels. The monitor's
/// own rectangle includes whatever the taskbar covers, so anything positioned
/// against the bottom edge using that ends up underneath it.
pub fn primary_work_area() -> Option<(i32, i32, i32, i32)> {
    // SAFETY: MONITOR_DEFAULTTOPRIMARY always yields a valid monitor, and the
    // struct is stack-allocated with its size field set as the API requires.
    unsafe {
        let monitor = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return None;
        }
        let work = info.rcWork;
        Some((work.left, work.top, work.right, work.bottom))
    }
}

/// Puts a window over everything, the taskbar included, without focusing it.
///
/// Being marked always-on-top is not enough on its own: the taskbar is topmost
/// too, and among topmost windows the most recently positioned one wins. So the
/// overlay has to re-assert its place every time it is shown, or the taskbar
/// stays clickable straight through a break.
///
/// `SWP_NOACTIVATE` keeps the focus where it was, which matters when the thing
/// underneath is a game.
pub fn raise_above_everything(hwnd: isize, x: i32, y: i32, width: i32, height: i32) {
    if hwnd == 0 {
        return;
    }

    // SAFETY: the handle comes straight from the window that owns it, and is
    // only used for the duration of this call.
    unsafe {
        let _ = SetWindowPos(
            HWND(hwnd as *mut std::ffi::c_void),
            Some(HWND_TOPMOST),
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

/// Hard mode. Opt-in, and the last rung of the ladder.
pub fn lock_workstation() {
    // SAFETY: no arguments, no state. Failing just means the screen stayed on.
    let _ = unsafe { LockWorkStation() };
}
