# GCP architecture

This is a plan. Nothing in it is built yet.

A Cloud Run Job makes proofs for compilations that are verified on Sourcify. The job does no proving work itself: the Succinct prover network makes the proofs. The job prepares the input, checks it, sends the request, waits, checks the proof and stores it.

| Part | What it does |
| --- | --- |
| Sourcify server | Adds a queued row for a compilation, then starts a job execution through the Cloud Run Admin API. |
| Cloud Run Job | Prepares the input, talks to the prover network, checks and stores the proofs. |
| Cloud Storage bucket | Holds `program.elf` and `build.json` for each solc build, at a path that contains the program key. |
| Sourcify Postgres | Holds the sources and settings, and a new table with one row per `(compilation_id, program_vkey)`. The table is also the work queue. |
| Secret Manager | Holds `NETWORK_PRIVATE_KEY`. The prover network takes its fee in PROVE from this account. |
| Succinct prover network | Runs an auction. The prover that wins makes the Groth16 proof. |

## Flow

```mermaid
sequenceDiagram
    autonumber
    participant SV as Sourcify server
    participant J as Cloud Run Job
    participant B as Bucket
    participant DB as Sourcify Postgres
    participant N as Succinct prover network

    SV->>DB: add a row for the compilation (status queued)
    SV->>J: start a job execution (Cloud Run Admin API)
    J->>DB: claim a queued row (status executing)
    J->>B: download program.elf for the solc version
    J->>J: compare the program key with the pinned list
    J->>DB: read sources and settings
    J->>J: build the input and execute it locally, without a proof
    J->>DB: compare the claim with the stored code hashes
    alt claim does not match, or too many cycles
        J->>DB: status failed, with the reason
    else claim matches
        J->>N: request(program, input, Groth16) with cycle limit and price cap
        N-->>J: request_id
        J->>DB: store request_id (status requested)
        Note over N: auction, then one prover makes the proof
        J->>N: wait_proof(request_id)
        N-->>J: proof and public values
        J->>J: verify the proof offchain with the pinned program key
        J->>DB: store the proof (status proven)
    end
```

## Notes

- **The job takes queued rows, not one compilation ID.** So it also picks up rows that earlier runs left behind, for example after an auction timeout. Two executions at the same time take different rows (`SELECT … FOR UPDATE SKIP LOCKED`).
- **Local execution is free.** It gives the cycle count and the claim. The job pays only after the claim matches the code hashes in the database.
- **The request is the step that costs money.** The job stores `request_id` before it waits. After a crash, the next run finds the row with status `requested` and calls `wait_proof` on the stored ID. It never sends a second request for the same compilation.
- **Two limits cap the cost.** `cycle_limit` comes from the local execution. `max_price_per_pgu` is the highest price per unit of prover gas. If no prover accepts the price, the request fails after the auction timeout and the row goes back to `queued`.
- **The bucket needs no trust.** The job computes the program key of the downloaded ELF and stops if it is not in the pinned list.
- **Cloud Run retries stay at 0.** The rows in the database control retries.
