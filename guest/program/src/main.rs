//! zkVM program: compiles standard JSON input with solc (libsolc, the entry point that solc-js
//! also uses) and commits the claim for one contract.
#![no_main]
sp1_zkvm::entrypoint!(main);

use alloy_sol_types::SolValue;
use std::ffi::{c_char, c_void, CStr, CString};

type ReadCallback = Option<unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char, *mut *mut c_char, *mut *mut c_char)>;

extern "C" {
    fn solidity_compile(input: *const c_char, callback: ReadCallback, context: *mut c_void) -> *mut c_char;
}

pub fn main() {
    cxx_runtime::init();
    let input = sp1_zkvm::io::read_vec();
    let contract = String::from_utf8(sp1_zkvm::io::read_vec()).expect("contract is not UTF-8");

    // No read callback: the input must hold every source, as Sourcify's inputs do.
    let c_input = CString::new(input.clone()).expect("input has a NUL byte");
    let output = unsafe { CStr::from_ptr(solidity_compile(c_input.as_ptr(), None, std::ptr::null_mut())) };

    let claim = solc_zk_proof_claim::claim_from_output(&input, output.to_bytes(), &contract).expect("claim");
    sp1_zkvm::io::commit_slice(&claim.abi_encode());
}
