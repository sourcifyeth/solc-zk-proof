# solc-zk-proof

Prove that solc compiled a Solidity source into the code of a deployed contract.

solc runs inside the [SP1](https://github.com/succinctlabs/sp1) zkVM (zero-knowledge virtual machine). SP1 makes a small proof of that run. A contract on the chain checks the proof and compares the result with the deployed code. After that, a wallet, an explorer or another contract can trust the link between source and code. Nobody has to recompile, and nobody has to trust the party that did.

## How it works

1. **Input.** The standard JSON input of a contract: all sources and the compiler settings. Sourcify stores this input for every verified contract.
2. **Compile in the zkVM.** The zkVM program runs solc on the input. solc is built from the official release source for the zkVM's RISC-V target. No line of solc is changed.
3. **Prove.** The program outputs a claim (see below). SP1 makes a Groth16 proof that this exact program gave this claim for this input. The proof is 356 bytes.
4. **Check onchain.** `ProvenanceRegistry` checks the proof with Succinct's SP1 verifier contract. Then it compares the claim with the code at the contract's address and stores the result under the code hash.

Each solc version is a separate zkVM program with its own program key (vkey). A registry accepts one program key, so there is one registry for each solc version.

### The claim

| Field | What it is |
| --- | --- |
| `inputHash` | keccak256 of the standard JSON input bytes. It pins the sources and the settings. |
| `compilerVersion` | The version from solc's metadata, for example `0.8.38+commit.1835d247`. |
| `contractName` | The contract, for example `src/Counter.sol:Counter`. |
| `creationCodeHash` | keccak256 of the creation bytecode. |
| `runtimeCodeHash` | keccak256 of the runtime bytecode, with the immutable values set to zero. |
| `runtimeCodeLength` | Length of the runtime bytecode. |
| `immutables` | Byte ranges of the immutable values. The constructor sets them, so solc cannot know them. |
| `metadataHash` | keccak256 of the metadata JSON. The bytecode contains the IPFS hash of the same metadata. |

The registry sets the immutable ranges of the deployed code to zero before it compares. So one proof covers every deployment of the same compiled contract. Every copy with the same code hash is proven at once. A contract with immutables needs one `register()` call per deployment, with the same proof.

## Requirements

- Linux on x86_64.
- Rust, and the SP1 toolchain v6.10.0: `curl -L https://sp1up.succinct.xyz | bash`, then `sp1up --version v6.10.0`.
- `protoc` (on Debian or Ubuntu: `apt install protobuf-compiler`).
- [Foundry](https://getfoundry.sh), for the contracts.
- `curl`, `tar` and `jq`.
- About 10 GB of disk space. The solc build needs up to 1.5 GB of RAM per compile job.

## Build a solc version

```sh
git submodule update --init
scripts/build.sh 0.8.38
cargo build --release
```

`scripts/build.sh <version>` does these steps:

1. It downloads the pinned tools into `.cache/` and checks their SHA-256 hashes: Succinct's GCC 15.2 for RISC-V (with newlib and libstdc++), Boost 1.83.0 and CMake 4.4.5.
2. It downloads the official solc release of that version:
   - The Linux binary. Its hash must match the one in [binaries.soliditylang.org](https://binaries.soliditylang.org/linux-amd64/list.json).
   - The source tarball from GitHub. When GitHub publishes a hash for the file (newer releases), the file must match it. The commit in the tarball must always be the commit of the official release.
3. It cross-builds solc's library (libsolc) with solc's own CMake files.
4. It builds the zkVM program with `cargo prove build`.

The result is in `builds/<long version>/`:
- `program.elf`: the zkVM program.
- `solc`: the official binary, which `execute` uses to compare outputs.
- `build.json`: the hashes of the inputs and of the program.

The build takes about 10 minutes. `JOBS` sets the number of parallel compile jobs (default 4).

The program key depends only on the sources and on the pinned tools, not on the folder or the user. `solc-zk-proof vkey 0.8.38` prints it. For solc 0.8.38 it is `0x00968b73b7c674d717db7856c6aaa8fc9b84ed336845302beaac76b2461306a2`.

Only solc 0.8.38 is tested. The script accepts any official release, but other versions can need changes in `solc-build/`. Each version is a separate build, and the tool selects the build that matches a job's compiler version.

## Prove a contract

```sh
cli=./target/release/solc-zk-proof
rpc=https://ethereum-sepolia-rpc.publicnode.com

# 1. Get the input of a contract that is verified on Sourcify.
$cli prepare --chain 11155111 --address 0xffD6151Be39D250058911ffcbdb227d4274D40E4 jobs/counter

# 2. Run solc in the zkVM, without a proof. This prints the cycles and the claim,
#    and checks the claim against the official solc binary and against the deployed code.
$cli execute jobs/counter --rpc $rpc

# 3. Make the proof on the Succinct prover network. It is written to jobs/counter/proof-groth16.json.
NETWORK_PRIVATE_KEY=... $cli prove jobs/counter --prover network

# 4. Check the proof offchain, as a wallet would.
$cli verify jobs/counter --rpc $rpc

# 5. Deploy a registry for this solc version and the example gate, and register the contract.
cd contracts
JOB=../jobs/counter forge script script/Deploy.s.sol --rpc-url $rpc --private-key ... --broadcast
```

A job folder holds `input.json` (the exact bytes that are hashed) and `job.json` (contract, compiler version, chain and address). `prove --prover mock` runs the same flow without a real proof and writes `proof-mock.json`. Use it for tests.

To register more contracts of the same solc version, call `register(target, publicValues, proof)` on the existing registry. Anyone can call it. The registry accepts only valid proofs that match the deployed code.

### The prover network

`--prover network` sends the program and the input to the [Succinct prover network](https://docs.succinct.xyz/docs/sp1/prover-network/quickstart). A prover there makes the proof, and you pay a fee in PROVE, a token on Ethereum mainnet. You do not need to trust the prover: `verify` and the registry check every proof.

1. Make a key for the network account, for example with `cast wallet new`. Set it as `NETWORK_PRIVATE_KEY`.
2. Get some PROVE on Ethereum mainnet. The token address is `0x6BEF15D938d4E72056AC92Ea4bDD0D76B1C4ad29`.
3. Open [explorer.succinct.xyz/account](https://explorer.succinct.xyz/account) and deposit the PROVE. If you deposit from another wallet, use **Transfer** to send it to the network account.
4. `solc-zk-proof balance` shows the balance.

In November 2026, a proof of the example cost about 0.40 PROVE (about €0.07) and took 30 to 90 seconds. The tool prints a link to the request on the explorer.

## Example

`examples/sepolia-counter/` is a small `Counter` on Sepolia, compiled with solc 0.8.38 and verified on Sourcify. It has the proof for the program key above.

```sh
./target/release/solc-zk-proof execute examples/sepolia-counter --rpc https://ethereum-sepolia-rpc.publicnode.com
./target/release/solc-zk-proof verify examples/sepolia-counter --rpc https://ethereum-sepolia-rpc.publicnode.com
```

| What | Result |
| --- | --- |
| Compile in the zkVM | 12,972,840 cycles, about 3 seconds without a proof |
| Proof | 356 bytes. The claim is 448 bytes. |
| Check offchain | Less than 0.5 seconds |
| Check onchain (`verifyProof`) | About 262,000 gas |
| `register()` on Sepolia | 710,527 gas |

Contracts on Sepolia, deployed from this repository:

| Contract | Address |
| --- | --- |
| Counter | [`0xffD6151Be39D250058911ffcbdb227d4274D40E4`](https://repo.sourcify.dev/11155111/0xffD6151Be39D250058911ffcbdb227d4274D40E4) |
| ProvenanceRegistry for solc 0.8.38 | [`0xEa0216F705d9d4d1FA4E77d2b0D303eaA9bFA070`](https://repo.sourcify.dev/11155111/0xEa0216F705d9d4d1FA4E77d2b0D303eaA9bFA070) |
| ProvenanceGate | [`0xfa796BCa39942657C92e3ED772c8D988ED5f3882`](https://repo.sourcify.dev/11155111/0xfa796BCa39942657C92e3ED772c8D988ED5f3882) |
| SP1 verifier gateway (Succinct) | [`0x397A5f7f3dBd538f23DE225B51f532c34448dA9B`](https://repo.sourcify.dev/11155111/0x397A5f7f3dBd538f23DE225B51f532c34448dA9B) |

## How solc runs in the zkVM

The zkVM is a 64-bit RISC-V machine (`rv64im`) with no operating system. SP1 has a Rust toolchain for it, but solc is C++. The build supplies what a C++ program usually gets from the operating system and from a normal GCC link. It changes no line of solc:

| solc needs | Where it comes from |
| --- | --- |
| A C++ compiler for `rv64im` | Succinct's GCC 15.2 toolchain, with newlib and libstdc++ |
| Heap memory | `malloc` and `free` on top of the zkVM's Rust allocator (`guest/runtime`) |
| stdout and stderr | SP1's debug output (`guest/runtime`) |
| Static constructors | `guest/runtime` runs `.init_array` before solc starts |
| C++ exceptions | `guest/eh_frame.ld` marks the unwind tables, and `guest/runtime` registers them |
| Atomic operations (`rv64im` has none) | Plain loads and stores in `guest/runtime`. The zkVM has one thread. |
| Boost 1.83 | The release headers. Only the path code of Boost.Filesystem is compiled. |
| Files, a terminal, other programs | None. Stand-ins in `solc-build/` report "no file", "no terminal" and "no SMT solver found". |

The zkVM program calls `solidity_compile`, solc's C entry point. solc-js uses the same entry point. There is no read callback, so the input must contain every source. The inputs that Sourcify stores always do.

## Contracts

- `ProvenanceRegistry` checks a proof for its program key, compares the claim with the deployed code and stores the result. `isProven(address)` and `provenanceOf(address)` read it.
- `ProvenanceGate` is an example consumer. It forwards a call only to a contract whose code is proven to come from one approved input.

`cd contracts && forge test` runs the tests. One test checks the example's real proof against Succinct's Groth16 verifier.

## Repository layout

| Path | What |
| --- | --- |
| `scripts/` | `setup.sh` (pinned tools) and `build.sh <version>` (one solc version for the zkVM) |
| `solc-build/` | CMake toolchain, project include, Boost package and stand-in headers for the cross-build |
| `guest/program/` | The zkVM program |
| `guest/runtime/` | The C and C++ runtime for the zkVM |
| `guest/claim/` | The claim, and how it is built from solc's output. The tool uses it too. |
| `cli/` | The `solc-zk-proof` command-line tool |
| `contracts/` | The registry, the example gate, tests and the deploy script |
| `examples/` | The example job with its proof |

## Limits

- Only solc 0.8.38 is tested.
- The deployed code must match the compiled code exactly, except for immutables. So a contract needs an exact match on Sourcify: a partial match has another metadata hash.
- Libraries must be linked at compile time (`settings.libraries` in the input). The zkVM program rejects output with open link references.
- Only the runtime code is checked. The creation code and the constructor arguments are not.
- One proof covers one contract of one input.

## License

GPL-3.0. solc is GPL-3.0, and the zkVM program contains it.
