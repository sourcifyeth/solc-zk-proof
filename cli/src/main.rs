//! Command-line tool: prepare a job from Sourcify, run solc in the SP1 zkVM, make and check proofs.

use alloy_sol_types::SolValue;
use anyhow::{bail, ensure, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{json, Value};
use solc_zk_proof_claim::{claim_from_output, runtime_matches, CompilationClaim};
use sp1_sdk::blocking::{EnvProver, ProveRequest, Prover, ProverClient};
use sp1_sdk::{Elf, HashableKey, ProvingKey, SP1Stdin};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

/// Outputs the zkVM program needs. It rejects input without them.
const OUTPUT_FIELDS: &[&str] = &[
    "metadata",
    "evm.bytecode.object",
    "evm.bytecode.linkReferences",
    "evm.deployedBytecode.object",
    "evm.deployedBytecode.linkReferences",
    "evm.deployedBytecode.immutableReferences",
];

#[derive(Parser)]
#[command(about = "Prove that solc compiled a standard JSON input into the deployed bytecode")]
struct Cli {
    /// Folder with one subfolder per solc version, made by scripts/build.sh.
    #[arg(long, global = true, default_value = "builds")]
    builds: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Get the standard JSON input of a verified contract from Sourcify and write a job folder.
    Prepare {
        #[arg(long)]
        chain: u64,
        #[arg(long)]
        address: String,
        #[arg(long, default_value = "https://sourcify.dev/server")]
        sourcify: String,
        /// Job folder to write.
        job: PathBuf,
    },
    /// Run solc in the zkVM without a proof. Compares the claim with the official solc binary
    /// and, with --rpc, with the deployed code.
    Execute {
        job: PathBuf,
        /// Ethereum RPC URL of the job's chain.
        #[arg(long, env = "RPC_URL")]
        rpc: Option<String>,
    },
    /// Make a Groth16 proof and write it to the job folder.
    Prove {
        job: PathBuf,
        /// network: Succinct prover network (needs NETWORK_PRIVATE_KEY). mock: no real proof, for tests.
        #[arg(long, value_enum)]
        prover: ProverKind,
    },
    /// Check the job's Groth16 proof offchain and, with --rpc, compare the claim with the deployed code.
    Verify {
        job: PathBuf,
        #[arg(long, env = "RPC_URL")]
        rpc: Option<String>,
    },
    /// Print the program key (vkey) of a solc build, for example `vkey 0.8.38`.
    Vkey { version: String },
    /// Print the PROVE balance of the prover network account (needs NETWORK_PRIVATE_KEY).
    Balance,
}

#[derive(Clone, Copy, ValueEnum)]
enum ProverKind {
    Network,
    Mock,
}

/// The zkVM program and the official binary of one solc version.
struct Build {
    dir: PathBuf,
}

impl Build {
    /// Finds the build for a long version (0.8.38+commit.1835d247) or a short one (0.8.38).
    fn find(builds: &Path, version: &str) -> Result<Self> {
        let short = version.split('+').next().unwrap_or(version);
        let hint = || format!("no zkVM build of solc {version} in {}: run scripts/build.sh {short}", builds.display());
        let exact = builds.join(version);
        if exact.join("program.elf").exists() {
            return Ok(Self { dir: exact });
        }
        ensure!(!version.contains('+'), hint());
        let mut found = Vec::new();
        for entry in fs::read_dir(builds).with_context(hint)? {
            let path = entry?.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            if name.starts_with(&format!("{version}+")) && path.join("program.elf").exists() {
                found.push(path);
            }
        }
        match found.as_slice() {
            [dir] => Ok(Self { dir: dir.clone() }),
            [] => bail!(hint()),
            _ => bail!("more than one build matches solc {version}: give the long version"),
        }
    }

    fn elf(&self) -> Result<Elf> {
        let path = self.dir.join("program.elf");
        Ok(Elf::from(fs::read(&path).with_context(|| format!("read {}", path.display()))?))
    }

    /// Program key, computed from the ELF with the mock prover (no proving work).
    fn vkey(&self) -> Result<String> {
        let pk = ProverClient::builder().mock().build().setup(self.elf()?).context("setup")?;
        Ok(pk.verifying_key().bytes32())
    }
}

/// A job folder: input.json (standard JSON input) and job.json (contract, compiler, deployment).
struct Job {
    dir: PathBuf,
    input: Vec<u8>,
    meta: Value,
}

impl Job {
    fn load(dir: &Path) -> Result<Self> {
        let input = fs::read(dir.join("input.json")).context("read input.json")?;
        let meta = serde_json::from_slice(&fs::read(dir.join("job.json")).context("read job.json")?)?;
        Ok(Self { dir: dir.to_path_buf(), input, meta })
    }

    fn field(&self, key: &str) -> Result<&str> {
        self.meta[key].as_str().with_context(|| format!("job.json has no {key}"))
    }

    fn build(&self, builds: &Path) -> Result<Build> {
        Build::find(builds, self.field("compilerVersion")?)
    }

    fn stdin(&self) -> Result<SP1Stdin> {
        let mut stdin = SP1Stdin::new();
        stdin.write_vec(self.input.clone());
        stdin.write_vec(self.field("contract")?.as_bytes().to_vec());
        Ok(stdin)
    }
}

fn main() -> Result<()> {
    // Show the prover network's request links, which the SDK logs at info level.
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "sp1_sdk::network=info");
    }
    sp1_sdk::utils::setup_logger();
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Prepare { chain, address, sourcify, job } => prepare(chain, &address, &sourcify, &job),
        Cmd::Execute { job, rpc } => execute(&cli.builds, &Job::load(&job)?, rpc.as_deref()),
        Cmd::Prove { job, prover } => prove(&cli.builds, &Job::load(&job)?, prover),
        Cmd::Verify { job, rpc } => verify(&cli.builds, &Job::load(&job)?, rpc.as_deref()),
        Cmd::Vkey { version } => {
            println!("{}", Build::find(&cli.builds, &version)?.vkey()?);
            Ok(())
        }
        Cmd::Balance => {
            let wei = ProverClient::builder().network().build().get_balance()?;
            println!("{} PROVE", alloy_primitives::utils::format_ether(wei));
            Ok(())
        }
    }
}

fn prepare(chain: u64, address: &str, sourcify: &str, dir: &Path) -> Result<()> {
    let url = format!("{sourcify}/v2/contract/{chain}/{address}?fields=stdJsonInput,compilation");
    let res: Value = ureq::get(&url).call()?.body_mut().read_json()?;
    let compilation = &res["compilation"];
    ensure!(compilation["language"] == "Solidity", "not a Solidity contract");
    let version = compilation["compilerVersion"].as_str().context("Sourcify gave no compiler version")?;
    let name = compilation["fullyQualifiedName"].as_str().context("Sourcify gave no contract name")?;
    let (path, contract) = name.rsplit_once(':').context("bad contract name")?;

    // Sourcify's input, with the output selection that the zkVM program needs.
    let mut input = res["stdJsonInput"].clone();
    input["settings"]["outputSelection"] = json!({ path: { contract: OUTPUT_FIELDS } });

    fs::create_dir_all(dir)?;
    fs::write(dir.join("input.json"), serde_json::to_vec(&input)?)?;
    let meta = json!({ "chainId": chain, "address": address, "contract": name, "compilerVersion": version });
    fs::write(dir.join("job.json"), serde_json::to_string_pretty(&meta)? + "\n")?;
    println!("Wrote {} for {name}, solc {version}", dir.display());
    Ok(())
}

fn execute(builds: &Path, job: &Job, rpc: Option<&str>) -> Result<()> {
    let build = job.build(builds)?;
    let start = Instant::now();
    let client = ProverClient::builder().mock().build();
    let (output, report) = client.execute(build.elf()?, job.stdin()?).run().context("execute")?;
    // Exit code 1: Rust panic, for example compiler errors in the input. 134: abort in C++.
    ensure!(report.exit_code == 0, "the zkVM program failed with exit code {}", report.exit_code);
    let claim = CompilationClaim::abi_decode(output.as_slice()).context("decode the claim")?;
    println!("Cycles: {} ({:.1?})", report.total_instruction_count(), start.elapsed());
    println!("Prover gas: {}", report.gas().map_or("unknown".into(), |g| g.to_string()));
    print_claim(&claim);
    ensure!(claim.compilerVersion == job.field("compilerVersion")?, "the claim has another compiler version than job.json");

    let official = official_claim(&build.dir.join("solc"), job)?;
    ensure!(official == claim, "the official solc binary gives another claim:\n{official:#?}");
    println!("The official solc binary gives the same claim.");
    if let Some(rpc) = rpc {
        check_deployed_code(job, &claim, rpc)?;
    }
    Ok(())
}

fn prove(builds: &Path, job: &Job, kind: ProverKind) -> Result<()> {
    let build = job.build(builds)?;
    let client = match kind {
        ProverKind::Network => EnvProver::Network(Box::new(ProverClient::builder().network().build())),
        ProverKind::Mock => EnvProver::Mock(ProverClient::builder().mock().build()),
    };
    let pk = client.setup(build.elf()?).context("setup")?;
    let start = Instant::now();
    let proof = client.prove(&pk, job.stdin()?).groth16().run().context("prove")?;
    println!("Proved in {:.0?}", start.elapsed());
    client.verify(&proof, pk.verifying_key(), None).context("verify")?;

    let claim = CompilationClaim::abi_decode(proof.public_values.as_slice())?;
    let file = json!({
        "vkey": pk.verifying_key().bytes32(),
        "publicValues": format!("0x{}", hex::encode(proof.public_values.as_slice())),
        "proof": format!("0x{}", hex::encode(proof.bytes())),
        "claim": claim_json(&claim),
    });
    // Mock proofs get their own file name, so they never pass for real ones.
    let path = job.dir.join(match kind {
        ProverKind::Network => "proof-groth16.json",
        ProverKind::Mock => "proof-mock.json",
    });
    fs::write(&path, serde_json::to_string_pretty(&file)? + "\n")?;
    println!("Wrote {}", path.display());
    Ok(())
}

fn verify(builds: &Path, job: &Job, rpc: Option<&str>) -> Result<()> {
    let file: Value = serde_json::from_slice(&fs::read(job.dir.join("proof-groth16.json")).context("read proof-groth16.json")?)?;
    let field = |key: &str| -> Result<Vec<u8>> {
        let hex = file[key].as_str().with_context(|| format!("proof-groth16.json has no {key}"))?;
        Ok(hex::decode(hex.trim_start_matches("0x"))?)
    };
    let (proof, public_values) = (field("proof")?, field("publicValues")?);

    // The program key comes from the local build, never from the proof file.
    let vkey = job.build(builds)?.vkey()?;
    let start = Instant::now();
    sp1_verifier::Groth16Verifier::verify(&proof, &public_values, &vkey, &sp1_verifier::GROTH16_VK_BYTES)
        .map_err(|e| anyhow::anyhow!("the proof is not valid: {e}"))?;
    println!("The Groth16 proof is valid for program {vkey} ({:.1?}).", start.elapsed());

    let claim = CompilationClaim::abi_decode(&public_values)?;
    ensure!(claim.inputHash == alloy_primitives::keccak256(&job.input), "the proof is for another input");
    print_claim(&claim);
    if let Some(rpc) = rpc {
        check_deployed_code(job, &claim, rpc)?;
    }
    Ok(())
}

fn check_deployed_code(job: &Job, claim: &CompilationClaim, rpc: &str) -> Result<()> {
    let address = job.field("address")?;
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "eth_getCode", "params": [address, "latest"] });
    let res: Value = ureq::post(rpc).send_json(body)?.body_mut().read_json()?;
    let code = hex::decode(res["result"].as_str().context("no eth_getCode result")?.trim_start_matches("0x"))?;
    ensure!(runtime_matches(&code, claim), "the code at {address} does not match the claim");
    println!("The code at {address} matches the claim.");
    Ok(())
}

/// Claim from the official solc binary for the same input, made outside the zkVM.
fn official_claim(solc: &Path, job: &Job) -> Result<CompilationClaim> {
    let mut child = Command::new(solc)
        .arg("--standard-json")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("run {}", solc.display()))?;
    child.stdin.take().context("stdin")?.write_all(&job.input)?;
    let out = child.wait_with_output()?;
    ensure!(out.status.success(), "{} failed", solc.display());
    claim_from_output(&job.input, &out.stdout, job.field("contract")?).map_err(anyhow::Error::msg)
}

fn claim_json(c: &CompilationClaim) -> Value {
    json!({
        "inputHash": c.inputHash,
        "compilerVersion": c.compilerVersion,
        "contractName": c.contractName,
        "creationCodeHash": c.creationCodeHash,
        "runtimeCodeHash": c.runtimeCodeHash,
        "runtimeCodeLength": c.runtimeCodeLength,
        "immutables": c.immutables.iter().map(|r| json!({ "start": r.start, "length": r.length })).collect::<Vec<_>>(),
        "metadataHash": c.metadataHash,
    })
}

fn print_claim(c: &CompilationClaim) {
    println!("Claim: {}", serde_json::to_string_pretty(&claim_json(c)).unwrap_or_default());
}
