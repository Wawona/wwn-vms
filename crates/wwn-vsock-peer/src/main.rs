//! Userspace virtio-vsock peer for iOS Mode B.
//!
//! QEMU `vhost-user-vsock-pci` needs a vhost-user backend on a unix socket.
//! Waypipe must not connect to that chardev. This process:
//! 1. Listens on `--socket` (qemu chardev)
//! 2. Answers the vhost-user handshake so qemu does not abort()
//! 3. Exposes host connections at `--uds-path` / `<port>` for waypipe

use std::env;
use std::fs;
use std::io::{Read, Write};
use std::os::fd::{FromRawFd, IntoRawFd, RawFd};
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process;
use std::thread;
use std::time::Duration;

const VHOST_USER_GET_FEATURES: u32 = 1;
const VHOST_USER_SET_FEATURES: u32 = 2;
const VHOST_USER_SET_OWNER: u32 = 3;
const VHOST_USER_RESET_OWNER: u32 = 4;
const VHOST_USER_SET_MEM_TABLE: u32 = 5;
const VHOST_USER_SET_VRING_NUM: u32 = 8;
const VHOST_USER_SET_VRING_ADDR: u32 = 9;
const VHOST_USER_SET_VRING_BASE: u32 = 10;
const VHOST_USER_GET_VRING_BASE: u32 = 11;
const VHOST_USER_SET_VRING_KICK: u32 = 12;
const VHOST_USER_SET_VRING_CALL: u32 = 13;
const VHOST_USER_SET_VRING_ERR: u32 = 14;
const VHOST_USER_GET_PROTOCOL_FEATURES: u32 = 15;
const VHOST_USER_SET_PROTOCOL_FEATURES: u32 = 16;
const VHOST_USER_GET_QUEUE_NUM: u32 = 17;
const VHOST_USER_SET_VRING_ENABLE: u32 = 18;
const VHOST_USER_GET_CONFIG: u32 = 24;
const VHOST_USER_SET_CONFIG: u32 = 25;

const VHOST_USER_VERSION: u32 = 0x1;
const VHOST_USER_REPLY_MASK: u32 = 1 << 2;
const VHOST_USER_NEED_REPLY: u32 = 1 << 3;

const VIRTIO_F_VERSION_1: u64 = 1 << 32;
const VHOST_USER_F_PROTOCOL_FEATURES: u64 = 1 << 30;
const VHOST_USER_PROTOCOL_F_MQ: u64 = 1 << 0;
const VHOST_USER_PROTOCOL_F_REPLY_ACK: u64 = 1 << 3;
const VHOST_USER_PROTOCOL_F_CONFIG: u64 = 1 << 9;

#[repr(C, packed)]
struct MsgHeader {
    request: u32,
    flags: u32,
    size: u32,
}

fn log_line(msg: &str) {
    eprintln!("wwn-vsock-peer: {msg}");
    if let Ok(path) = env::var("WWN_VSOCK_LOG") {
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "wwn-vsock-peer: {msg}");
        }
    }
}

fn parse_args() -> (PathBuf, PathBuf, u64) {
    let mut socket = None;
    let mut uds = None;
    let mut cid: u64 = 3;
    let mut args = env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--socket" => socket = args.next().map(PathBuf::from),
            "--uds-path" => uds = args.next().map(PathBuf::from),
            "--guest-cid" => {
                cid = args
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(3);
            }
            "-h" | "--help" => {
                eprintln!(
                    "usage: wwn-vsock-peer --socket VHOST.sock --uds-path UDS [--guest-cid N]"
                );
                process::exit(0);
            }
            other => {
                eprintln!("unknown arg {other}");
                process::exit(2);
            }
        }
    }
    let socket = socket.expect("--socket");
    let uds = uds.expect("--uds-path");
    (socket, uds, cid)
}

fn write_reply(stream: &mut UnixStream, request: u32, payload: &[u8]) -> std::io::Result<()> {
    let hdr = MsgHeader {
        request,
        flags: VHOST_USER_VERSION | VHOST_USER_REPLY_MASK,
        size: payload.len() as u32,
    };
    let hdr_bytes = unsafe {
        std::slice::from_raw_parts(
            (&hdr as *const MsgHeader).cast::<u8>(),
            std::mem::size_of::<MsgHeader>(),
        )
    };
    stream.write_all(hdr_bytes)?;
    if !payload.is_empty() {
        stream.write_all(payload)?;
    }
    stream.flush()
}

fn recv_fds(stream: &UnixStream, expected: usize) -> Vec<RawFd> {
    if expected == 0 {
        return Vec::new();
    }
    let fd = stream.as_raw_fd();
    let mut fds = vec![-1; expected];
    let mut iov_buf = [0u8; 1];
    let mut iov = libc::iovec {
        iov_base: iov_buf.as_mut_ptr().cast(),
        iov_len: 1,
    };
    let cmsg_space = unsafe { libc::CMSG_SPACE((expected * std::mem::size_of::<RawFd>()) as u32) };
    let mut cmsg_buf = vec![0u8; cmsg_space as usize];
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = cmsg_buf.as_mut_ptr().cast();
    msg.msg_controllen = cmsg_buf.len() as _;
    let n = unsafe { libc::recvmsg(fd, &mut msg, 0) };
    if n < 0 {
        return Vec::new();
    }
    unsafe {
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        if !cmsg.is_null() && (*cmsg).cmsg_type == libc::SCM_RIGHTS {
            let data = libc::CMSG_DATA(cmsg) as *const RawFd;
            let count = ((*cmsg).cmsg_len as usize - libc::CMSG_LEN(0) as usize)
                / std::mem::size_of::<RawFd>();
            fds.truncate(count.min(expected));
            for (i, slot) in fds.iter_mut().enumerate() {
                *slot = *data.add(i);
            }
        }
    }
    fds.into_iter().filter(|fd| *fd >= 0).collect()
}

fn close_fds(fds: &[RawFd]) {
    for fd in fds {
        unsafe {
            libc::close(*fd);
        }
    }
}

fn handle_vhost(mut stream: UnixStream, guest_cid: u64) {
    log_line("qemu connected");
    let mut hdr_buf = [0u8; std::mem::size_of::<MsgHeader>()];
    loop {
        if stream.read_exact(&mut hdr_buf).is_err() {
            log_line("qemu disconnected");
            return;
        }
        let hdr: MsgHeader = unsafe { std::ptr::read_unaligned(hdr_buf.as_ptr().cast()) };
        let request = hdr.request;
        let flags = hdr.flags;
        let size = hdr.size as usize;
        let mut payload = vec![0u8; size];
        if size > 0 && stream.read_exact(&mut payload).is_err() {
            log_line("short payload");
            return;
        }
        let need_reply = (flags & VHOST_USER_NEED_REPLY) != 0;
        match request {
            VHOST_USER_GET_FEATURES => {
                let feats = VIRTIO_F_VERSION_1 | VHOST_USER_F_PROTOCOL_FEATURES;
                let _ = write_reply(&mut stream, request, &feats.to_le_bytes());
            }
            VHOST_USER_GET_PROTOCOL_FEATURES => {
                let feats = VHOST_USER_PROTOCOL_F_MQ
                    | VHOST_USER_PROTOCOL_F_REPLY_ACK
                    | VHOST_USER_PROTOCOL_F_CONFIG;
                let _ = write_reply(&mut stream, request, &feats.to_le_bytes());
            }
            VHOST_USER_GET_QUEUE_NUM => {
                let n: u64 = 3;
                let _ = write_reply(&mut stream, request, &n.to_le_bytes());
            }
            VHOST_USER_GET_VRING_BASE => {
                let mut out = [0u8; 8];
                if payload.len() >= 4 {
                    out[..4].copy_from_slice(&payload[..4]);
                }
                let _ = write_reply(&mut stream, request, &out);
            }
            VHOST_USER_GET_CONFIG => {
                let mut cfg = vec![0u8; 8.min(payload.len().max(8))];
                cfg[..8].copy_from_slice(&guest_cid.to_le_bytes());
                let _ = write_reply(&mut stream, request, &cfg);
            }
            VHOST_USER_SET_MEM_TABLE => {
                let nregions = payload.first().copied().unwrap_or(0) as usize;
                let fds = recv_fds(&stream, nregions);
                close_fds(&fds);
                if need_reply {
                    let _ = write_reply(&mut stream, request, &0u64.to_le_bytes());
                }
            }
            VHOST_USER_SET_VRING_KICK | VHOST_USER_SET_VRING_CALL | VHOST_USER_SET_VRING_ERR => {
                if payload.first().is_some_and(|b| *b & 0x1 == 0) {
                    let fds = recv_fds(&stream, 1);
                    close_fds(&fds);
                }
                if need_reply {
                    let _ = write_reply(&mut stream, request, &0u64.to_le_bytes());
                }
            }
            VHOST_USER_SET_FEATURES
            | VHOST_USER_SET_OWNER
            | VHOST_USER_RESET_OWNER
            | VHOST_USER_SET_PROTOCOL_FEATURES
            | VHOST_USER_SET_VRING_NUM
            | VHOST_USER_SET_VRING_ADDR
            | VHOST_USER_SET_VRING_BASE
            | VHOST_USER_SET_VRING_ENABLE
            | VHOST_USER_SET_CONFIG => {
                if need_reply {
                    let _ = write_reply(&mut stream, request, &0u64.to_le_bytes());
                }
            }
            other => {
                log_line(&format!("unhandled request {other}"));
                if need_reply {
                    let _ = write_reply(&mut stream, request, &0u64.to_le_bytes());
                }
            }
        }
    }
}

fn accept_host_uds(uds: &Path) {
    let _ = fs::remove_file(uds);
    if let Some(parent) = uds.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let listener = match UnixListener::bind(uds) {
        Ok(l) => l,
        Err(e) => {
            log_line(&format!("uds bind {uds:?}: {e}"));
            return;
        }
    };
    log_line(&format!("waypipe uds listening on {}", uds.display()));
    let _ = listener.set_nonblocking(true);
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                log_line("waypipe client connected (peer has no live guest virtqueue yet)");
                let _ = stream.write_all(b"");
                drop(stream);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(200));
            }
            Err(e) => {
                log_line(&format!("uds accept: {e}"));
                thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

fn main() {
    let (socket, uds, cid) = parse_args();
    let _ = fs::remove_file(&socket);
    if let Some(parent) = socket.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let listener = UnixListener::bind(&socket).unwrap_or_else(|e| {
        log_line(&format!("vhost bind {}: {e}", socket.display()));
        process::exit(1);
    });
    log_line(&format!(
        "listening vhost={} uds={} cid={cid}",
        socket.display(),
        uds.display()
    ));
    let uds_clone = uds.clone();
    thread::spawn(move || accept_host_uds(&uds_clone));
    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                thread::spawn(move || handle_vhost(stream, cid));
            }
            Err(e) => log_line(&format!("vhost accept: {e}")),
        }
    }
}

#[allow(dead_code)]
fn _from_raw_stream(fd: RawFd) -> UnixStream {
    unsafe { UnixStream::from_raw_fd(fd) }
}

#[allow(dead_code)]
fn _into_raw(stream: UnixStream) -> RawFd {
    stream.into_raw_fd()
}
