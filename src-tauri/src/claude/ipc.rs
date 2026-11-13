//! Server side of the local channel that hook clients (`island-hook`) write to.
//! One message per connection: the client writes a JSON document and closes.
//! Windows: a named pipe that rejects remote clients.
//! Linux: a Unix socket in the user's runtime dir (mode 0600).
//!
//! The server doubles as the single-instance guard: if it can't claim the
//! channel, another island is already running.

pub use platform::serve;

#[cfg(windows)]
mod platform {
    use std::{thread, time::Duration};
    use windows::{
        core::HSTRING,
        Win32::{
            Foundation::{CloseHandle, GetLastError, ERROR_PIPE_CONNECTED, HANDLE},
            Storage::FileSystem::{ReadFile, FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_INBOUND},
            System::Pipes::{
                ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
                PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
            },
        },
    };

    fn create(first: bool) -> Option<HANDLE> {
        let mut mode = PIPE_ACCESS_INBOUND;
        if first {
            mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        let h = unsafe {
            CreateNamedPipeW(
                &HSTRING::from(super::super::hookclient::PIPE_NAME),
                mode,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                0,
                64 * 1024,
                0,
                None,
            )
        };
        (!h.is_invalid()).then_some(h)
    }

    /// Claims the pipe. Returns false if another island already owns it.
    /// `on_message` runs on the server thread for each received document.
    pub fn serve(on_message: impl Fn(Vec<u8>) + Send + 'static) -> bool {
        let Some(first) = create(true) else {
            return false;
        };
        // HANDLE wraps a raw pointer (not Send); move it across as an integer.
        let first = first.0 as isize;
        thread::spawn(move || {
            let mut next = Some(HANDLE(first as *mut _));
            loop {
                let Some(h) = next.take().or_else(|| create(false)) else {
                    thread::sleep(Duration::from_secs(1));
                    continue;
                };
                let connected = unsafe { ConnectNamedPipe(h, None) }.is_ok()
                    || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
                let mut data = Vec::new();
                if connected {
                    let mut buf = [0u8; 8192];
                    loop {
                        let mut n = 0u32;
                        let ok = unsafe { ReadFile(h, Some(&mut buf), Some(&mut n), None) }.is_ok();
                        if !ok || n == 0 || data.len() > 256 * 1024 {
                            break;
                        }
                        data.extend_from_slice(&buf[..n as usize]);
                    }
                }
                unsafe {
                    let _ = DisconnectNamedPipe(h);
                    let _ = CloseHandle(h);
                }
                if !data.is_empty() {
                    on_message(data);
                }
            }
        });
        true
    }
}

#[cfg(unix)]
mod platform {
    use std::{
        io::Read,
        os::unix::{
            fs::PermissionsExt,
            net::{UnixListener, UnixStream},
        },
        thread,
    };

    pub fn serve(on_message: impl Fn(Vec<u8>) + Send + 'static) -> bool {
        let p = super::super::hookclient::socket_path();
        if UnixStream::connect(&p).is_ok() {
            return false; // another island is alive
        }
        let _ = std::fs::remove_file(&p); // stale socket from a crash
        let Ok(listener) = UnixListener::bind(&p) else {
            return false;
        };
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut data = Vec::new();
                let _ = stream.take(256 * 1024).read_to_end(&mut data);
                if !data.is_empty() {
                    on_message(data);
                }
            }
        });
        true
    }
}
