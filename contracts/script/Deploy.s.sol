// SPDX-License-Identifier: GPL-3.0-only
pragma solidity ^0.8.28;

import {Script, console} from "forge-std/Script.sol";
import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";
import {ProvenanceRegistry} from "../src/ProvenanceRegistry.sol";
import {ProvenanceGate} from "../src/ProvenanceGate.sol";

/// Deploys a registry for one solc version and the example gate, then registers the job's contract.
/// The registry pins the program key from the job's proof file, so each solc version needs its own registry.
/// JOB selects the job folder (default: the example). VERIFIER overrides the SP1 verifier gateway.
contract Deploy is Script {
    // Succinct's SP1 Groth16 verifier gateway. It has this address on every chain in
    // lib/sp1-contracts/contracts/deployments.
    address constant GATEWAY = 0x397A5f7f3dBd538f23DE225B51f532c34448dA9B;

    function run() external {
        string memory job = vm.envOr("JOB", string("../examples/sepolia-counter"));
        string memory proof = vm.readFile(string.concat(job, "/proof-groth16.json"));
        address target = vm.parseJsonAddress(vm.readFile(string.concat(job, "/job.json")), ".address");
        ISP1Verifier verifier = ISP1Verifier(vm.envOr("VERIFIER", GATEWAY));

        vm.startBroadcast();
        ProvenanceRegistry registry = new ProvenanceRegistry(verifier, vm.parseJsonBytes32(proof, ".vkey"));
        ProvenanceGate gate = new ProvenanceGate(registry, vm.parseJsonBytes32(proof, ".claim.inputHash"));
        registry.register(target, vm.parseJsonBytes(proof, ".publicValues"), vm.parseJsonBytes(proof, ".proof"));
        vm.stopBroadcast();

        console.log("registry", address(registry));
        console.log("gate", address(gate));
    }
}
