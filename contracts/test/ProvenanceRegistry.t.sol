// SPDX-License-Identifier: GPL-3.0-only
pragma solidity ^0.8.28;

import {Test} from "forge-std/Test.sol";
import {SP1MockVerifier} from "@sp1-contracts/SP1MockVerifier.sol";
import {SP1Verifier} from "@sp1-contracts/v6.1.0/SP1VerifierGroth16.sol";
import {ProvenanceRegistry} from "../src/ProvenanceRegistry.sol";
import {ProvenanceGate} from "../src/ProvenanceGate.sol";

contract ProvenanceRegistryTest is Test {
    string constant JOB = "../examples/sepolia-counter/";
    address constant COUNTER = 0xffD6151Be39D250058911ffcbdb227d4274D40E4;

    ProvenanceRegistry registry;
    bytes counterCode;

    function setUp() public {
        registry = new ProvenanceRegistry(new SP1MockVerifier(), bytes32(uint256(1)));
        counterCode = vm.parseBytes(vm.trim(vm.readFile(string.concat(JOB, "runtime.hex"))));
        vm.etch(COUNTER, counterCode);
    }

    function claimFor(bytes memory code, ProvenanceRegistry.ImmutableRange[] memory ranges)
        internal
        pure
        returns (bytes memory)
    {
        bytes memory zeroed = bytes.concat(code);
        for (uint256 i; i < ranges.length; ++i) {
            for (uint256 j = ranges[i].start; j < ranges[i].start + ranges[i].length; ++j) {
                zeroed[j] = 0;
            }
        }
        return abi.encode(
            ProvenanceRegistry.CompilationClaim({
                inputHash: keccak256("input"),
                compilerVersion: "0.8.38+commit.1835d247",
                contractName: "src/Counter.sol:Counter",
                creationCodeHash: keccak256("creation"),
                runtimeCodeHash: keccak256(zeroed),
                runtimeCodeLength: uint32(code.length),
                immutables: ranges,
                metadataHash: keccak256("metadata")
            })
        );
    }

    function test_registersMatchingCode() public {
        registry.register(COUNTER, claimFor(counterCode, new ProvenanceRegistry.ImmutableRange[](0)), "");
        assertTrue(registry.isProven(COUNTER));
        ProvenanceRegistry.Provenance memory p = registry.provenanceOf(COUNTER);
        assertEq(p.inputHash, keccak256("input"));
        assertEq(p.contractName, "src/Counter.sol:Counter");
    }

    function test_sameCodeAnywhereIsProven() public {
        registry.register(COUNTER, claimFor(counterCode, new ProvenanceRegistry.ImmutableRange[](0)), "");
        address copy = makeAddr("copy");
        vm.etch(copy, counterCode);
        assertTrue(registry.isProven(copy));
    }

    function test_rejectsOtherCode() public {
        bytes memory publicValues = claimFor(counterCode, new ProvenanceRegistry.ImmutableRange[](0));
        bytes memory other = bytes.concat(counterCode);
        other[10] = 0xff;
        vm.etch(COUNTER, other);
        vm.expectPartialRevert(ProvenanceRegistry.CodeMismatch.selector);
        registry.register(COUNTER, publicValues, "");
    }

    function test_rejectsOtherLength() public {
        bytes memory publicValues = claimFor(counterCode, new ProvenanceRegistry.ImmutableRange[](0));
        vm.etch(COUNTER, bytes.concat(counterCode, hex"00"));
        vm.expectPartialRevert(ProvenanceRegistry.LengthMismatch.selector);
        registry.register(COUNTER, publicValues, "");
    }

    function test_rejectsNoCode() public {
        bytes memory publicValues = claimFor(counterCode, new ProvenanceRegistry.ImmutableRange[](0));
        vm.expectPartialRevert(ProvenanceRegistry.NoCode.selector);
        registry.register(makeAddr("eoa"), publicValues, "");
    }

    function test_rejectsInvalidProof() public {
        bytes memory publicValues = claimFor(counterCode, new ProvenanceRegistry.ImmutableRange[](0));
        vm.expectRevert();
        registry.register(COUNTER, publicValues, hex"01");
    }

    function test_ignoresImmutableValues() public {
        // Compiler output has zeros where the deployment wrote immutable values.
        ProvenanceRegistry.ImmutableRange[] memory ranges = new ProvenanceRegistry.ImmutableRange[](1);
        ranges[0] = ProvenanceRegistry.ImmutableRange(2, 4);
        bytes memory publicValues = claimFor(hex"60aa000000005b", ranges);
        address a = makeAddr("a");
        address b = makeAddr("b");
        vm.etch(a, hex"60aa123456785b");
        vm.etch(b, hex"60aacafebabe5b");
        registry.register(a, publicValues, "");
        registry.register(b, publicValues, "");
        assertTrue(registry.isProven(a) && registry.isProven(b));
    }

    function test_gateForwardsOnlyToProvenCode() public {
        ProvenanceGate gate = new ProvenanceGate(registry, keccak256("input"));
        bytes memory increment = abi.encodeWithSignature("increment()");
        vm.expectPartialRevert(ProvenanceGate.NotApproved.selector);
        gate.forward(COUNTER, increment);

        registry.register(COUNTER, claimFor(counterCode, new ProvenanceRegistry.ImmutableRange[](0)), "");
        gate.forward(COUNTER, increment);
        (bool ok, bytes memory ret) = COUNTER.call(abi.encodeWithSignature("number()"));
        assertTrue(ok);
        assertEq(abi.decode(ret, (uint256)), 1);
    }

    function test_rejectsRangeOutOfBounds() public {
        ProvenanceRegistry.ImmutableRange[] memory ranges = new ProvenanceRegistry.ImmutableRange[](1);
        ranges[0] = ProvenanceRegistry.ImmutableRange(2, 4);
        bytes memory publicValues = claimFor(hex"60aa000000005b", ranges);
        ProvenanceRegistry.CompilationClaim memory c = abi.decode(publicValues, (ProvenanceRegistry.CompilationClaim));
        c.immutables[0].length = 100;
        address a = makeAddr("a");
        vm.etch(a, hex"60aa123456785b");
        vm.expectPartialRevert(ProvenanceRegistry.RangeOutOfBounds.selector);
        registry.register(a, abi.encode(c), "");
    }
}

/// Uses the public values the guest really committed, with the mock verifier, so Rust and Solidity agree on the claim.
contract GuestOutputTest is Test {
    string constant FIXTURE = "../examples/sepolia-counter/proof-groth16.json";
    address constant COUNTER = 0xffD6151Be39D250058911ffcbdb227d4274D40E4;

    function test_guestClaimDecodesAndMatchesSepoliaCode() public {
        string memory json = vm.readFile(FIXTURE);
        bytes memory publicValues = vm.parseJsonBytes(json, ".publicValues");
        vm.etch(COUNTER, vm.parseBytes(vm.trim(vm.readFile("../examples/sepolia-counter/runtime.hex"))));

        ProvenanceRegistry registry = new ProvenanceRegistry(new SP1MockVerifier(), vm.parseJsonBytes32(json, ".vkey"));
        registry.register(COUNTER, publicValues, "");

        ProvenanceRegistry.Provenance memory p = registry.provenanceOf(COUNTER);
        assertEq(p.inputHash, vm.parseJsonBytes32(json, ".claim.inputHash"));
        assertEq(p.metadataHash, vm.parseJsonBytes32(json, ".claim.metadataHash"));
        assertEq(p.compilerVersion, "0.8.38+commit.1835d247");
        assertEq(p.contractName, "src/Counter.sol:Counter");
    }
}

/// Uses the real Groth16 proof from `solc-zk-proof prove` and Succinct's v6.1.0 verifier.
contract RealProofTest is Test {
    string constant FIXTURE = "../examples/sepolia-counter/proof-groth16.json";
    address constant COUNTER = 0xffD6151Be39D250058911ffcbdb227d4274D40E4;

    function test_realProof() public {
        vm.skip(!vm.exists(FIXTURE));
        string memory json = vm.readFile(FIXTURE);
        bytes32 vkey = vm.parseJsonBytes32(json, ".vkey");
        bytes memory publicValues = vm.parseJsonBytes(json, ".publicValues");
        bytes memory proof = vm.parseJsonBytes(json, ".proof");

        vm.etch(COUNTER, vm.parseBytes(vm.trim(vm.readFile("../examples/sepolia-counter/runtime.hex"))));
        ProvenanceRegistry registry = new ProvenanceRegistry(new SP1Verifier(), vkey);
        uint256 gasBefore = gasleft();
        registry.register(COUNTER, publicValues, proof);
        emit log_named_uint("register gas", gasBefore - gasleft());

        ProvenanceRegistry.Provenance memory p = registry.provenanceOf(COUNTER);
        assertEq(p.compilerVersion, "0.8.38+commit.1835d247");
        assertEq(p.contractName, "src/Counter.sol:Counter");

        // A changed claim must fail against the same proof.
        publicValues[40] = bytes1(uint8(publicValues[40]) ^ 1);
        vm.expectRevert();
        registry.register(COUNTER, publicValues, proof);
    }
}
