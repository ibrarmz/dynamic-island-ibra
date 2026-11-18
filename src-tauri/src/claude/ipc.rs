//! Server side of the local channel that hook clients (`island-hook`) write to.
//! One message per connection: the client writes a JSON document (ending in a
//! newline, or just closes), and the island may answer on the same connection
//! (permission requests wait for the user's decision; see `permission.rs`).
//! Windows: a named pipe that rejects remote clients.
//! Linux: a Unix socket in the user's runtime dir (mode 0600).
//!
//! The server doubles as the single-instance guard: if it can't claim the
//! channel, another island is already running.

pub use platform::{serve, Conn};

const MAX_MSG: usize = 256 * 1024;

#[cfg(windows)]
mod platform {
    use std::{thread, time::Duration};
    use windows::{
        core::HSTRING,
        Win32::{
            Foundation::{CloseHandle, GetLastError, ERROR_PIPE_CONNECTED, HANDLE},
            Storage::FileSystem::{FlushFileBuffers, ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX},
            System::Pipes::{
                ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PeekNamedPipe, PIPE_READMODE_BYTE,
                PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
            },
        },
    };

    /// One client connection. Closed when dropped.
    // HANDLE wraps a raw pointer (not Send); keep it as an integer.
    pub struct Conn(isize);

    impl Conn {
        fn h(&self) -> HANDLE {
            HANDLE(self.0 as *mut _)
        }

        /// The client is still there (hasn't been killed or given up).
        pub fn alive(&self) -> bool {
            unsafe { PeekNamedPipe(self.h(), None, 0, None, None, None) }.is_ok()
        }

        /// Answer and close. Waits until the client has read it.
        pub fn reply(self, data: &[u8]) {
            unsafe {
                let mut n = 0u32;
                if WriteFile(self.h(), Some(data), Some(&mut n), None).is_ok() {
                    let _ = FlushFileBuffers(self.h());
                }
            }
        }
    }

    impl Drop for Conn {
        fn drop(&mut self) {
            unsafe {
                let _ = DisconnectNamedPipe(self.h());
                let _ = CloseHandle(self.h());
            }
        }
    }

    fn create(first: bool) -> Option<HANDLE> {
        let mut mode = PIPE_ACCESS_DUPLEX;
        if first {
            mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        let h = unsafe {
            CreateNamedPipeW(
                &HSTRING::from(super::super::hookclient::PIPE_NAME),
                mode,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                4096,
                64 * 1024,
                0,
                None,
            )
        };
        (!h.is_invalid()).then_some(h)
    }

    /// Claims the pipe. Returns false if another island already owns it.
    /// `on_message` runs on the server thread for each received document.
    pub fn serve(on_message: impl Fn(Vec<u8>, Conn) + Send + 'static) -> bool {
        let Some(first) = create(true) else {
            return false;
        };
        let first = first.0 as isize;
        thread::spawn(move || {
            let mut next = Some(HANDLE(first as *mut _));
            loop {
                let Some(h) = next.take().or_else(|| create(false)) else {
                    thread::sleep(Duration::from_secs(1));
                    continue;
                };
                let conn = Conn(h.0 as isize);
                let connected = unsafe { ConnectNamedPipe(h, None) }.is_ok()
                    || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
                let mut data = Vec::new();
                if connected {
                    let mut buf = [0u8; 8192];
                    loop {
                        let mut n = 0u32;
                        let ok = unsafe { ReadFile(h, Some(&mut buf), Some(&mut n), None) }.is_ok();
                        if !ok || n == 0 {
                            break;
                        }
                        data.extend_from_slice(&buf[..n as usize]);
                        // A newline ends the message; older clients just close.
                        if data.contains(&b'\n') || data.len() > super::MAX_MSG {
                            break;
                        }
                    }
                }
                if !data.is_empty() {
                    on_message(data, conn);
                }
            }
        });
        true
    }
}

#[cfg(unix)]
mod platform {
    use std::{
        io::{Read, Write},
        os::unix::{
            fs::PermissionsExt,
            net::{UnixListener, UnixStream},
        },
        thread,
    };

    /// One client connection. Closed when dropped.
    pub struct Conn(UnixStream);

    impl Conn {
        /// The client is still there (hasn't been killed or given up).
        pub fn alive(&self) -> bool {
            if self.0.set_nonblocking(true).is_err() {
                return false;
            }
            // The client sends nothing after its message, so reading consumes
            // nothing: it is either EOF (gone) or WouldBlock (waiting).
            let r = (&self.0).read(&mut [0u8; 1]);
            let _ = self.0.set_nonblocking(false);
            match r {
                Ok(0) => false,
                Ok(_) => true,
                Err(e) => e.kind() == std::io::ErrorKind::WouldBlock,
            }
        }

        /// Answer and close.
        pub fn reply(mut self, data: &[u8]) {
            let _ = self.0.write_all(data);
        }
    }

    pub fn serve(on_message: impl Fn(Vec<u8>, Conn) + Send + 'static) -> bool {
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
            for mut stream in listener.incoming().flatten() {
                let mut data = Vec::new();
                let mut buf = [0u8; 8192];
                loop {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => data.extend_from_slice(&buf[..n]),
                    }
                    // A newline ends the message; older clients just close.
                    if data.contains(&b'\n') || data.len() > super::MAX_MSG {
                        break;
                    }
                }
                if !data.is_empty() {
                    on_message(data, Conn(stream));
                }
            }
        });
        true
    }
}
