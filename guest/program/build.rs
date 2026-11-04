// Links solc's libraries (built by scripts/build.sh) and the GCC runtime libraries.
use std::path::{Path, PathBuf};

/// solc's static libraries and Boost.Filesystem, in the folder SOLC_ZK_LIBS.
const LIBS: &[&str] = &["solc", "solidity", "yul", "evmasm", "smtutil", "langutil", "solutil", "boost_filesystem"];
const GCC_VERSION: &str = "15.2.0";

fn main() {
    let libs = env_path("SOLC_ZK_LIBS");
    let riscv = env_path("SOLC_ZK_RISCV");
    for name in LIBS {
        link(&libs.join(format!("lib{name}.a")));
    }
    let newlib = riscv.join("riscv64-unknown-elf/lib");
    let gcc = riscv.join("lib/gcc/riscv64-unknown-elf").join(GCC_VERSION);
    for lib in [newlib.join("libstdc++.a"), newlib.join("libm.a"), newlib.join("libc.a"), gcc.join("libgcc.a")] {
        link(&lib);
    }
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../eh_frame.ld");
    println!("cargo:rerun-if-changed={}", script.display());
    println!("cargo:rustc-link-arg=-T{}", script.display());
}

fn env_path(name: &str) -> PathBuf {
    println!("cargo:rerun-if-env-changed={name}");
    PathBuf::from(std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set: build with scripts/build.sh")))
}

fn link(lib: &Path) {
    assert!(lib.exists(), "missing {}: build with scripts/build.sh", lib.display());
    println!("cargo:rerun-if-changed={}", lib.display());
    println!("cargo:rustc-link-arg={}", lib.display());
}
