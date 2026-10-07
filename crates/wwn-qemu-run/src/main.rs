//! iOS Mode B helper. The qemu framework image is MH_DYLIB (`posix_spawn` of
//! it is ENOEXEC). `fork`+`dlopen` in Wawona is SIGKILL'd once `qemu_main_loop`
//! starts. This binary is MH_EXECUTE: Wawona `posix_spawn`s it, it `dlopen`s
//! the framework, then calls `qemu_init` / `qemu_main_loop`.

use std::env;
use std::ffi::{CStr, CString, OsString};
use std::io::Write;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::process;

use libc::{c_char, c_int, dlerror, dlopen, dlsym, RTLD_NOW};

const PT_TRACE_ME: c_int = 0;
const PT_DETACH: c_int = 11;
const CS_OPS_STATUS: u32 = 0;
const CS_DEBUGGED: u32 = 0x1000_0000;

unsafe extern "C" {
    fn csops(
        pid: libc::pid_t,
        ops: u32,
        useraddr: *mut libc::c_void,
        usersize: usize,
    ) -> c_int;
    fn ptrace(
        request: c_int,
        pid: libc::pid_t,
        addr: *mut libc::c_void,
        data: c_int,
    ) -> c_int;
}

fn cs_debugged() -> bool {
    let mut flags: u32 = 0;
    unsafe {
        csops(
            0,
            CS_OPS_STATUS,
            (&mut flags as *mut u32).cast(),
            std::mem::size_of::<u32>(),
        ) == 0
            && (flags & CS_DEBUGGED) != 0
    }
}

fn log_line(msg: &str) {
    eprintln!("wwn-qemu-run: {msg}");
    if let Ok(path) = env::var("WWN_QEMU_LOG") {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(f, "wwn-qemu-run: {msg}");
        }
    }
}

/// Pojav / TrollStore self-enable. Parent briefly debugs a PT_TRACE_ME child
/// so this process gets CS_DEBUGGED before TCG executes generated pages.
fn try_self_enable_jit() {
    if env::args().nth(1).as_deref() == Some("--wwn-jit-child") {
        unsafe {
            ptrace(PT_TRACE_ME, 0, std::ptr::null_mut(), 0);
        }
        process::exit(0);
    }
    if cs_debugged() {
        log_line("cs_debugged=1 (already)");
        return;
    }
    let exe = match env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            log_line(&format!("self-jit: current_exe: {e}"));
            return;
        }
    };
    let exe_c = match CString::new(exe.to_string_lossy().as_bytes()) {
        Ok(s) => s,
        Err(_) => return,
    };
    let child_flag = CString::new("--wwn-jit-child").unwrap();
    let argv = [exe_c.as_ptr(), child_flag.as_ptr(), std::ptr::null()];
    let mut child: libc::pid_t = 0;
    let rc = unsafe {
        libc::posix_spawn(
            &mut child,
            exe_c.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            argv.as_ptr() as *const *mut c_char,
            environ_ptr(),
        )
    };
    if rc != 0 {
        log_line(&format!("self-jit: posix_spawn rc={rc}"));
        return;
    }
    let mut status: c_int = 0;
    unsafe {
        libc::waitpid(child, &mut status, libc::WUNTRACED);
        ptrace(PT_DETACH, child, std::ptr::null_mut(), 0);
        libc::kill(child, libc::SIGTERM);
        libc::waitpid(child, std::ptr::null_mut(), 0);
    }
    log_line(&format!(
        "self-jit done cs_debugged={}",
        u8::from(cs_debugged())
    ));
}

fn environ_ptr() -> *const *mut c_char {
    unsafe extern "C" {
        static mut environ: *mut *mut c_char;
    }
    unsafe { environ as *const *mut c_char }
}

type QemuInit =
    unsafe extern "C" fn(c_int, *const *const c_char, *const *const c_char) -> c_int;
type QemuFn = unsafe extern "C" fn();

fn dylib_path() -> CString {
    if let Ok(p) = env::var("WWN_QEMU_DYLIB") {
        return CString::new(p).expect("WWN_QEMU_DYLIB");
    }
    let rel = "qemu-aarch64-softmmu.framework/qemu-aarch64-softmmu";
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            let next_to_exe = dir.join(rel);
            if next_to_exe.is_file() {
                return CString::new(next_to_exe.to_string_lossy().as_bytes()).expect("path");
            }
            let frameworks = dir.join("Frameworks").join(rel);
            if frameworks.is_file() {
                return CString::new(frameworks.to_string_lossy().as_bytes()).expect("path");
            }
        }
    }
    CString::new(rel).expect("rel")
}

fn load_sym(handle: *mut libc::c_void, name: &CStr) -> *mut libc::c_void {
    let p = unsafe { dlsym(handle, name.as_ptr()) };
    if p.is_null() {
        let err = unsafe { dlerror() };
        let msg = if err.is_null() {
            "dlsym failed".into()
        } else {
            unsafe { CStr::from_ptr(err) }.to_string_lossy().into_owned()
        };
        eprintln!("wwn-qemu-run: {name:?}: {msg}");
        process::exit(1);
    }
    p
}

fn main() {
    try_self_enable_jit();

    let mut qemu_args: Vec<CString> = Vec::new();
    qemu_args.push(CString::new("qemu-aarch64-softmmu").expect("argv0"));
    for a in env::args().skip(1) {
        qemu_args.push(CString::new(a).unwrap_or_else(|_| CString::new("?").unwrap()));
    }
    let mut ptrs: Vec<*const c_char> = qemu_args.iter().map(|s| s.as_ptr()).collect();
    ptrs.push(std::ptr::null());

    let env_pairs: Vec<CString> = env::vars_os()
        .filter_map(|(k, v)| {
            let mut bytes = OsString::from(k).into_vec();
            bytes.push(b'=');
            bytes.extend(OsString::from(v).into_vec());
            CString::new(bytes).ok()
        })
        .collect();
    let mut env_ptrs: Vec<*const c_char> = env_pairs.iter().map(|s| s.as_ptr()).collect();
    env_ptrs.push(std::ptr::null());

    let dylib = dylib_path();
    eprintln!(
        "wwn-qemu-run: dlopen {} argc={}",
        dylib.to_string_lossy(),
        qemu_args.len()
    );
    let handle = unsafe { dlopen(dylib.as_ptr(), RTLD_NOW) };
    if handle.is_null() {
        let err = unsafe { dlerror() };
        let msg = if err.is_null() {
            "dlopen failed".into()
        } else {
            unsafe { CStr::from_ptr(err) }.to_string_lossy().into_owned()
        };
        eprintln!("wwn-qemu-run: {msg}");
        process::exit(1);
    }

    let qemu_init: QemuInit = unsafe {
        std::mem::transmute(load_sym(
            handle,
            CStr::from_bytes_with_nul(b"qemu_init\0").unwrap(),
        ))
    };
    let qemu_main_loop: QemuFn = unsafe {
        std::mem::transmute(load_sym(
            handle,
            CStr::from_bytes_with_nul(b"qemu_main_loop\0").unwrap(),
        ))
    };
    let qemu_cleanup: QemuFn = unsafe {
        std::mem::transmute(load_sym(
            handle,
            CStr::from_bytes_with_nul(b"qemu_cleanup\0").unwrap(),
        ))
    };

    if let Ok(path) = env::var("WWN_QEMU_LOG") {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path)
        {
            use std::io::Write;
            let _ = writeln!(
                f,
                "wwn-qemu-run pid={} dylib={} argc={}",
                process::id(),
                dylib.to_string_lossy(),
                qemu_args.len()
            );
        }
    }

    let cwd = env::var("WWN_QEMU_CHDIR")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.is_dir());
    if let Some(dir) = cwd {
        let _ = env::set_current_dir(dir);
    }

    let rc = unsafe {
        qemu_init(
            qemu_args.len() as c_int,
            ptrs.as_ptr(),
            env_ptrs.as_ptr(),
        )
    };
    eprintln!("wwn-qemu-run: qemu_init rc={rc}");
    if rc != 0 {
        process::exit(if rc > 0 { rc } else { 1 });
    }
    unsafe {
        qemu_main_loop();
        qemu_cleanup();
    }
}
