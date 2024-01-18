import * as anchor from '@coral-xyz/anchor';
import { Program, web3, AnchorProvider, BN} from '@coral-xyz/anchor';
import { SolanaPastelOracleProgram, IDL} from '../target/types/solana_pastel_oracle_program';
import { assert } from 'chai';
import * as crypto from 'crypto';
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
    const maxSize = 100 * 1024; // 100KB
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
  it('Adds a new TXID for monitoring', async () => {
    // Setup

    const expectedAmountLamports = COST_IN_SOL_OF_ADDING_PASTEL_TXID_FOR_MONITORING * web3.LAMPORTS_PER_SOL;
    const expectedAmountStr = expectedAmountLamports.toString();

    // Concatenate "pending_payment", txidToAdd, and the Base58 string of admin's public key
    const preimageString = "pending_payment" + txidToAdd + admin.publicKey.toBase58();
    console.log('Preimage String:', preimageString);

    // Convert the concatenated string to bytes using UTF-8 encoding
    const preimageBytes = Buffer.from(preimageString, 'utf8');

    const seedHash = crypto.createHash('sha256').update(preimageBytes).digest();

    // Find the PDA for pendingPaymentAccount using the hashed seed
    const [pendingPaymentAccountPDA, pendingPaymentAccountBump] = web3.PublicKey.findProgramAddressSync(
      [seedHash],
      program.programId
    );

    await program.methods.addPendingPayment(txidToAdd, expectedAmountStr, "Pending")
    .accounts({
      pendingPaymentAccount: pendingPaymentAccountPDA,
      oracleContractState: oracleContractState.publicKey,
      user: admin.publicKey,
      systemProgram: web3.SystemProgram.programId
    })
    .rpc();

    // Invoke the add_txid_for_monitoring method
    await program.methods.addTxidForMonitoring({ txid: txidToAdd })
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

    // Assertions
    assert(state.monitoredTxids.includes(txidToAdd), 'The TXID should be added to the monitored list');
    assert.strictEqual(pendingPaymentData.pendingPayment.expectedAmount.toNumber(), expectedAmountLamports, 'The expected amount for pending payment should match');
    console.log('TXID successfully added for monitoring');
  });
});


describe('Data Report Submission', () => {
  it('Submits data reports for a monitored TXID with consensus and dissent', async () => {
    // Define the monitored TXID
    const seedPreamble = "pastel_tx_status_report";
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

    // Transfer SOL to each contributor
    const transferAmountSOL = 0.1;
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

     // Submit reports for each contributor
    for (let i = 0; i < contributors.length; i++) {
      const contributor = contributors[i];
      const rewardAddress = contributor.publicKey;
      const txidStatusValue = i < 8 ? TxidStatusEnum.MinedActivated : TxidStatusEnum.Invalid;
      const pastelTicketTypeValue = PastelTicketTypeEnum.Nft;

      const preimageString = seedPreamble + txidToAdd + rewardAddress.toBase58();
      const preimageBytes = Buffer.from(preimageString, 'utf8');
      const seedHash = crypto.createHash('sha256').update(preimageBytes).digest();
      const [reportAccountPDA] = web3.PublicKey.findProgramAddressSync([seedHash], program.programId);
      
      try {
        // Create a new transaction
        const transaction = new Transaction();

        // Add instruction to increase the compute budget
        transaction.add(ComputeBudgetProgram.setComputeUnitLimit({
          units: 1400000 // Setting the compute budget to 1.4M CU
        }));

        // Prepare the instruction for submitDataReport
        const submitDataReportInstruction = await program.methods.submitDataReport(
          txidToAdd,
          txidStatusValue,
          pastelTicketTypeValue,
          'abcdef',
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

        // Add the submitDataReport instruction to the transaction
        transaction.add(submitDataReportInstruction);

        // Send the transaction with increased compute budget
        await provider.sendAndConfirm(transaction, [contributor]);
        console.log(`Data report submitted by contributor ${i + 1}`);
      } catch (error) {
        console.error(`Error submitting report for contributor ${i + 1}:`, error);
        throw error; // Rethrow to fail the test
      }
    }

    // Fetch the updated state after all submissions
    const updatedState = await program.account.oracleContractState.fetch(oracleContractState.publicKey);
    console.log('Updated Oracle Contract State:', updatedState);

    // Check if the txid is in the monitored list
    assert(updatedState.monitoredTxids.includes(txidToAdd), "TXID should be in the monitored list");

    // Verify the consensus data for the TXID
    const consensusData = updatedState.aggregatedConsensusData.find(data => data.txid === txidToAdd);
    assert(consensusData !== undefined, "Consensus data should be present for the TXID");

    // Assuming the consensus is based on the majority rule
    const consensusStatusIndex = consensusData.statusWeights.indexOf(Math.max(...consensusData.statusWeights));
    const consensusStatus = ['Invalid', 'PendingMining', 'MinedPendingActivation', 'MinedActivated'][consensusStatusIndex];
    console.log('Consensus Status:', consensusStatus);

    // Check if the majority consensus is achieved for 'MinedActivated'
    assert(consensusStatus === 'MinedActivated', "Majority consensus should be 'MinedActivated'");

    // Check for the hash with the highest weight (assuming this logic is in your contract)
    const consensusHash = consensusData.hashWeights.reduce((max, h) => max.weight > h.weight ? max : h, { hash: '', weight: -1 }).hash;
    console.log('Consensus Hash:', consensusHash);

    // Add further checks if needed based on the contract's consensus logic

    console.log('Data report submission verification successful for the TXID:', txidToAdd);

  });
});

