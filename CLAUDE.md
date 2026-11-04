# CLAUDE.md

Guidance for coding agents in this repository. Read README.md first: it explains what the project does and how to run it.

## Commands

```sh
git submodule update --init                       # forge-std and sp1-contracts
scripts/build.sh 0.8.38                           # one solc version for the zkVM, into builds/
PROTOC=/path/to/protoc cargo build --release      # the command-line tool
cargo test -p solc-zk-proof-claim                 # claim unit tests
cd contracts && forge test                        # contract tests, including the real example proof
./target/release/solc-zk-proof execute examples/sepolia-counter --rpc https://ethereum-sepolia-rpc.publicnode.com
./target/release/solc-zk-proof verify examples/sepolia-counter --rpc https://ethereum-sepolia-rpc.publicnode.com
```

`execute` and `verify` are the end-to-end check. `execute` must print "The official solc binary gives the same claim."

## Rules

- **solc's source stays unchanged.** Put every adaptation for the zkVM in `solc-build/` (CMake toolchain, project include, stand-in headers, Boost package) or in `guest/runtime/`. Never patch the extracted source in `.cache/`.
- **The claim has two definitions that must match:** `CompilationClaim` in `guest/claim/src/lib.rs` and in `contracts/src/ProvenanceRegistry.sol`.
- **The program key changes when the zkVM program changes.** That includes changes in `guest/` (also `guest/claim/`), `solc-build/`, `scripts/build.sh`, `guest/Cargo.lock` and the pinned tool versions. After such a change, `examples/sepolia-counter/proof-groth16.json` and the registry deployed for it no longer match. Say so in your summary. Do not make a new network proof without asking.
- **The program key must not depend on local paths.** Keep `-ffile-prefix-map` (GCC) and `--remap-path-prefix` (Rust) in place. To check, build in two different folders and compare `solc-zk-proof vkey 0.8.38`.
- **Network proofs cost real money.** `prove --prover network` spends PROVE from the account of `NETWORK_PRIVATE_KEY`. Ask the user before each one. Use `--prover mock` for tests.
- **Never print keys.** This covers `NETWORK_PRIVATE_KEY` and any deployer key: not in logs, commits or comments.
- **Memory:** each compile job of the solc build needs up to 1.5 GB of RAM. On a shared machine, set `JOBS` low.
- **Docs:** README.md is for people. Use simple English, short sentences and no filler words.

## Layout

| Path | What |
| --- | --- |
| `guest/claim/` | The claim type and how it is built from solc's standard JSON output. Used by the zkVM program and the tool. It must stay inside the guest workspace: Cargo hashes crates outside it by absolute path, and that changes the program key. |
| `guest/program/` | The zkVM program: reads the input, calls `solidity_compile`, commits the claim. |
| `guest/runtime/` | C and C++ runtime for the zkVM: heap, system calls, static constructors, unwind tables, atomics. |
| `guest/eh_frame.ld` | Linker script addition that marks `.eh_frame` for the C++ unwinder. |
| `solc-build/` | CMake toolchain, project include, Boost package and stand-in headers for the cross-build. |
| `scripts/` | `setup.sh` (pinned tools) and `build.sh <version>` (one solc version). |
| `cli/` | The `solc-zk-proof` tool. |
| `contracts/` | `ProvenanceRegistry`, the example `ProvenanceGate`, tests and the deploy script. |
| `examples/sepolia-counter/` | A real job with its proof. The contract tests use it. |
