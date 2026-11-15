//! Global keyboard hook (Windows only) for keymapping.
//!
//! While mapping is on and the *Android Emulator window is in front*, a key that has a binding is
//! swallowed (so the emulator does not also receive it) and turned into a tap request.
//! All other keys, and every key while another program is in front, pass through untouched.
//! F10 (while the emulator is in front) pauses and resumes mapping.
use std::collections::HashMap;
use std::sync::mpsc::Sender;

/// Called with `true` (mapping resumed) or `false` (paused) when F10 is pressed.
pub type ToggleFn = Box<dyn Fn(bool) + Send + Sync>;

#[cfg(windows)]
mod imp {
    use super::{HashMap, Sender, ToggleFn};
    use std::collections::HashSet;
    use std::ptr::{null, null_mut};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Mutex};
    use std::thread::{self, JoinHandle};
    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW, GetWindowTextW,
        PostThreadMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HC_ACTION,
        KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT,
        WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    const VK_F10: u32 = 0x79;

    struct State {
        binds: HashMap<u32, (f32, f32)>,
        tx: Sender<(f32, f32)>,
        on_toggle: ToggleFn,
        down: HashSet<u32>, // keys currently held, so key-repeat does not tap again
    }

    static STATE: Mutex<Option<State>> = Mutex::new(None);
    static ACTIVE: AtomicBool = AtomicBool::new(true);

    pub struct HookHandle {
        thread_id: u32,
        join: Option<JoinHandle<()>>,
    }

    pub fn set_active(on: bool) {
        ACTIVE.store(on, Ordering::SeqCst);
    }

    pub fn start(
        binds: HashMap<u32, (f32, f32)>,
        tx: Sender<(f32, f32)>,
        on_toggle: ToggleFn,
    ) -> Result<HookHandle, String> {
        {
            let mut g = STATE.lock().map_err(|_| "Internal error (lock).".to_string())?;
            if g.is_some() {
                return Err("Keymapping is already running.".to_string());
            }
            *g = Some(State { binds, tx, on_toggle, down: HashSet::new() });
        }
        ACTIVE.store(true, Ordering::SeqCst);

        let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();
        let join = thread::spawn(move || unsafe {
            let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), GetModuleHandleW(null()), 0);
            if hook.is_null() {
                let _ = ready_tx.send(Err("Windows refused to install the keyboard hook.".to_string()));
                return;
            }
            let _ = ready_tx.send(Ok(GetCurrentThreadId()));

            // A low-level hook only works while its thread pumps messages.
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            UnhookWindowsHookEx(hook);
        });

        let result = match ready_rx.recv() {
            Ok(Ok(id)) => Ok(HookHandle { thread_id: id, join: Some(join) }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err("The keyboard hook thread failed to start.".to_string()),
        };
        if result.is_err() {
            if let Ok(mut g) = STATE.lock() {
                *g = None;
            }
        }
        result
    }

    impl Drop for HookHandle {
        fn drop(&mut self) {
            unsafe {
                PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0); // ends the message loop
            }
            if let Some(j) = self.join.take() {
                let _ = j.join();
            }
            if let Ok(mut g) = STATE.lock() {
                *g = None; // also drops the sender, which lets the injector finish
            }
        }
    }

    /// Is the Android Emulator window the one in front?
    unsafe fn emulator_in_front() -> bool {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return false;
        }
        let mut buf = [0u16; 128];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        n > 0 && String::from_utf16_lossy(&buf[..n as usize]).starts_with("Android Emulator")
    }

    unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 {
            let kb = &*(lparam as *const KBDLLHOOKSTRUCT);
            let msg = wparam as u32;
            let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
            let injected = kb.flags & LLKHF_INJECTED != 0;

            if (is_down || is_up) && !injected && emulator_in_front() {
                // try_lock: never make the keyboard wait for us
                if let Ok(mut guard) = STATE.try_lock() {
                    if let Some(st) = guard.as_mut() {
                        let vk = kb.vkCode;
                        if vk == VK_F10 {
                            if is_down && st.down.insert(vk) {
                                let now = !ACTIVE.load(Ordering::SeqCst);
                                ACTIVE.store(now, Ordering::SeqCst);
                                (st.on_toggle)(now);
                            } else if is_up {
                                st.down.remove(&vk);
                            }
                            return 1; // swallow
                        }
                        if ACTIVE.load(Ordering::SeqCst) {
                            if let Some(&(x, y)) = st.binds.get(&vk) {
                                if is_down {
                                    if st.down.insert(vk) {
                                        let _ = st.tx.send((x, y));
                                    }
                                } else {
                                    st.down.remove(&vk);
                                }
                                return 1; // swallow: the emulator should not see this key
                            }
                        }
                    }
                }
            }
        }
        CallNextHookEx(null_mut(), code, wparam, lparam)
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{HashMap, Sender, ToggleFn};

    pub struct HookHandle;

    pub fn set_active(_on: bool) {}

    pub fn start(
        _binds: HashMap<u32, (f32, f32)>,
        _tx: Sender<(f32, f32)>,
        _on_toggle: ToggleFn,
    ) -> Result<HookHandle, String> {
        Err("Keymapping only works on Windows.".to_string())
    }
}

pub use imp::{set_active, start, HookHandle};
