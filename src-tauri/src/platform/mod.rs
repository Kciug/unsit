//! Everything that touches Win32 lives here, behind traits the engine can fake.
//!
//! Game detection is limited to the process list, `SHQueryUserNotificationState`
//! and foreground-window geometry. No injection and no hooking, ever — that is
//! what keeps anti-cheat out of the picture.

use std::mem::size_of;
use std::time::{Duration, SystemTime};

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::Shell::{
    SHQueryUserNotificationState, QUERY_USER_NOTIFICATION_STATE, QUNS_PRESENTATION_MODE,
    QUNS_RUNNING_D3D_FULL_SCREEN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId,
};

use crate::engine::{Context, Timestamp};

pub trait Clock {
    fn now(&self) -> Timestamp;
}

pub trait IdleSource {
    /// Time since the last input, via `GetLastInputInfo`.
    ///
    /// Known gap: gamepads are invisible to it, so controller input does not
    /// register as activity.
    fn idle_for(&self) -> Duration;
}

pub trait ContextSource {
    fn current(&self) -> Context;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        SystemTime::now()
    }
}

pub struct WindowsIdle;

impl IdleSource for WindowsIdle {
    fn idle_for(&self) -> Duration {
        idle_duration()
    }
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

pub fn detect_context(games: &[String]) -> Context {
    if let Some(state) = notification_state() {
        if state == QUNS_PRESENTATION_MODE {
            return Context::Presentation;
        }
        // Exclusive fullscreen Direct3D: a game, and one we cannot draw over.
        if state == QUNS_RUNNING_D3D_FULL_SCREEN {
            return Context::Game;
        }
    }

    // Borderless fullscreen never sets the D3D flag, so fall back to asking
    // what is actually in front.
    let listed = foreground_process_name()
        .map(|name| games.contains(&name))
        .unwrap_or(false);

    if listed {
        Context::Game
    } else {
        Context::Free
    }
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

/// Hard mode. Opt-in, and the last rung of the ladder.
pub fn lock_workstation() {
    // SAFETY: no arguments, no state. Failing just means the screen stayed on.
    let _ = unsafe { LockWorkStation() };
}
