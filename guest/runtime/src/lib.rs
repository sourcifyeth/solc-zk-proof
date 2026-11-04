//! Runtime for GCC-built C and C++ code (newlib, libstdc++) inside the SP1 zkVM.
//!
//! The zkVM has no operating system. This crate gives newlib and libstdc++ what they expect
//! from one: heap memory (from the Rust allocator), stdout and stderr (SP1 debug output),
//! static constructors, exception unwind tables, and atomics (the target has no A extension).
#![allow(clippy::missing_safety_doc, non_upper_case_globals)]

use core::ffi::{c_char, c_int, c_long, c_void};
use core::ptr::{self, null_mut};
use std::alloc::{alloc, alloc_zeroed, dealloc, Layout};

/// Runs the C++ static constructors and registers the unwind tables. Call once, first.
pub fn init() {
    extern "C" {
        static __preinit_array_start: u8;
        static __preinit_array_end: u8;
        static __init_array_start: u8;
        static __init_array_end: u8;
        static __eh_frame_start: u8;
        fn __register_frame_info(begin: *const c_void, object: *mut c_void);
    }
    // libgcc's `struct object` is 6 words; it must live for the whole run.
    static mut EH_OBJECT: [usize; 16] = [0; 16];
    unsafe {
        __register_frame_info(ptr::addr_of!(__eh_frame_start).cast(), ptr::addr_of_mut!(EH_OBJECT).cast());
        run_array(ptr::addr_of!(__preinit_array_start), ptr::addr_of!(__preinit_array_end));
        run_array(ptr::addr_of!(__init_array_start), ptr::addr_of!(__init_array_end));
    }
}

unsafe fn run_array(start: *const u8, end: *const u8) {
    let mut f = start as *const unsafe extern "C" fn();
    while (f as *const u8) < end {
        (*f)();
        f = f.add(1);
    }
}

fn halt(code: u8) -> ! {
    sp1_zkvm::syscalls::syscall_halt(code)
}

fn debug(msg: &[u8]) {
    sp1_zkvm::syscalls::syscall_write(2, msg.as_ptr(), msg.len());
}

// ---- malloc family: blocks from the Rust allocator, with a 16-byte header before each one.

const MIN_ALIGN: usize = 16; // alignof(max_align_t) on rv64

unsafe fn allocate(size: usize, align: usize, zero: bool) -> *mut c_void {
    let align = align.max(MIN_ALIGN);
    let Some(total) = size.checked_add(align) else { return null_mut() };
    let Ok(layout) = Layout::from_size_align(total, align) else { return null_mut() };
    let base = if zero { alloc_zeroed(layout) } else { alloc(layout) };
    if base.is_null() {
        return null_mut();
    }
    let p = base.add(align);
    p.cast::<usize>().sub(2).write(size);
    p.cast::<usize>().sub(1).write(align);
    p.cast()
}

unsafe fn header(p: *mut c_void) -> (usize, usize) {
    (p.cast::<usize>().sub(2).read(), p.cast::<usize>().sub(1).read())
}

#[no_mangle]
pub unsafe extern "C" fn malloc(size: usize) -> *mut c_void {
    allocate(size, MIN_ALIGN, false)
}

#[no_mangle]
pub unsafe extern "C" fn calloc(n: usize, size: usize) -> *mut c_void {
    match n.checked_mul(size) {
        Some(total) => allocate(total, MIN_ALIGN, true),
        None => null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn free(p: *mut c_void) {
    if p.is_null() {
        return;
    }
    let (size, align) = header(p);
    dealloc(p.cast::<u8>().sub(align), Layout::from_size_align_unchecked(size + align, align));
}

#[no_mangle]
pub unsafe extern "C" fn realloc(p: *mut c_void, size: usize) -> *mut c_void {
    if p.is_null() {
        return malloc(size);
    }
    let (old, _) = header(p);
    let q = malloc(size);
    if !q.is_null() {
        ptr::copy_nonoverlapping(p.cast::<u8>(), q.cast::<u8>(), old.min(size));
        free(p);
    }
    q
}

#[no_mangle]
pub unsafe extern "C" fn memalign(align: usize, size: usize) -> *mut c_void {
    if !align.is_power_of_two() {
        return null_mut();
    }
    allocate(size, align, false)
}

#[no_mangle]
pub unsafe extern "C" fn aligned_alloc(align: usize, size: usize) -> *mut c_void {
    memalign(align, size)
}

#[no_mangle]
pub unsafe extern "C" fn posix_memalign(out: *mut *mut c_void, align: usize, size: usize) -> c_int {
    let p = memalign(align, size);
    if p.is_null() {
        return 12; // ENOMEM
    }
    *out = p;
    0
}

#[no_mangle]
pub unsafe extern "C" fn malloc_usable_size(p: *mut c_void) -> usize {
    if p.is_null() { 0 } else { header(p).0 }
}

// newlib's reentrant names, used inside libc itself.
#[no_mangle]
pub unsafe extern "C" fn _malloc_r(_: *mut c_void, size: usize) -> *mut c_void {
    malloc(size)
}
#[no_mangle]
pub unsafe extern "C" fn _calloc_r(_: *mut c_void, n: usize, size: usize) -> *mut c_void {
    calloc(n, size)
}
#[no_mangle]
pub unsafe extern "C" fn _free_r(_: *mut c_void, p: *mut c_void) {
    free(p)
}
#[no_mangle]
pub unsafe extern "C" fn _realloc_r(_: *mut c_void, p: *mut c_void, size: usize) -> *mut c_void {
    realloc(p, size)
}
#[no_mangle]
pub unsafe extern "C" fn _memalign_r(_: *mut c_void, align: usize, size: usize) -> *mut c_void {
    memalign(align, size)
}
#[no_mangle]
pub unsafe extern "C" fn _malloc_usable_size_r(_: *mut c_void, p: *mut c_void) -> usize {
    malloc_usable_size(p)
}

// ---- newlib system calls. No files, no processes, no clock: the program sees an empty system.

extern "C" {
    fn __errno() -> *mut c_int;
}

unsafe fn fail(errno: c_int) -> c_int {
    *__errno() = errno;
    -1
}

const EBADF: c_int = 9;
const ENOENT: c_int = 2;
const ENOMEM: c_int = 12;
const ENOSYS: c_int = 88;
const ESPIPE: c_int = 29;

#[no_mangle]
pub unsafe extern "C" fn _write(fd: c_int, buf: *const c_char, len: usize) -> isize {
    if fd == 1 || fd == 2 {
        sp1_zkvm::syscalls::syscall_write(fd as u32, buf.cast(), len);
        return len as isize;
    }
    fail(EBADF) as isize
}

#[no_mangle]
pub unsafe extern "C" fn _read(_fd: c_int, _buf: *mut c_char, _len: usize) -> isize {
    0
}

#[no_mangle]
pub unsafe extern "C" fn _open(_path: *const c_char, _flags: c_int, _mode: c_int) -> c_int {
    fail(ENOENT)
}

#[no_mangle]
pub unsafe extern "C" fn _close(_fd: c_int) -> c_int {
    fail(EBADF)
}

#[no_mangle]
pub unsafe extern "C" fn _lseek(_fd: c_int, _off: c_long, _whence: c_int) -> c_long {
    fail(ESPIPE) as c_long
}

#[no_mangle]
pub unsafe extern "C" fn _fstat(_fd: c_int, _st: *mut c_void) -> c_int {
    fail(EBADF)
}

#[no_mangle]
pub unsafe extern "C" fn _stat(_path: *const c_char, _st: *mut c_void) -> c_int {
    fail(ENOENT)
}

#[no_mangle]
pub unsafe extern "C" fn _isatty(_fd: c_int) -> c_int {
    0
}

#[no_mangle]
pub unsafe extern "C" fn _sbrk(_incr: isize) -> *mut c_void {
    *__errno() = ENOMEM;
    usize::MAX as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn _unlink(_path: *const c_char) -> c_int {
    fail(ENOENT)
}

#[no_mangle]
pub unsafe extern "C" fn _link(_old: *const c_char, _new: *const c_char) -> c_int {
    fail(ENOSYS)
}

#[no_mangle]
pub unsafe extern "C" fn _getpid() -> c_int {
    1
}

#[no_mangle]
pub unsafe extern "C" fn _kill(_pid: c_int, _sig: c_int) -> c_int {
    fail(ENOSYS)
}

#[no_mangle]
pub unsafe extern "C" fn _gettimeofday(tv: *mut [c_long; 2], _tz: *mut c_void) -> c_int {
    if !tv.is_null() {
        *tv = [0, 0];
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn _times(_buf: *mut c_void) -> c_long {
    fail(ENOSYS) as c_long
}

#[no_mangle]
pub unsafe extern "C" fn _exit(code: c_int) -> ! {
    // The Rust main commits the result; an exit from C or C++ is always a failure.
    debug(b"cxx-runtime: _exit\n");
    halt(if code == 0 { 1 } else { code as u8 })
}

#[no_mangle]
pub unsafe extern "C" fn abort() -> ! {
    debug(b"cxx-runtime: abort\n");
    halt(134)
}

// ---- C++ support that normally comes from crtbegin.o and the stack protector library.

#[no_mangle]
pub static __dso_handle: u8 = 0;

/// Destructors of static objects never run: the program halts instead of exiting.
#[no_mangle]
pub unsafe extern "C" fn __cxa_atexit(_f: *mut c_void, _arg: *mut c_void, _dso: *mut c_void) -> c_int {
    0
}

#[no_mangle]
pub unsafe extern "C" fn atexit(_f: *mut c_void) -> c_int {
    0
}

#[no_mangle]
pub static __stack_chk_guard: usize = 0x5a17_c0de_5a17_c0de;

#[no_mangle]
pub unsafe extern "C" fn __stack_chk_fail() -> ! {
    debug(b"cxx-runtime: stack smashing detected\n");
    halt(134)
}

// ---- Atomics. rv64im has no atomic instructions, so GCC calls these. The zkVM is single-threaded.

macro_rules! atomics {
    ($t:ty, $load:ident, $store:ident, $xchg:ident, $cas:ident, $(($fetch:ident, $op:expr)),*) => {
        #[no_mangle]
        pub unsafe extern "C" fn $load(p: *const $t, _m: c_int) -> $t { p.read() }
        #[no_mangle]
        pub unsafe extern "C" fn $store(p: *mut $t, v: $t, _m: c_int) { p.write(v) }
        #[no_mangle]
        pub unsafe extern "C" fn $xchg(p: *mut $t, v: $t, _m: c_int) -> $t { p.replace(v) }
        #[no_mangle]
        pub unsafe extern "C" fn $cas(p: *mut $t, expected: *mut $t, v: $t, _weak: bool, _s: c_int, _f: c_int) -> bool {
            if p.read() == expected.read() { p.write(v); true } else { expected.write(p.read()); false }
        }
        $(
            #[no_mangle]
            pub unsafe extern "C" fn $fetch(p: *mut $t, v: $t, _m: c_int) -> $t {
                let old = p.read();
                let f: fn($t, $t) -> $t = $op;
                p.write(f(old, v));
                old
            }
        )*
    };
}

atomics!(u8, __atomic_load_1, __atomic_store_1, __atomic_exchange_1, __atomic_compare_exchange_1,
    (__atomic_fetch_add_1, |a, b| a.wrapping_add(b)), (__atomic_fetch_sub_1, |a, b| a.wrapping_sub(b)),
    (__atomic_fetch_and_1, |a, b| a & b), (__atomic_fetch_or_1, |a, b| a | b), (__atomic_fetch_xor_1, |a, b| a ^ b));
atomics!(u16, __atomic_load_2, __atomic_store_2, __atomic_exchange_2, __atomic_compare_exchange_2,
    (__atomic_fetch_add_2, |a, b| a.wrapping_add(b)), (__atomic_fetch_sub_2, |a, b| a.wrapping_sub(b)),
    (__atomic_fetch_and_2, |a, b| a & b), (__atomic_fetch_or_2, |a, b| a | b), (__atomic_fetch_xor_2, |a, b| a ^ b));
atomics!(u32, __atomic_load_4, __atomic_store_4, __atomic_exchange_4, __atomic_compare_exchange_4,
    (__atomic_fetch_add_4, |a, b| a.wrapping_add(b)), (__atomic_fetch_sub_4, |a, b| a.wrapping_sub(b)),
    (__atomic_fetch_and_4, |a, b| a & b), (__atomic_fetch_or_4, |a, b| a | b), (__atomic_fetch_xor_4, |a, b| a ^ b));
atomics!(u64, __atomic_load_8, __atomic_store_8, __atomic_exchange_8, __atomic_compare_exchange_8,
    (__atomic_fetch_add_8, |a, b| a.wrapping_add(b)), (__atomic_fetch_sub_8, |a, b| a.wrapping_sub(b)),
    (__atomic_fetch_and_8, |a, b| a & b), (__atomic_fetch_or_8, |a, b| a | b), (__atomic_fetch_xor_8, |a, b| a ^ b));
