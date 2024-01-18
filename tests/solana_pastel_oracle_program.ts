import * as anchor from '@coral-xyz/anchor';
import { Program, web3, AnchorProvider, BN} from '@coral-xyz/anchor';
import { SolanaPastelOracleProgram, IDL} from '../target/types/solana_pastel_oracle_program';
import { assert } from 'chai';
import * as crypto from 'crypto';
import bs58 from 'bs58';
const { ComputeBudgetProgram, Transaction } = anchor.web3;

process.env.ANCHOR_PROVIDER_URL = "http://127.0.0.1:8899";
process.env.RUST_LOG = "solana_runtime::system_instruction_processor=trace,solana_runtime::message_processor=trace,solana_bpf_loader=debug,solana_rbpf=debug";
const provider = AnchorProvider.env();
anchor.setProvider(provider);
const programID = new anchor.web3.PublicKey("AfP1c4sFcY1FeiGjQEtyxCim8BRnw22okNbKAsH2sBsB");
const program = new Program<SolanaPastelOracleProgram>(IDL, programID, provider);
const admin = provider.wallet; // Use the provider's wallet
const oracleContractState = web3.Keypair.generate();
const REGISTRATION_ENTRANCE_FEE_SOL = 0.1;
const NUM_CONTRIBUTORS = 10;
let contributors = []; // Array to store contributor keypairs
const txidToAdd = '9930511c526808e6849a25cb0eb6513f729c2a71ec51fbca084d7c7e4a8dea2f';
const COST_IN_SOL_OF_ADDING_PASTEL_TXID_FOR_MONITORING = 0.0001;
const MIN_REPORTS_FOR_REWARD = 10;
const MIN_COMPLIANCE_SCORE_FOR_REWARD = 80;
const BASE_REWARD_AMOUNT_IN_LAMPORTS = 100000;
const maxSize = 200 * 1024; // 200KB

const TxidStatusEnum = {
  Invalid: "Invalid",
  PendingMining: "PendingMining",
  MinedPendingActivation: "MinedPendingActivation",
  MinedActivated: "MinedActivated"
};

const PastelTicketTypeEnum = {
  Sense: "Sense",
  Cascade: "Cascade",
  Nft: "Nft",
  InferenceApi: "InferenceApi"
};

console.log("Program ID:", programID.toString());
console.log("Admin ID:", admin.publicKey.toString());

describe('Initialization', () => {
  it('Initializes and expands the oracle contract state', async () => {
    // Find the PDAs for the RewardPoolAccount and FeeReceivingContractAccount
    const [rewardPoolAccountPDA] = await web3.PublicKey.findProgramAddressSync(
      [Buffer.from("reward_pool")],
      program.programId
    );
    const [feeReceivingContractAccountPDA] = await web3.PublicKey.findProgramAddressSync(
      [Buffer.from("fee_receiving_contract")],
      program.programId
    );

    // Calculate the rent-exempt minimum balance for the account size
    const minBalanceForRentExemption = await provider.connection.getMinimumBalanceForRentExemption(100 * 1024); // 100KB
    console.log("Minimum Balance for Rent Exemption:", minBalanceForRentExemption);

    // Fund the oracleContractState account with enough SOL for rent exemption
    console.log("Funding Oracle Contract State account for rent exemption");
    const fundTx = new anchor.web3.Transaction().add(
      anchor.web3.SystemProgram.transfer({
        fromPubkey: admin.publicKey,
        toPubkey: oracleContractState.publicKey,
        lamports: minBalanceForRentExemption,
      })
    );
    await provider.sendAndConfirm(fundTx);

    // Initial Initialization
    console.log("Initializing Oracle Contract State");
    await program.methods.initialize(admin.publicKey)
      .accounts({
        oracleContractState: oracleContractState.publicKey,
        user: admin.publicKey,
        rewardPoolAccount: rewardPoolAccountPDA,
        feeReceivingContractAccount: feeReceivingContractAccountPDA,
        systemProgram: web3.SystemProgram.programId,
      })
      .signers([oracleContractState])
      .rpc();

    let state = await program.account.oracleContractState.fetch(oracleContractState.publicKey);
    assert.ok(state.isInitialized, "Oracle Contract State should be initialized after first init");
    assert.equal(state.adminPubkey.toString(), admin.publicKey.toString(), "Admin public key should match after first init");

    // Incremental Reallocation
    let currentSize = 10_240;   // Initial size after first init

    while (currentSize < maxSize) {
      console.log(`Expanding Oracle Contract State size from ${currentSize} to ${currentSize + 10_240}`);
      await program.methods.reallocateOracleState()
        .accounts({
          oracleContractState: oracleContractState.publicKey,
          adminPubkey: admin.publicKey
        })
        .rpc();

      currentSize += 10_240;
      state = await program.account.oracleContractState.fetch(oracleContractState.publicKey);

      // Log the updated size of the account
      console.log(`Oracle Contract State size after expansion: ${currentSize}`);
    }

    // Final Assertions
    assert.equal(currentSize, maxSize, "Oracle Contract State should reach the maximum size");
    console.log("Oracle Contract State expanded to the maximum size successfully");
  });
});


describe('Set Bridge Contract', () => {
  it('Sets the bridge contract address to admin address', async () => {
    await program.methods.setBridgeContract(admin.publicKey)
      .accounts({
        oracleContractState: oracleContractState.publicKey,
        adminPubkey: admin.publicKey,
      })
      .rpc();

    // Fetch the updated state to verify the bridge contract address
    const state = await program.account.oracleContractState.fetch(oracleContractState.publicKey);

    // Assertions
    assert.strictEqual(state.bridgeContractPubkey.toString(), admin.publicKey.toString(), 'The bridge contract pubkey should be set to the admin address');
    console.log('Bridge contract address set to admin address');
  });
});


describe('Contributor Registration', () => {
  it('Registers new data contributors', async () => {

    // Find the PDAs for the RewardPoolAccount and FeeReceivingContractAccount
    const [rewardPoolAccountPDA] = await web3.PublicKey.findProgramAddressSync(
      [Buffer.from("reward_pool")],
      program.programId
    );
    const [feeReceivingContractAccountPDA] = await web3.PublicKey.findProgramAddressSync(
      [Buffer.from("fee_receiving_contract")],
      program.programId
    );

    for (let i = 0; i < NUM_CONTRIBUTORS; i++) {
      // Generate a new keypair for each contributor
      const contributor = web3.Keypair.generate();

      // Transfer the registration fee to feeReceivingContractAccount PDA
      const transaction = new web3.Transaction().add(
        web3.SystemProgram.transfer({
          fromPubkey: admin.publicKey,
          toPubkey: feeReceivingContractAccountPDA,
          lamports: REGISTRATION_ENTRANCE_FEE_SOL * web3.LAMPORTS_PER_SOL,
        })
      );

      // Sign and send the transaction
      await provider.sendAndConfirm(transaction);

      // Call the RPC method to register the new data contributor
      await program.methods.registerNewDataContributor()
        .accounts({
          oracleContractState: oracleContractState.publicKey,
          contributorAccount: contributor.publicKey,
          rewardPoolAccount: rewardPoolAccountPDA,
          feeReceivingContractAccount: feeReceivingContractAccountPDA,
        })
        .signers([contributor])
        .rpc();

      console.log(`Contributor ${i + 1} registered successfully with the address:`, contributor.publicKey.toBase58());
      contributors.push(contributor);
    }

    // Fetch the updated state to verify all contributors are registered
    const state = await program.account.oracleContractState.fetch(oracleContractState.publicKey);
    console.log('Total number of registered contributors:', state.contributors.length);

    // Verify each contributor is registered
    contributors.forEach((contributor, index) => {
      const isRegistered = state.contributors.some(c => c.rewardAddress.equals(contributor.publicKey));
      assert.isTrue(isRegistered, `Contributor ${index + 1} should be registered`);
    });
  });
});


describe('TXID Monitoring', () => {
  it('Adds multiple TXIDs for monitoring', async () => {
    // Define the number of TXIDs to add for monitoring
    const numTxids = MIN_REPORTS_FOR_REWARD;

    // Helper function to generate a random TXID
    const generateRandomTxid = () => {
      return [...Array(64)].map(() => Math.floor(Math.random() * 16).toString(16)).join('');
    };

    for (let i = 0; i < numTxids; i++) {
      const txid = generateRandomTxid();

      const expectedAmountLamports = COST_IN_SOL_OF_ADDING_PASTEL_TXID_FOR_MONITORING * web3.LAMPORTS_PER_SOL;
      const expectedAmountStr = expectedAmountLamports.toString();

      const preimageString = "pending_payment" + txid + admin.publicKey.toBase58();
      const preimageBytes = Buffer.from(preimageString, 'utf8');
      const seedHash = crypto.createHash('sha256').update(preimageBytes).digest();
      const [pendingPaymentAccountPDA] = web3.PublicKey.findProgramAddressSync([seedHash], program.programId);

      await program.methods.addPendingPayment(txid, expectedAmountStr, "Pending")
        .accounts({
          pendingPaymentAccount: pendingPaymentAccountPDA,
          oracleContractState: oracleContractState.publicKey,
          user: admin.publicKey,
          systemProgram: web3.SystemProgram.programId
        })
        .rpc();

      await program.methods.addTxidForMonitoring({ txid: txid })
        .accounts({
          oracleContractState: oracleContractState.publicKey,
          caller: admin.publicKey,
          pendingPaymentAccount: pendingPaymentAccountPDA,
          user: admin.publicKey,
          systemProgram: web3.SystemProgram.programId
        })
        .rpc();

      // Fetch the updated state
      const state = await program.account.oracleContractState.fetch(oracleContractState.publicKey);
      const pendingPaymentData = await program.account.pendingPaymentAccount.fetch(pendingPaymentAccountPDA);

      // Assertions for each TXID
      assert(state.monitoredTxids.includes(txid), `The TXID ${txid} should be added to the monitored list`);
      assert.strictEqual(pendingPaymentData.pendingPayment.expectedAmount.toNumber(), expectedAmountLamports, `The expected amount for pending payment for TXID ${txid} should match`);
      console.log(`TXID ${txid} successfully added for monitoring`);
    }
  });
});



describe('Data Report Submission', () => {
  it('Submits multiple data reports for different TXIDs with consensus and dissent', async () => {
    const seedPreamble = "pastel_tx_status_report";

    // Transfer SOL to each contributor
    const transferAmountSOL = 1.0;
    for (const contributor of contributors) {
      const transferTransaction = new anchor.web3.Transaction().add(
        anchor.web3.SystemProgram.transfer({
          fromPubkey: admin.publicKey,
          toPubkey: contributor.publicKey,
          lamports: transferAmountSOL * anchor.web3.LAMPORTS_PER_SOL,
        })
      );

      await provider.sendAndConfirm(transferTransaction);
      console.log(`Transferred ${transferAmountSOL} SOL to contributor account with address ${contributor.publicKey.toBase58()}`);
    }

    // Fetch monitored TXIDs from the updated state
    const state = await program.account.oracleContractState.fetch(oracleContractState.publicKey);
    const monitoredTxids = state.monitoredTxids;

    // Loop through each monitored TXID
    for (const txid of monitoredTxids) {
      // Generate a random file hash for this TXID
      const randomFileHash = [...Array(6)].map(() => Math.floor(Math.random() * 16).toString(16)).join('');
      console.log(`Random file hash (first 6 characters) for TXID ${txid} generated as:`, randomFileHash);

      for (let i = 0; i < contributors.length; i++) {
        const contributor = contributors[i];
        const rewardAddress = contributor.publicKey;

        // Randomize the status value for each report
        const txidStatusValue = i < 8 ? TxidStatusEnum.MinedActivated : TxidStatusEnum.Invalid;
        const pastelTicketTypeValue = PastelTicketTypeEnum.Nft;
        console.log(`Status value for TXID ${txid} by contributor ${contributor.publicKey.toBase58()} is '${txidStatusValue}'; ticket type value is '${pastelTicketTypeValue}'`);

        const preimageString = seedPreamble + txid + rewardAddress.toBase58();
        console.log(`Preimage string for TXID ${txid} by contributor ${contributor.publicKey.toBase58()}:`, preimageString);

        const preimageBytes = Buffer.from(preimageString, 'utf8');
        
        const seedHash = crypto.createHash('sha256').update(preimageBytes).digest();
        console.log(`Seed hash for PDA for TXID ${txid} by contributor ${contributor.publicKey.toBase58()}:`, bs58.encode(seedHash));
        
        const [reportAccountPDA] = web3.PublicKey.findProgramAddressSync([seedHash], program.programId);
        console.log(`Report account PDA for TXID ${txid} by contributor ${contributor.publicKey.toBase58()}:`, reportAccountPDA.toBase58());


        // Create and send the transaction
        try {
          const transaction = new Transaction();
          transaction.add(ComputeBudgetProgram.setComputeUnitLimit({ units: 1400000 }));

          const submitDataReportInstruction = await program.methods.submitDataReport(
            txid,
            txidStatusValue,
            pastelTicketTypeValue,
            randomFileHash,
            contributor.publicKey
          )
          .accounts({
            reportAccount: reportAccountPDA,
            oracleContractState: oracleContractState.publicKey,
            user: contributor.publicKey,
            systemProgram: web3.SystemProgram.programId,
          })
          .signers([contributor])
          .instruction();

          transaction.add(submitDataReportInstruction);
          await provider.sendAndConfirm(transaction, [contributor]);
          console.log(`Data report for TXID ${txid} submitted by contributor ${contributor.publicKey.toBase58()}`);
        } catch (error) {
          console.error(`Error submitting report for TXID ${txid} by contributor ${contributor.publicKey.toBase58()}:`, error);
          throw error;
        }
      }

      // Fetch the updated state and verify consensus data for each TXID
      const updatedState = await program.account.oracleContractState.fetch(oracleContractState.publicKey);
      const consensusData = updatedState.aggregatedConsensusData.find(data => data.txid === txid);

      if (consensusData) {
        const consensusStatusIndex = consensusData.statusWeights.indexOf(Math.max(...consensusData.statusWeights));
        const consensusStatus = ['Invalid', 'PendingMining', 'MinedPendingActivation', 'MinedActivated'][consensusStatusIndex];
        console.log(`Consensus Status for TXID ${txid}:`, consensusStatus);
      } else {
        console.log(`No consensus data found for TXID ${txid}`);
      }
    }

    // Loop through each monitored TXID for validation
    for (const txid of monitoredTxids) {
      // Fetch the updated state after all submissions for this TXID
      const updatedState = await program.account.oracleContractState.fetch(oracleContractState.publicKey);
      console.log(`Updated Oracle Contract State for TXID ${txid}:`, updatedState);

      // Check if the txid is in the monitored list
      assert(updatedState.monitoredTxids.includes(txid), `TXID ${txid} should be in the monitored list`);

      // Verify the consensus data for the TXID
      const consensusData = updatedState.aggregatedConsensusData.find(data => data.txid === txid);
      assert(consensusData !== undefined, `Consensus data should be present for the TXID ${txid}`);

      // Assuming the consensus is based on the majority rule
      const consensusStatusIndex = consensusData.statusWeights.indexOf(Math.max(...consensusData.statusWeights));
      const consensusStatus = ['Invalid', 'PendingMining', 'MinedPendingActivation', 'MinedActivated'][consensusStatusIndex];
      console.log(`Consensus Status for TXID ${txid}:`, consensusStatus);

      // Check if the majority consensus is achieved for 'MinedActivated'
      assert(consensusStatus === 'MinedActivated', `Majority consensus for TXID ${txid} should be 'MinedActivated'`);

      // Check for the hash with the highest weight
      const consensusHash = consensusData.hashWeights.reduce((max, h) => max.weight > h.weight ? max : h, { hash: '', weight: -1 }).hash;
      console.log(`Consensus Hash for TXID ${txid}:`, consensusHash);

      // Add further checks if needed based on the contract's consensus logic
      console.log(`Data report submission verification successful for the TXID: ${txid}`);
    }
  });
});


// describe('Eligibility for Rewards', () => {
//   it('should check if contributors meet reward eligibility criteria', async () => {
//     // Fetch the updated state after all submissions
//     const state = await program.account.oracleContractState.fetch(oracleContractState.publicKey);

//     // Loop through each contributor and check their eligibility
//     for (const contributor of contributors) {
//       const contributorData = state.contributors.find(c => c.rewardAddress.equals(contributor.publicKey));

//       // Define your eligibility criteria based on your contract logic
//       const isEligibleForRewards = contributorData.totalReportsSubmitted >= MIN_REPORTS_FOR_REWARD 
//                                     && contributorData.reliabilityScore >= 80 
//                                     && contributorData.complianceScore >= MIN_COMPLIANCE_SCORE_FOR_REWARD;

//       assert(isEligibleForRewards, `Contributor with address ${contributor.publicKey.toBase58()} should be eligible for rewards`);
//     }
//   });
// });


// describe('Reward Distribution', () => {
//   it('should distribute rewards correctly from the reward pool', async () => {
//     // Choose an eligible contributor
//     const eligibleContributor = contributors[0]; // Assuming the first contributor is eligible

//     // Find the PDA for the RewardPoolAccount
//     const [rewardPoolAccountPDA] = await web3.PublicKey.findProgramAddressSync(
//       [Buffer.from("reward_pool")],
//       program.programId
//     );
    
//     // Get initial balance of the reward pool
//     const initialRewardPoolBalance = await provider.connection.getBalance(rewardPoolAccountPDA);

//     // Request reward for the eligible contributor
//     await program.methods.requestReward(eligibleContributor.publicKey)
//       .accounts({
//         rewardPoolAccount: rewardPoolAccountPDA,
//         oracleContractState: oracleContractState.publicKey
//       })
//       .rpc();

//     // Get updated balance of the reward pool
//     const updatedRewardPoolBalance = await provider.connection.getBalance(rewardPoolAccountPDA);

//     // Check if the balance is deducted correctly
//     const expectedBalanceAfterReward = initialRewardPoolBalance - BASE_REWARD_AMOUNT_IN_LAMPORTS;
//     assert.equal(updatedRewardPoolBalance, expectedBalanceAfterReward, 'Reward pool balance should be deducted by the reward amount');
//   });
// });


// describe('Request Reward for Ineligible Contributor', () => {
//   it('should not allow reward requests from ineligible contributors', async () => {
//     // Choose an ineligible contributor
//     const ineligibleContributor = contributors[contributors.length - 1]; // Assuming the last contributor is ineligible

//     // Find the PDA for the RewardPoolAccount
//     const [rewardPoolAccountPDA] = await web3.PublicKey.findProgramAddressSync(
//       [Buffer.from("reward_pool")],
//       program.programId
//     );

//     try {
//       // Attempt to request reward
//       await program.methods.requestReward(ineligibleContributor.publicKey)
//         .accounts({
//           rewardPoolAccount: rewardPoolAccountPDA,
//           oracleContractState: oracleContractState.publicKey
//         })
//         .rpc();

//       throw new Error('Reward request should have failed for ineligible contributor');
//     } catch (error) {
//       // Check for the specific error thrown by your program
//       assert.equal(error.msg, 'NotEligibleForReward', 'Should throw NotEligibleForReward error');
//     }
//   });
// });
