// SPDX-License-Identifier: GPL-3.0-only
pragma solidity ^0.8.28;

import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";

/// @title ProvenanceRegistry
/// @notice Maps deployed runtime code to the compiler input it was proven to come from.
/// @dev A proof shows that the guest program (solc inside the SP1 zkVM) compiled an input into
/// code with a given hash. Anyone can register; the registry checks the proof and the code.
contract ProvenanceRegistry {
    /// @dev Must match `ImmutableRange` in guest/claim/src/lib.rs.
    struct ImmutableRange {
        uint32 start;
        uint32 length;
    }

    /// @dev Must match `CompilationClaim` in guest/claim/src/lib.rs (the proof's public values).
    struct CompilationClaim {
        bytes32 inputHash;
        string compilerVersion;
        string contractName;
        bytes32 creationCodeHash;
        bytes32 runtimeCodeHash;
        uint32 runtimeCodeLength;
        ImmutableRange[] immutables;
        bytes32 metadataHash;
    }

    struct Provenance {
        bytes32 inputHash;
        bytes32 metadataHash;
        string compilerVersion;
        string contractName;
    }

    /// @notice SP1 verifier (gateway) that checks the proofs.
    ISP1Verifier public immutable verifier;
    /// @notice Verification key of the guest program; it pins the compiler build.
    bytes32 public immutable programVKey;

    mapping(bytes32 codehash => Provenance) private _provenance;

    event Registered(
        address indexed target,
        bytes32 indexed codehash,
        bytes32 inputHash,
        bytes32 metadataHash,
        string contractName,
        string compilerVersion
    );

    error NoCode(address target);
    error LengthMismatch(uint256 onchain, uint256 claimed);
    error RangeOutOfBounds(uint256 start, uint256 length);
    error CodeMismatch(bytes32 onchain, bytes32 claimed);

    constructor(ISP1Verifier verifier_, bytes32 programVKey_) {
        verifier = verifier_;
        programVKey = programVKey_;
    }

    /// @notice Checks a proof and records its claim for the code at `target`.
    function register(address target, bytes calldata publicValues, bytes calldata proofBytes)
        external
        returns (bytes32 codehash)
    {
        verifier.verifyProof(programVKey, publicValues, proofBytes);
        CompilationClaim memory claim = abi.decode(publicValues, (CompilationClaim));

        bytes memory code = target.code;
        if (code.length == 0) revert NoCode(target);
        if (code.length != claim.runtimeCodeLength) revert LengthMismatch(code.length, claim.runtimeCodeLength);

        // Immutable values differ per deployment; the compiler output has zeros there.
        for (uint256 i; i < claim.immutables.length; ++i) {
            uint256 start = claim.immutables[i].start;
            uint256 end = start + claim.immutables[i].length;
            if (end > code.length) revert RangeOutOfBounds(start, claim.immutables[i].length);
            for (uint256 j = start; j < end; ++j) {
                code[j] = 0;
            }
        }
        bytes32 normalized = keccak256(code);
        if (normalized != claim.runtimeCodeHash) revert CodeMismatch(normalized, claim.runtimeCodeHash);

        codehash = target.codehash;
        _provenance[codehash] =
            Provenance(claim.inputHash, claim.metadataHash, claim.compilerVersion, claim.contractName);
        emit Registered(
            target, codehash, claim.inputHash, claim.metadataHash, claim.contractName, claim.compilerVersion
        );
    }

    /// @notice Proven provenance of the code at `target`; empty if none.
    function provenanceOf(address target) external view returns (Provenance memory) {
        return _provenance[target.codehash];
    }

    /// @notice True if the code at `target` has a registered proof.
    function isProven(address target) external view returns (bool) {
        return _provenance[target.codehash].inputHash != bytes32(0);
    }
}
