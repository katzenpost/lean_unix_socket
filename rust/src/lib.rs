// SPDX-FileCopyrightText: Copyright (C) 2026 David Stainton
// SPDX-License-Identifier: AGPL-3.0-only

//! Unix domain sockets for Lean: connect, send, receive and shut down, as functions with Lean's
//! calling convention. `UnixSocket.lean` declares them. The socket only moves bytes; framing is
//! the caller's.

#![allow(clippy::missing_safety_doc)]

use lean_sys::*;
use std::ffi::c_void;
use std::io::{self, Read};
use std::net::Shutdown;
#[cfg(target_os = "android")]
use std::os::android::net::SocketAddrExt;
use std::os::fd::AsRawFd;
#[cfg(target_os = "linux")]
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
use std::sync::OnceLock;
use std::thread;

/// The address `path` names: the abstract name after a leading `@`, otherwise a file.
pub fn address(path: &str) -> io::Result<SocketAddr> {
    match path.strip_prefix('@') {
        Some(name) => abstract_address(name),
        None => SocketAddr::from_pathname(path),
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn abstract_address(name: &str) -> io::Result<SocketAddr> {
    SocketAddr::from_abstract_name(name)
}

/// Abstract names are a Linux feature; elsewhere a socket needs a file path.
#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn abstract_address(_name: &str) -> io::Result<SocketAddr> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "abstract socket names need Linux; use a file path",
    ))
}

/// A connected stream and the address it was opened with, for error messages.
pub struct Socket {
    stream: UnixStream,
    path: String,
}

impl Socket {
    pub fn connect(path: &str) -> io::Result<Socket> {
        let stream = UnixStream::connect_addr(&address(path)?)?;
        Ok(Socket {
            stream,
            path: path.into(),
        })
    }

    pub fn pair() -> io::Result<(Socket, Socket)> {
        let (a, b) = UnixStream::pair()?;
        let s = |stream| Socket {
            stream,
            path: String::new(),
        };
        Ok((s(a), s(b)))
    }

    /// Send all of `buf`. A peer that has gone is an error, not a SIGPIPE.
    pub fn send(&self, mut buf: &[u8]) -> io::Result<()> {
        while !buf.is_empty() {
            let n = unsafe {
                libc::send(
                    self.stream.as_raw_fd(),
                    buf.as_ptr() as *const c_void,
                    buf.len(),
                    libc::MSG_NOSIGNAL,
                )
            };
            if n < 0 {
                let e = io::Error::last_os_error();
                if e.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e);
            }
            buf = &buf[n as usize..];
        }
        Ok(())
    }

    /// Wait for up to `max` bytes; none once the peer has shut down its side.
    pub fn recv(&self, max: usize) -> io::Result<Option<Vec<u8>>> {
        if max == 0 {
            return Ok(Some(Vec::new()));
        }
        let mut buf = vec![0; max];
        loop {
            match (&self.stream).read(&mut buf) {
                Ok(0) => return Ok(None),
                Ok(n) => {
                    buf.truncate(n);
                    return Ok(Some(buf));
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
    }

    /// Shut down both directions, waking a `recv` blocked in another thread. The descriptor
    /// stays open until the socket is dropped, so it cannot be reused under that thread.
    pub fn shutdown(&self) -> io::Result<()> {
        match self.stream.shutdown(Shutdown::Both) {
            Err(e) if e.kind() == io::ErrorKind::NotConnected => Ok(()),
            r => r,
        }
    }
}

/// Listen on `path`, then in a thread accept one connection and send back what it receives
/// until the peer shuts down.
pub fn echo_once(path: &str) -> io::Result<()> {
    let listener = UnixListener::bind_addr(&address(path)?)?;
    thread::spawn(move || -> io::Result<()> {
        let (s, _) = listener.accept()?;
        io::copy(&mut &s, &mut &s).map(drop)
    });
    Ok(())
}

unsafe fn string(s: &str) -> *mut lean_object {
    lean_mk_string_from_bytes(s.as_ptr(), s.len())
}

unsafe fn str_of<'a>(o: b_lean_obj_arg) -> &'a str {
    let bytes = std::slice::from_raw_parts(lean_string_cstr(o), lean_string_size(o) - 1);
    std::str::from_utf8_unchecked(bytes)
}

unsafe fn byte_array(b: &[u8]) -> *mut lean_object {
    let a = lean_alloc_sarray(1, b.len(), b.len());
    std::ptr::copy_nonoverlapping(b.as_ptr(), lean_sarray_cptr(a), b.len());
    a
}

unsafe fn some(v: *mut lean_object) -> *mut lean_object {
    let o = lean_alloc_ctor(1, 1, 0);
    lean_ctor_set(o, 0, v);
    o
}

unsafe fn pair(a: *mut lean_object, b: *mut lean_object) -> *mut lean_object {
    let o = lean_alloc_ctor(0, 2, 0);
    lean_ctor_set(o, 0, a);
    lean_ctor_set(o, 1, b);
    o
}

/// An OS error as Lean's `IO.Error` for it, naming `path`; others as a user error.
unsafe fn io_error(e: io::Error, path: &str) -> *mut lean_object {
    match e.raw_os_error() {
        Some(n) => {
            let f = string(path);
            let r = lean_decode_io_error(n, f);
            lean_dec_ref(f);
            r
        }
        None => lean_mk_io_user_error(string(&e.to_string())),
    }
}

unsafe fn io<T>(
    r: io::Result<T>,
    path: &str,
    ok: impl FnOnce(T) -> *mut lean_object,
) -> *mut lean_object {
    match r {
        Ok(v) => lean_io_result_mk_ok(ok(v)),
        Err(e) => lean_io_result_mk_error(io_error(e, path)),
    }
}

unsafe extern "C" fn finalize(p: *mut c_void) {
    drop(Box::from_raw(p as *mut Socket));
}

unsafe extern "C" fn foreach(_: *mut c_void, _: b_lean_obj_arg) {}

struct Class(*mut lean_external_class);
unsafe impl Send for Class {}
unsafe impl Sync for Class {}

static CLASS: OnceLock<Class> = OnceLock::new();

unsafe fn external(s: Socket) -> *mut lean_object {
    let cls =
        CLASS.get_or_init(|| Class(lean_register_external_class(Some(finalize), Some(foreach))));
    lean_alloc_external(cls.0, Box::into_raw(Box::new(s)) as *mut c_void)
}

unsafe fn socket<'a>(o: b_lean_obj_arg) -> &'a Socket {
    &*(lean_get_external_data(o) as *const Socket)
}

#[no_mangle]
pub unsafe extern "C" fn lus_connect(path: b_lean_obj_arg) -> *mut lean_object {
    let path = str_of(path);
    io(Socket::connect(path), path, |s| external(s))
}

#[no_mangle]
pub unsafe extern "C" fn lus_pair() -> *mut lean_object {
    io(Socket::pair(), "", |(a, b)| pair(external(a), external(b)))
}

#[no_mangle]
pub unsafe extern "C" fn lus_send(s: b_lean_obj_arg, data: b_lean_obj_arg) -> *mut lean_object {
    let s = socket(s);
    let data = std::slice::from_raw_parts(lean_sarray_cptr(data), lean_sarray_size(data));
    io(s.send(data), &s.path, |()| lean_box(0))
}

#[no_mangle]
pub unsafe extern "C" fn lus_recv(s: b_lean_obj_arg, max: usize) -> *mut lean_object {
    let s = socket(s);
    io(s.recv(max), &s.path, |r| match r {
        Some(b) => some(byte_array(&b)),
        None => lean_box(0),
    })
}

#[no_mangle]
pub unsafe extern "C" fn lus_shutdown(s: b_lean_obj_arg) -> *mut lean_object {
    let s = socket(s);
    io(s.shutdown(), &s.path, |()| lean_box(0))
}

#[no_mangle]
pub unsafe extern "C" fn lus_echo_once(path: b_lean_obj_arg) -> *mut lean_object {
    let path = str_of(path);
    io(echo_once(path), path, |()| lean_box(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn name(test: &str) -> String {
        format!("@lean_unix_socket/{test}/{}", std::process::id())
    }

    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    fn name(test: &str) -> String {
        let dir = std::env::temp_dir();
        format!("{}/lus-{test}-{}.sock", dir.display(), std::process::id())
    }

    #[test]
    fn addresses() {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            let a = address("@kpclientd").unwrap();
            assert_eq!(a.as_abstract_name(), Some(&b"kpclientd"[..]));
            assert_eq!(a.as_pathname(), None);
        }
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        assert_eq!(
            address("@kpclientd").unwrap_err().kind(),
            io::ErrorKind::Unsupported
        );
        let f = address("/run/kpclientd.sock").unwrap();
        assert_eq!(
            f.as_pathname().unwrap().to_str(),
            Some("/run/kpclientd.sock")
        );
        assert!(address(&"x".repeat(200)).is_err());
    }

    #[test]
    fn pair_round_trip() {
        let (a, b) = Socket::pair().unwrap();
        let msg: Vec<u8> = (0..1 << 20).map(|i| i as u8).collect();
        let sender = thread::spawn(move || {
            a.send(&msg).unwrap();
            a.shutdown().unwrap();
            msg
        });
        let mut got = Vec::new();
        while let Some(chunk) = b.recv(65536).unwrap() {
            got.extend(chunk);
        }
        assert_eq!(got, sender.join().unwrap());
    }

    #[test]
    fn shutdown_wakes_recv() {
        let (a, _b) = Socket::pair().unwrap();
        let a = std::sync::Arc::new(a);
        let reader = {
            let a = a.clone();
            thread::spawn(move || a.recv(16).unwrap())
        };
        thread::sleep(std::time::Duration::from_millis(50));
        a.shutdown().unwrap();
        assert_eq!(reader.join().unwrap(), None);
    }

    #[test]
    fn send_to_closed_peer_is_an_error() {
        let (a, b) = Socket::pair().unwrap();
        drop(b);
        assert_eq!(a.send(b"x").unwrap_err().kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn connect_abstract() {
        let path = name("connect_abstract");
        echo_once(&path).unwrap();
        let s = Socket::connect(&path).unwrap();
        s.send(b"hello").unwrap();
        assert_eq!(s.recv(16).unwrap().as_deref(), Some(&b"hello"[..]));
    }

    #[test]
    fn connect_refused() {
        let path = name("connect_refused");
        // A file socket refuses only once its file exists: leave one with nobody listening.
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        drop(UnixListener::bind(&path).unwrap());
        let e = Socket::connect(&path).err().unwrap();
        assert_eq!(e.kind(), io::ErrorKind::ConnectionRefused);
    }
}
