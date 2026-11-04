// SPDX-License-Identifier: GPL-3.0-only
pragma solidity ^0.8.28;

import {ProvenanceRegistry} from "./ProvenanceRegistry.sol";

/// @title ProvenanceGate
/// @notice Example consumer: forwards calls only to code proven to come from one approved input.
/// @dev Think of a DAO that approved a source; any deployment of it passes, anything else reverts.
contract ProvenanceGate {
    ProvenanceRegistry public immutable registry;
    bytes32 public immutable approvedInputHash;

    error NotApproved(address target);

    constructor(ProvenanceRegistry registry_, bytes32 approvedInputHash_) {
        registry = registry_;
        approvedInputHash = approvedInputHash_;
    }

    function forward(address target, bytes calldata data) external returns (bytes memory result) {
        if (registry.provenanceOf(target).inputHash != approvedInputHash) revert NotApproved(target);
        bool ok;
        (ok, result) = target.call(data);
        if (!ok) {
            assembly {
                revert(add(result, 32), mload(result))
            }
        }
    }
}
