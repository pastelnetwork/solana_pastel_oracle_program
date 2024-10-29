import { assert, expect } from "chai";
import Decimal from "decimal.js";
import * as crypto from "crypto";
import * as anchor from "@coral-xyz/anchor";
import { Program, web3, AnchorProvider, BN } from "@coral-xyz/anchor";
import { ComputeBudgetProgram, SystemProgram } from "@solana/web3.js";
import { SolanaPastelOracleProgram } from "../target/types/solana_pastel_oracle_program";
import IDL from "../target/idl/solana_pastel_oracle_program.json";

const TURN_ON_STORAGE_AND_COMPUTE_PROFILING = true; // Set this flag to true to enable profiling
process.env.ANCHOR_PROVIDER_URL = "http://127.0.0.1:8899";
process.env.RUST_LOG =
  "solana_runtime::system_instruction_processor=trace,solana_runtime::message_processor=trace,solana_bpf_loader=debug,solana_rbpf=debug";
const provider = AnchorProvider.env();
anchor.setProvider(provider);
const program = new Program<SolanaPastelOracleProgram>(IDL as any, provider);
const admin = provider.wallet; // Use the provider's wallet
const oracleContractState = web3.Keypair.generate();
let contributors = []; // Array to store contributor keypairs
let trackedTxids = []; // Initialize an empty array to track TXIDs

const maxSize = 100 * 1024; // 200KB (max size of the oracle contract state account)

const NUM_CONTRIBUTORS = 12;
const NUMBER_OF_SIMULATED_REPORTS = 20;

const REGISTRATION_ENTRANCE_FEE_SOL = 0.1;
const COST_IN_SOL_OF_ADDING_PASTEL_TXID_FOR_MONITORING = 0.0001;
const MIN_NUMBER_OF_ORACLES = 8;
const MIN_REPORTS_FOR_REWARD = 10;
const BAD_CONTRIBUTOR_INDEX = 5; // Define a constant to represent the index at which contributors start submitting incorrect reports with increasing probability
const MIN_COMPLIANCE_SCORE_FOR_REWARD = new BN(65_000000000);
const MIN_RELIABILITY_SCORE_FOR_REWARD = new BN(80_000000000);
const BASE_REWARD_AMOUNT_IN_LAMPORTS = 100000;

const TxidStatusEnum = {
  Invalid: "invalid",
  PendingMining: "pendingMining",
  MinedPendingActivation: "minedPendingActivation",
  MinedActivated: "minedActivated",
};

const PastelTicketTypeEnum = {
  Sense: "sense",
  Cascade: "cascade",
  Nft: "nft",
  InferenceApi: "inferenceApi",
};

let totalComputeUnitsUsed = 0;
let maxAccountStorageUsed = 0;

console.log("Program ID:", program.programId.toString());
console.log("Admin ID:", admin.publicKey.toString());

const measureComputeUnitsAndStorage = async (txSignature: string) => {
  if (!TURN_ON_STORAGE_AND_COMPUTE_PROFILING) return;

  // Retry logic to handle cases where the transaction might not be immediately available
  for (let attempts = 0; attempts < 5; attempts++) {
    const txDetails = await provider.connection.getParsedTransaction(
      txSignature,
      { commitment: "confirmed" }
    );
    if (txDetails) {
      if (txDetails.meta && txDetails.meta.computeUnitsConsumed) {
        totalComputeUnitsUsed += txDetails.meta.computeUnitsConsumed;
      }

      const accounts = txDetails.transaction.message.accountKeys.map(
        (key) => new web3.PublicKey(key.pubkey)
      );
      for (const account of accounts) {
        const accountInfo = await provider.connection.getAccountInfo(account);
        if (accountInfo && accountInfo.data.length > maxAccountStorageUsed) {
          maxAccountStorageUsed = accountInfo.data.length;
        }
      }
      return; // Exit if transaction details are successfully processed
    }
    // Wait a bit before retrying
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  console.error(
    `Failed to fetch transaction details for signature: ${txSignature}`
  );
};

describe("Initialization", () => {
  it("Initializes and expands the oracle contract state", async () => {
    // Find the PDAs for all accounts
    const [rewardPoolAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("reward_pool")],
      program.programId
    );
    const [feeReceivingContractAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("fee_receiving_contract")],
      program.programId
    );
    const [contributorDataAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("contributor_data")],
      program.programId
    );
    const [txidSubmissionCountsAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("txid_submission_counts")],
      program.programId
    );
    const [aggregatedConsensusDataAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("aggregated_consensus_data")],
      program.programId
    );
    const [tempReportAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("temp_tx_status_report")],
      program.programId
    );

    // Calculate the rent-exempt minimum balance for the account size
    const minBalanceForRentExemption = await provider.connection.getMinimumBalanceForRentExemption(100 * 1024);
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
    const fundTxSignature = await provider.sendAndConfirm(fundTx);
    await measureComputeUnitsAndStorage(fundTxSignature);

    // Step 1: Initialize main state
    console.log("Initializing Oracle Contract State");
    const initMainTxSignature = await program.methods
      .initialize()
      .accountsStrict({
        oracleContractState: oracleContractState.publicKey,
        user: admin.publicKey,
        systemProgram: web3.SystemProgram.programId,
      })
      .signers([oracleContractState])
      .rpc();
    await measureComputeUnitsAndStorage(initMainTxSignature);

    // Verify main state initialization
    let state = await program.account.oracleContractState.fetch(
      oracleContractState.publicKey
    );
    assert.ok(
      state.isInitialized,
      "Oracle Contract State should be initialized after first init"
    );
    assert.equal(
      state.adminPubkey.toString(),
      admin.publicKey.toString(),
      "Admin public key should match after first init"
    );

    // Step 2: Initialize PDAs
    console.log("Initializing PDA accounts");
    const initPDAsTxSignature = await program.methods
      .initializePdas()
      .accountsStrict({
        oracleContractState: oracleContractState.publicKey,
        user: admin.publicKey,
        tempReportAccount: tempReportAccountPDA,
        contributorDataAccount: contributorDataAccountPDA,
        txidSubmissionCountsAccount: txidSubmissionCountsAccountPDA,
        aggregatedConsensusDataAccount: aggregatedConsensusDataAccountPDA,
        systemProgram: web3.SystemProgram.programId,
      })
      .rpc();
    await measureComputeUnitsAndStorage(initPDAsTxSignature);

    // Verify PDA initialization
    const tempReportAccount = await program.account.tempTxStatusReportAccount.fetch(
      tempReportAccountPDA
    );
    const contributorDataAccount = await program.account.contributorDataAccount.fetch(
      contributorDataAccountPDA
    );
    const txidSubmissionCountsAccount = await program.account.txidSubmissionCountsAccount.fetch(
      txidSubmissionCountsAccountPDA
    );
    const aggregatedConsensusDataAccount = await program.account.aggregatedConsensusDataAccount.fetch(
      aggregatedConsensusDataAccountPDA
    );

    // Verify all PDAs are properly initialized
    assert.ok(Array.isArray(tempReportAccount.reports), "TempReportAccount reports should be initialized as empty array");
    assert.ok(Array.isArray(contributorDataAccount.contributors), "ContributorDataAccount contributors should be initialized as empty array");
    assert.ok(Array.isArray(txidSubmissionCountsAccount.submissionCounts), "TxidSubmissionCountsAccount counts should be initialized as empty array");
    assert.ok(Array.isArray(aggregatedConsensusDataAccount.consensusData), "AggregatedConsensusDataAccount data should be initialized as empty array");

    // Incremental Reallocation
    let currentSize = 10_240; // Initial size after first init

    while (currentSize < maxSize) {
      console.log(
        `Expanding Oracle Contract State size from ${currentSize} to ${
          currentSize + 10_240
        }`
      );
      const reallocateTxSignature = await program.methods
        .reallocateOracleState()
        .accountsStrict({
          oracleContractState: oracleContractState.publicKey,
          adminPubkey: admin.publicKey,
          tempReportAccount: tempReportAccountPDA,
          contributorDataAccount: contributorDataAccountPDA,
          txidSubmissionCountsAccount: txidSubmissionCountsAccountPDA,
          aggregatedConsensusDataAccount: aggregatedConsensusDataAccountPDA,
          systemProgram: web3.SystemProgram.programId,
        })
        .rpc();
      await measureComputeUnitsAndStorage(reallocateTxSignature);

      currentSize += 10_240;
      state = await program.account.oracleContractState.fetch(
        oracleContractState.publicKey
      );

      console.log(`Oracle Contract State size after expansion: ${currentSize}`);
    }

    // Final Assertions
    assert.equal(
      currentSize,
      maxSize,
      "Oracle Contract State should reach the maximum size"
    );
    console.log("Oracle Contract State expanded to the maximum size successfully");
  });
});

describe("Reinitialization Prevention", () => {
  it("Prevents reinitialization of OracleContractState and all PDAs", async () => {
    const [rewardPoolAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("reward_pool")],
      program.programId
    );
    const [feeReceivingContractAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("fee_receiving_contract")],
      program.programId
    );
    const [contributorDataAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("contributor_data")],
      program.programId
    );
    const [txidSubmissionCountsAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("txid_submission_counts")],
      program.programId
    );
    const [aggregatedConsensusDataAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("aggregated_consensus_data")],
      program.programId
    );
    const [tempReportAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("temp_tx_status_report")],
      program.programId
    );

    try {
      // Try to reinitialize main state
      const initMainTxSignature = await program.methods
        .initialize()
        .accountsStrict({
          oracleContractState: oracleContractState.publicKey,
          user: admin.publicKey,
          systemProgram: web3.SystemProgram.programId,
        })
        .rpc();
      
      await measureComputeUnitsAndStorage(initMainTxSignature);
      assert.fail("Main state reinitialization should have failed");
    } catch (error) {
      if (error instanceof anchor.AnchorError) {
        const anchorError = error as anchor.AnchorError;
        assert.equal(
          anchorError.error.errorCode.code,
          "AccountAlreadyInitialized",
          "Should throw AccountAlreadyInitialized error for main state"
        );
      }
    }

    try {
      // Try to reinitialize PDAs
      const initPDAsTxSignature = await program.methods
        .initializePdas()
        .accountsStrict({
          oracleContractState: oracleContractState.publicKey,
          user: admin.publicKey,
          tempReportAccount: tempReportAccountPDA,
          contributorDataAccount: contributorDataAccountPDA,
          txidSubmissionCountsAccount: txidSubmissionCountsAccountPDA,
          aggregatedConsensusDataAccount: aggregatedConsensusDataAccountPDA,
          systemProgram: web3.SystemProgram.programId,
        })
        .rpc();

      await measureComputeUnitsAndStorage(initPDAsTxSignature);
      assert.fail("PDA reinitialization should have failed");
    } catch (error) {
      if (error instanceof anchor.AnchorError) {
        const anchorError = error as anchor.AnchorError;
        assert.equal(
          anchorError.error.errorCode.code,
          "AccountAlreadyInitialized",
          "Should throw AccountAlreadyInitialized error for PDAs"
        );
      }
    }

    // Verify state remains unchanged
    const state = await program.account.oracleContractState.fetch(
      oracleContractState.publicKey
    );
    
    assert.ok(
      state.isInitialized,
      "Oracle Contract State should still be initialized"
    );
    assert.equal(
      state.adminPubkey.toString(),
      admin.publicKey.toString(),
      "Admin public key should remain unchanged"
    );

    console.log("Reinitialization prevention test completed successfully");
  });
});

describe("Set Bridge Contract", () => {
  it("Sets the bridge contract address to admin address", async () => {
    const setBridgeTxSignature = await program.methods
      .setBridgeContract(admin.publicKey)
      .accountsPartial({
        oracleContractState: oracleContractState.publicKey,
        adminPubkey: admin.publicKey,
      })
      .rpc();
    await measureComputeUnitsAndStorage(setBridgeTxSignature);

    // Fetch the updated state to verify the bridge contract address
    const state = await program.account.oracleContractState.fetch(
      oracleContractState.publicKey
    );

    // Assertions
    assert.strictEqual(
      state.bridgeContractPubkey.toString(),
      admin.publicKey.toString(),
      "The bridge contract pubkey should be set to the admin address"
    );
    console.log("Bridge contract address set to admin address");
  });
});

describe("Admin Access Control", () => {
  it("Prevents non-admin users from performing admin actions", async () => {
    // Generate an unauthorized user
    const unauthorizedAdmin = web3.Keypair.generate();

    // Find PDAs once to avoid repetition
    const [rewardPoolAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("reward_pool")],
      program.programId
    );
    const [feeReceivingContractAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("fee_receiving_contract")],
      program.programId
    );

    console.log("Funding unauthorized admin account...");
    // Fund the unauthorized admin with enough SOL for transactions
    const fundTx = new web3.Transaction().add(
      web3.SystemProgram.transfer({
        fromPubkey: admin.publicKey,
        toPubkey: unauthorizedAdmin.publicKey,
        lamports: 1 * web3.LAMPORTS_PER_SOL,
      })
    );

    const fundTxSignature = await provider.sendAndConfirm(fundTx);
    await measureComputeUnitsAndStorage(fundTxSignature);
    console.log(
      `Funded unauthorized admin account: ${unauthorizedAdmin.publicKey.toBase58()}`
    );

    // Test 1: Attempt to set bridge contract as unauthorized user
    console.log("Testing unauthorized bridge contract setting...");
    try {
      const newBridgePubkey = web3.Keypair.generate().publicKey;
      const setBridgeTxSignature = await program.methods
        .setBridgeContract(newBridgePubkey)
        .accountsStrict({
          oracleContractState: oracleContractState.publicKey,
          adminPubkey: unauthorizedAdmin.publicKey,
        })
        .signers([unauthorizedAdmin])
        .rpc();
      
      await measureComputeUnitsAndStorage(setBridgeTxSignature);
      assert.fail("Setting bridge contract should have failed for unauthorized admin");
    } catch (error) {
      const anchorError = anchor.AnchorError.parse(error.logs);
      if (anchorError) {
        assert.equal(
          anchorError.error.errorCode.code,
          "UnauthorizedWithdrawalAccount",
          "Should throw UnauthorizedWithdrawalAccount error when non-admin sets bridge contract"
        );
        console.log("Unauthorized bridge contract setting was correctly rejected");
      } else {
        throw error;
      }
    }

    // Test 2: Attempt to withdraw funds as unauthorized user
    console.log("Testing unauthorized withdrawal...");
    try {
      const withdrawTxSignature = await program.methods
        .withdrawFunds(new BN(1000), new BN(1000))
        .accountsStrict({
          oracleContractState: oracleContractState.publicKey,
          adminAccount: unauthorizedAdmin.publicKey,
          rewardPoolAccount: rewardPoolAccountPDA,
          feeReceivingContractAccount: feeReceivingContractAccountPDA,
          systemProgram: web3.SystemProgram.programId,
        })
        .signers([unauthorizedAdmin])
        .rpc();
      
      await measureComputeUnitsAndStorage(withdrawTxSignature);
      assert.fail("Withdrawal should have failed for unauthorized admin");
    } catch (error) {
      const anchorError = anchor.AnchorError.parse(error.logs);
      if (anchorError) {
        assert.equal(
          anchorError.error.errorCode.code,
          "UnauthorizedWithdrawalAccount",
          "Should throw UnauthorizedWithdrawalAccount error when non-admin withdraws funds"
        );
        console.log("Unauthorized withdrawal was correctly rejected");
      } else {
        throw error;
      }
    }

    // Test 3: Verify that admin access hasn't been compromised
    console.log("Verifying admin access remains intact...");
    const state = await program.account.oracleContractState.fetch(
      oracleContractState.publicKey
    );
    assert.ok(
      state.adminPubkey.equals(admin.publicKey),
      "Admin pubkey should remain unchanged after unauthorized attempts"
    );
    console.log("Admin access verification completed successfully");
  });
});

describe("Fee Handling Verification", () => {
  it("Ensures correct transfer of registration fees upon contributor registration", async () => {
    // Find PDAs for fee receiving and reward pool accounts
    const [feeReceivingContractAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("fee_receiving_contract")],
      program.programId
    );
    const [rewardPoolAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("reward_pool")],
      program.programId
    );
    const [contributorDataAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("contributor_data")],
      program.programId
    );

    // Generate a new contributor keypair
    const feeContributor = web3.Keypair.generate();
    console.log("Test contributor address:", feeContributor.publicKey.toBase58());

    // Fund the contributor with some SOL for transaction fees
    const fundingTx = new web3.Transaction().add(
      web3.SystemProgram.transfer({
        fromPubkey: admin.publicKey,
        toPubkey: feeContributor.publicKey,
        lamports: 0.1 * web3.LAMPORTS_PER_SOL, // 0.1 SOL for transaction fees
      })
    );
    const fundingTxSignature = await provider.sendAndConfirm(fundingTx);
    await measureComputeUnitsAndStorage(fundingTxSignature);

    // Record balances before any registration-related transfers
    const initialFeeReceivingBalance = await provider.connection.getBalance(
      feeReceivingContractAccountPDA
    );
    const initialRewardPoolBalance = await provider.connection.getBalance(
      rewardPoolAccountPDA
    );

    console.log("Initial fee receiving balance:", initialFeeReceivingBalance);
    console.log("Initial reward pool balance:", initialRewardPoolBalance);

    // Transfer the registration fee to the fee receiving contract
    const registrationFeeInLamports = REGISTRATION_ENTRANCE_FEE_SOL * web3.LAMPORTS_PER_SOL;
    const feeTx = new web3.Transaction().add(
      web3.SystemProgram.transfer({
        fromPubkey: admin.publicKey,
        toPubkey: feeReceivingContractAccountPDA,
        lamports: registrationFeeInLamports,
      })
    );
    const feeTxSignature = await provider.sendAndConfirm(feeTx);
    await measureComputeUnitsAndStorage(feeTxSignature);

    // Record intermediate balances after fee transfer but before registration
    const intermediateFeeReceivingBalance = await provider.connection.getBalance(
      feeReceivingContractAccountPDA
    );
    console.log("Intermediate fee receiving balance:", intermediateFeeReceivingBalance);

    // Register the contributor
    const registerTxSignature = await program.methods
      .registerNewDataContributor()
      .accountsStrict({
        contributorDataAccount: contributorDataAccountPDA,
        contributorAccount: feeContributor.publicKey,
        rewardPoolAccount: rewardPoolAccountPDA,
        feeReceivingContractAccount: feeReceivingContractAccountPDA,
        systemProgram: SystemProgram.programId,
      })
      .signers([feeContributor])
      .rpc();
    await measureComputeUnitsAndStorage(registerTxSignature);

    // Allow some time for the network to process the transaction
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Fetch final balances
    const finalFeeReceivingBalance = await provider.connection.getBalance(
      feeReceivingContractAccountPDA
    );
    const finalRewardPoolBalance = await provider.connection.getBalance(
      rewardPoolAccountPDA
    );

    console.log("Final fee receiving balance:", finalFeeReceivingBalance);
    console.log("Final reward pool balance:", finalRewardPoolBalance);

    // Define an acceptable margin for balance comparisons (0.001 SOL)
    const margin = 1_000_000;

    // Verify that 90% of the registration fee was transferred to the reward pool
    // and 10% remained in the fee receiving contract
    const expectedRewardPoolIncrease = registrationFeeInLamports * 0.1; // 10% of registration fee
    const expectedFeeReceivingBalance = registrationFeeInLamports * 0.9; // 90% of registration fee

    assert.approximately(
      finalFeeReceivingBalance,
      expectedFeeReceivingBalance,
      margin,
      "Fee receiving contract should retain 90% of the registration fee"
    );

    assert.approximately(
      finalRewardPoolBalance - initialRewardPoolBalance,
      expectedRewardPoolIncrease,
      margin,
      "Reward pool should increase by 10% of the registration fee"
    );

    // Verify the contributor was actually registered
    const contributorData = await program.account.contributorDataAccount.fetch(
      contributorDataAccountPDA
    );
    const registeredContributor = contributorData.contributors.find(
      (c) => c.rewardAddress.toBase58() === feeContributor.publicKey.toBase58()
    );
    assert(registeredContributor, "Contributor should be registered in the ContributorDataAccount");
    
    // Log final registration details
    console.log("Registration verification complete:");
    console.log("Total fee receiving balance change:", finalFeeReceivingBalance - initialFeeReceivingBalance);
    console.log("Total reward pool balance change:", finalRewardPoolBalance - initialRewardPoolBalance);
    console.log("Expected fee receiving balance:", expectedFeeReceivingBalance);
    console.log("Expected reward pool increase:", expectedRewardPoolIncrease);
  });
});

describe("Contributor Registration", () => {
  it("Registers new data contributors", async () => {
    // Find the PDAs for the RewardPoolAccount and FeeReceivingContractAccount
    const [rewardPoolAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("reward_pool")],
      program.programId
    );
    const [feeReceivingContractAccountPDA] =
      web3.PublicKey.findProgramAddressSync(
        [Buffer.from("fee_receiving_contract")],
        program.programId
      );

    const [contributorDataAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("contributor_data")],
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
      const transferTxSignature = await provider.sendAndConfirm(transaction);
      await measureComputeUnitsAndStorage(transferTxSignature);

      // Call the RPC method to register the new data contributor
      const registerTxSignature = await program.methods
        .registerNewDataContributor()
        .accountsPartial({
          contributorDataAccount: contributorDataAccountPDA,
          contributorAccount: contributor.publicKey,
          rewardPoolAccount: rewardPoolAccountPDA,
          feeReceivingContractAccount: feeReceivingContractAccountPDA,
          systemProgram: SystemProgram.programId,
        })
        .signers([contributor])
        .rpc();
      await measureComputeUnitsAndStorage(registerTxSignature);

      console.log(
        `Contributor ${i + 1} registered successfully with the address:`,
        contributor.publicKey.toBase58()
      );
      contributors.push(contributor);
    }

    // Fetch the ContributorDataAccount to verify all contributors are registered
    const contributorData = await program.account.contributorDataAccount.fetch(
      contributorDataAccountPDA
    );
    console.log(
      "Total number of registered contributors in ContributorDataAccount:",
      contributorData.contributors.length
    );

    // Verify each contributor is registered in ContributorDataAccount
    contributors.forEach((contributor, index) => {
      const isRegistered = contributorData.contributors.some((c) =>
        c.rewardAddress.equals(contributor.publicKey)
      );
      assert.isTrue(
        isRegistered,
        `Contributor ${
          index + 1
        } should be registered in ContributorDataAccount`
      );
    });
  });
});

describe("TXID Monitoring and Verification", () => {
  it("Adds and verifies multiple TXIDs for monitoring with parallel processing", async () => {
    const numTxids = NUMBER_OF_SIMULATED_REPORTS;
    const BATCH_SIZE = 4; // Process TXIDs in batches of 4

    // Helper function to generate a random TXID
    const generateRandomTxid = () => {
      return [...Array(64)]
        .map(() => Math.floor(Math.random() * 16).toString(16))
        .join("");
    };

    // Generate all TXIDs upfront
    const txids = Array(numTxids).fill(null).map(() => generateRandomTxid());
    const expectedAmountLamports = COST_IN_SOL_OF_ADDING_PASTEL_TXID_FOR_MONITORING * web3.LAMPORTS_PER_SOL;
    const expectedAmountStr = expectedAmountLamports.toString();

    // Process TXIDs in parallel batches
    for (let i = 0; i < txids.length; i += BATCH_SIZE) {
      const currentBatch = txids.slice(i, i + BATCH_SIZE);
      
      // Process batch in parallel
      await Promise.all(currentBatch.map(async (txid) => {
        // Create PDA for pending payment account
        const preimageString = "pending_payment" + txid + admin.publicKey.toBase58();
        const preimageBytes = Buffer.from(preimageString, "utf8");
        const seedHash = crypto.createHash("sha256").update(preimageBytes).digest();
        const [pendingPaymentAccountPDA] = web3.PublicKey.findProgramAddressSync(
          [seedHash],
          program.programId
        );

        // Add pending payment
        const addPendingPaymentTxSignature = await program.methods
          .addPendingPayment(txid, new BN(expectedAmountStr), { pending: {} })
          .accountsPartial({
            pendingPaymentAccount: pendingPaymentAccountPDA,
            oracleContractState: oracleContractState.publicKey,
            user: admin.publicKey,
            systemProgram: web3.SystemProgram.programId,
          })
          .rpc();
        await measureComputeUnitsAndStorage(addPendingPaymentTxSignature);

        // Add TXID for monitoring
        const addTxidTxSignature = await program.methods
          .addTxidForMonitoring({ txid: txid })
          .accountsPartial({
            oracleContractState: oracleContractState.publicKey,
            caller: admin.publicKey,
            pendingPaymentAccount: pendingPaymentAccountPDA,
            user: admin.publicKey,
            systemProgram: web3.SystemProgram.programId,
          })
          .rpc();
        await measureComputeUnitsAndStorage(addTxidTxSignature);

        // Immediately verify the pending payment account
        const pendingPaymentData = await program.account.pendingPaymentAccount.fetch(
          pendingPaymentAccountPDA
        );

        // Verify pending payment data
        assert.strictEqual(
          pendingPaymentData.pendingPayment.txid,
          txid,
          `The TXID in PendingPayment should match the monitored TXID: ${txid}`
        );
        
        assert.strictEqual(
          pendingPaymentData.pendingPayment.expectedAmount.toNumber(),
          expectedAmountLamports,
          `The expected amount in PendingPayment should match for TXID: ${txid}`
        );

        const paymentStatusJson = JSON.stringify(pendingPaymentData.pendingPayment.paymentStatus);
        assert.strictEqual(
          paymentStatusJson,
          JSON.stringify({ pending: {} }),
          `The payment status for TXID: ${txid} should be 'Pending'`
        );

        console.log(`TXID ${txid} added and verified successfully`);
      }));

      // Optional: Add a small delay between batches to prevent rate limiting
      if (i + BATCH_SIZE < txids.length) {
        await new Promise(resolve => setTimeout(resolve, 100));
      }
    }

    // Final verification of all TXIDs in oracle contract state
    const state = await program.account.oracleContractState.fetch(
      oracleContractState.publicKey
    );

    // Verify all TXIDs are in the monitored list
    txids.forEach(txid => {
      assert(
        state.monitoredTxids.includes(txid),
        `TXID ${txid} should be in the monitored list`
      );
    });

    console.log(`Successfully added and verified ${numTxids} TXIDs`);
  });
});

describe("Data Report Submission", () => {
  it("Submits multiple data reports for different TXIDs with consensus and dissent", async () => {
    const seedPreamble = "pastel_tx_status_report";

    // Transfer SOL to contributors in parallel
    const transferAmountSOL = 1.0;
    await Promise.all(
      contributors.map(async (contributor) => {
        const transferTransaction = new anchor.web3.Transaction().add(
          anchor.web3.SystemProgram.transfer({
            fromPubkey: admin.publicKey,
            toPubkey: contributor.publicKey,
            lamports: transferAmountSOL * anchor.web3.LAMPORTS_PER_SOL,
          })
        );

        await provider.sendAndConfirm(transferTransaction);
        console.log(
          `Transferred ${transferAmountSOL} SOL to contributor account with address ${contributor.publicKey.toBase58()}`
        );
      })
    );

    // Find PDAs once outside the loops
    const [contributorDataAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("contributor_data")],
      program.programId
    );

    const [txidSubmissionCountsAccountPDA] =
      web3.PublicKey.findProgramAddressSync(
        [Buffer.from("txid_submission_counts")],
        program.programId
      );

    const [aggregatedConsensusDataAccountPDA] =
      web3.PublicKey.findProgramAddressSync(
        [Buffer.from("aggregated_consensus_data")],
        program.programId
      );

    const [tempReportAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("temp_tx_status_report")],
      program.programId
    );

    // Fetch monitored TXIDs from the updated state
    const state = await program.account.oracleContractState.fetch(
      oracleContractState.publicKey
    );
    const monitoredTxids = state.monitoredTxids;

    // Process TXIDs in parallel batches
    const BATCH_SIZE = 4; // Adjust based on your needs
    for (let i = 0; i < monitoredTxids.length; i += BATCH_SIZE) {
      const currentBatch = monitoredTxids.slice(i, i + BATCH_SIZE);
      await Promise.all(
        currentBatch.map(async (txid) => {
          // Generate random file hash
          const randomFileHash = [...Array(6)]
            .map(() => Math.floor(Math.random() * 16).toString(16))
            .join("");

          console.log(
            `Processing batch for TXID ${txid} with hash ${randomFileHash}`
          );

          // Process contributors in parallel batches
          const CONTRIBUTOR_BATCH_SIZE = 4;
          for (
            let j = 0;
            j < contributors.length;
            j += CONTRIBUTOR_BATCH_SIZE
          ) {
            const contributorBatch = contributors.slice(
              j,
              j + CONTRIBUTOR_BATCH_SIZE
            );

            // Create all the submission promises for this batch
            const submissionPromises = contributorBatch.map(
              async (contributor, batchIndex) => {
                const contributorIndex = j + batchIndex;
                const rewardAddress = contributor.publicKey;

                // Calculate error probability
                const errorProbability =
                  contributorIndex < BAD_CONTRIBUTOR_INDEX
                    ? 0
                    : (contributorIndex - BAD_CONTRIBUTOR_INDEX + 1) /
                      (contributors.length - BAD_CONTRIBUTOR_INDEX);
                const isIncorrect = Math.random() < errorProbability;

                const txidStatusValue = isIncorrect
                  ? TxidStatusEnum.Invalid
                  : TxidStatusEnum.MinedActivated;
                const pastelTicketTypeValue = PastelTicketTypeEnum.Nft;

                const preimageString =
                  seedPreamble + txid + rewardAddress.toBase58();
                const preimageBytes = Buffer.from(preimageString, "utf8");
                const seedHash = crypto
                  .createHash("sha256")
                  .update(preimageBytes)
                  .digest();

                const [reportAccountPDA] =
                  web3.PublicKey.findProgramAddressSync(
                    [seedHash],
                    program.programId
                  );

                try {
                  const submitTxSignature = await program.methods
                    .submitDataReport(
                      txid,
                      isIncorrect ? { invalid: {} } : { minedActivated: {} },
                      { nft: {} }, // Using PastelTicketType.Nft directly
                      randomFileHash,
                      contributor.publicKey
                    )
                    .accountsPartial({
                      reportAccount: reportAccountPDA,
                      tempReportAccount: tempReportAccountPDA,
                      contributorDataAccount: contributorDataAccountPDA,
                      txidSubmissionCountsAccount:
                        txidSubmissionCountsAccountPDA,
                      aggregatedConsensusDataAccount:
                        aggregatedConsensusDataAccountPDA,
                      oracleContractState: oracleContractState.publicKey,
                      user: contributor.publicKey,
                      systemProgram: web3.SystemProgram.programId,
                    })
                    .preInstructions([
                      ComputeBudgetProgram.setComputeUnitLimit({
                        units: 1_400_000,
                      }),
                    ])
                    .signers([contributor])
                    .rpc();

                  await measureComputeUnitsAndStorage(submitTxSignature);
                  console.log(
                    `Data report for TXID ${txid} submitted by contributor ${contributor.publicKey.toBase58()}`
                  );
                } catch (error) {
                  handleSubmissionError(error, contributor, contributorIndex);
                }
              }
            );

            // Wait for all submissions in this batch to complete
            await Promise.all(submissionPromises);
          }

          // Verify consensus after all contributors have submitted
          await verifyConsensus(txid, aggregatedConsensusDataAccountPDA);
        })
      );
    }

    // Final validation of contributor states
    await validateContributorStates(contributorDataAccountPDA);
  });
});

// Utility function to handle submission errors
function handleSubmissionError(
  error: any,
  contributor: any,
  contributorIndex: number
) {
  const anchorError = anchor.AnchorError.parse(error.logs);
  if (anchorError) {
    if (
      anchorError.error.errorCode.code === "ContributorBanned" &&
      contributorIndex >= BAD_CONTRIBUTOR_INDEX
    ) {
      console.log(
        `Expected 'ContributorBanned' error for contributor ${contributor.publicKey.toBase58()}: ${
          anchorError.error.errorMessage
        }`
      );
    } else if (
      anchorError.error.errorCode.code === "EnoughReportsSubmittedForTxid" &&
      contributorIndex >= MIN_NUMBER_OF_ORACLES
    ) {
      console.log(
        `Expected 'EnoughReportsSubmittedForTxid' error for contributor ${contributor.publicKey.toBase58()}: ${
          anchorError.error.errorMessage
        }`
      );
    } else {
      console.error(
        `Unexpected error: ${
          anchorError.error.errorCode.code || "Unknown error"
        } - ${anchorError.error.errorMessage}`
      );
    }
  } else {
    console.error(`Error parsing error code: ${error.toString()}`);
  }
}

// Utility function to verify consensus for a TXID
async function verifyConsensus(
  txid: string,
  aggregatedConsensusDataAccountPDA: web3.PublicKey
) {
  const aggregatedConsensusDataAccount =
    await program.account.aggregatedConsensusDataAccount.fetch(
      aggregatedConsensusDataAccountPDA
    );

  const consensusData = aggregatedConsensusDataAccount.consensusData.find(
    (data) => data.txid === txid
  );

  if (consensusData) {
    const maxWeight = [...consensusData.statusWeights].sort((a, b) =>
      new Decimal(b.sub(a).toString()).div(new Decimal(1e9)).toDP(9).toNumber()
    )[0];

    const consensusStatusIndex = consensusData.statusWeights.findIndex(
      (weight) => weight.eq(maxWeight)
    );

    const consensusStatus = [
      "Invalid",
      "PendingMining",
      "MinedPendingActivation",
      "MinedActivated",
    ][consensusStatusIndex];

    console.log(`Consensus Status for TXID ${txid}:`, consensusStatus);
    assert(
      consensusStatus === "MinedActivated",
      `Majority consensus for TXID ${txid} should be 'MinedActivated'`
    );
  } else {
    console.log(`No consensus data found for TXID ${txid}`);
  }
}

// Utility function to validate final contributor states
async function validateContributorStates(
  contributorDataAccountPDA: web3.PublicKey
) {
  console.log(`Now checking the state of each contributor:`);
  const contributorData = await program.account.contributorDataAccount.fetch(
    contributorDataAccountPDA
  );

  await Promise.all(
    contributors.map(async (contributor) => {
      const currentContributorData = contributorData.contributors.find((c) =>
        c.rewardAddress.equals(contributor.publicKey)
      );

      if (!currentContributorData) {
        throw new Error(
          `Contributor data not found for address ${contributor.publicKey.toBase58()}`
        );
      }

      // Log contributor state
      console.log(`______________________________________________________`);
      console.log(`Contributor: ${contributor.publicKey.toBase58()}`);
      console.log(
        `Compliance Score: ${currentContributorData.complianceScore}`
      );
      console.log(
        `Total Reports Submitted: ${currentContributorData.totalReportsSubmitted}`
      );
      console.log(
        `Accurate Reports Count: ${currentContributorData.accurateReportsCount}`
      );
      console.log(`Current Streak: ${currentContributorData.currentStreak}`);
      console.log(
        `Reliability Score: ${currentContributorData.reliabilityScore}`
      );
      console.log(
        `Consensus Failures: ${currentContributorData.consensusFailures}`
      );
      console.log(`Ban Expiry: ${currentContributorData.banExpiry}`);
      console.log(
        `Is Eligible for Rewards: ${currentContributorData.isEligibleForRewards}`
      );
      console.log(
        `Is Recently Active: ${currentContributorData.isRecentlyActive}`
      );
      console.log(`Is Reliable: ${currentContributorData.isReliable}`);
    })
  );
}

describe("Data Cleanup Verification", () => {
  it("Verifies that data is cleaned up post-consensus", async () => {
    const txidsToCheck = trackedTxids;

    const [tempReportAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("temp_tx_status_report")],
      program.programId
    );

    const tempReportAccountData =
      await program.account.tempTxStatusReportAccount.fetch(
        tempReportAccountPDA
      );

    txidsToCheck.forEach((txid) => {
      const isTxidPresentInTempReport = tempReportAccountData.reports.some(
        (report) => {
          const commonDataIndex = report.commonDataRef.toNumber(); // Convert BN to number
          const commonData =
            tempReportAccountData.commonReports[commonDataIndex]; // Use converted index
          return commonData.txid === txid;
        }
      );
      assert.isFalse(
        isTxidPresentInTempReport,
        `TXID ${txid} should be cleaned up from TempTxStatusReportAccount`
      );
    });

    console.log(
      "Data cleanup post-consensus verification completed successfully."
    );
  });
});

describe("Payment Processing by Bridge Contract", () => {
  it("Processes payments for monitored TXIDs", async () => {
    const COST_IN_LAMPORTS_OF_ADDING_PASTEL_TXID_FOR_MONITORING = new BN(
      COST_IN_SOL_OF_ADDING_PASTEL_TXID_FOR_MONITORING * web3.LAMPORTS_PER_SOL
    );
    // Derive the FeeReceivingContractAccount PDA
    const [feeReceivingContractAccountPDA] =
      web3.PublicKey.findProgramAddressSync(
        [Buffer.from("fee_receiving_contract")],
        program.programId
      );

    // Fetch monitored TXIDs from the updated state
    const state = await program.account.oracleContractState.fetch(
      oracleContractState.publicKey
    );
    const monitoredTxids = state.monitoredTxids;

    for (const txid of monitoredTxids) {
      // Transfer the payment for the TXID from the admin account to the fee-receiving contract
      const transferTransaction = new anchor.web3.Transaction().add(
        anchor.web3.SystemProgram.transfer({
          fromPubkey: admin.publicKey,
          toPubkey: feeReceivingContractAccountPDA,
          lamports:
            COST_IN_LAMPORTS_OF_ADDING_PASTEL_TXID_FOR_MONITORING.toNumber(),
        })
      );

      const transferTxSignature = await provider.sendAndConfirm(
        transferTransaction
      );
      await measureComputeUnitsAndStorage(transferTxSignature);
      console.log(
        `Transferred payment for TXID ${txid} to fee-receiving contract`
      );

      // Manually create the seed for pendingPaymentAccountPDA
      const preimageString =
        "pending_payment" + txid + admin.publicKey.toBase58();
      const preimageBytes = Buffer.from(preimageString, "utf8");
      const seedHash = crypto
        .createHash("sha256")
        .update(preimageBytes)
        .digest();
      const [pendingPaymentAccountPDA] = web3.PublicKey.findProgramAddressSync(
        [seedHash],
        program.programId
      );

      // Process the payment in the oracle contract
      await program.methods
        .processPayment(
          txid,
          COST_IN_LAMPORTS_OF_ADDING_PASTEL_TXID_FOR_MONITORING
        )
        .accountsPartial({
          sourceAccount: admin.publicKey,
          oracleContractState: oracleContractState.publicKey,
          pendingPaymentAccount: pendingPaymentAccountPDA,
          systemProgram: web3.SystemProgram.programId,
        })
        .rpc();

      // Fetch the updated PendingPayment struct to verify the payment status
      const pendingPaymentData =
        await program.account.pendingPaymentAccount.fetch(
          pendingPaymentAccountPDA
        );

      // Convert paymentStatus to JSON and compare the stringified version
      const paymentStatusJson = JSON.stringify(
        pendingPaymentData.pendingPayment.paymentStatus
      );
      assert.strictEqual(
        paymentStatusJson,
        JSON.stringify({ received: {} }),
        `The payment status for TXID: ${txid} should be 'Received'`
      );

      console.log(`Payment processed for TXID ${txid}`);
    }
  });
});

describe("Eligibility for Rewards", () => {
  it("should check if contributors meet reward eligibility criteria", async () => {
    // Fetch the ContributorDataAccount
    const [contributorDataAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("contributor_data")],
      program.programId
    );

    const contributorData = await program.account.contributorDataAccount.fetch(
      contributorDataAccountPDA
    );

    // Loop through each contributor and check their eligibility
    for (const contributor of contributors) {
      const currentContributorData = contributorData.contributors.find((c) =>
        c.rewardAddress.equals(contributor.publicKey)
      );

      // Check if the contributor data exists
      if (!currentContributorData) {
        throw new Error(
          `Contributor data not found for address ${contributor.publicKey.toBase58()}`
        );
      }

      // Log all fields for each contributor
      console.log(`Contributor: ${contributor.publicKey.toBase58()}`);
      console.log(
        `Registration Entrance Fee Transaction Signature: ${currentContributorData.registrationEntranceFeeTransactionSignature}`
      );
      console.log(
        `Compliance Score: ${currentContributorData.complianceScore}`
      );
      console.log(
        `Last Active Timestamp: ${currentContributorData.lastActiveTimestamp}`
      );
      console.log(
        `Total Reports Submitted: ${currentContributorData.totalReportsSubmitted}`
      );
      console.log(
        `Accurate Reports Count: ${currentContributorData.accurateReportsCount}`
      );
      console.log(`Current Streak: ${currentContributorData.currentStreak}`);
      console.log(
        `Reliability Score: ${currentContributorData.reliabilityScore}`
      );
      console.log(
        `Consensus Failures: ${currentContributorData.consensusFailures}`
      );
      console.log(`Ban Expiry: ${currentContributorData.banExpiry}`);
      console.log(
        `Is Eligible for Rewards: ${currentContributorData.isEligibleForRewards}`
      );
      console.log(
        `Is Recently Active: ${currentContributorData.isRecentlyActive}`
      );
      console.log(`Is Reliable: ${currentContributorData.isReliable}`);
      console.log(`______________________________________________________`);

      // Define your eligibility criteria based on your contract logic
      const isEligibleForRewards =
        currentContributorData.totalReportsSubmitted >=
          MIN_REPORTS_FOR_REWARD &&
        currentContributorData.reliabilityScore.gte(
          MIN_RELIABILITY_SCORE_FOR_REWARD
        ) &&
        currentContributorData.complianceScore.gte(
          MIN_COMPLIANCE_SCORE_FOR_REWARD
        );
      assert(
        currentContributorData.isEligibleForRewards === isEligibleForRewards,
        `Eligibility for rewards for contributor with address ${contributor.publicKey.toBase58()} should be correctly determined`
      );
    }
  });
});

describe("Reward Distribution for Eligible Contributor", () => {
  it("should distribute rewards correctly from the reward pool", async () => {
    // Choose an eligible contributor
    const eligibleContributor = contributors[0]; // Assuming the first contributor is eligible

    // Find the PDA for the RewardPoolAccount
    const [rewardPoolAccountPDA] = await web3.PublicKey.findProgramAddress(
      [Buffer.from("reward_pool")],
      program.programId
    );

    const [contributorDataAccountPDA] = await web3.PublicKey.findProgramAddress(
      [Buffer.from("contributor_data")],
      program.programId
    );

    // Get initial balances
    const initialRewardPoolBalance = await provider.connection.getBalance(
      rewardPoolAccountPDA
    );
    console.log(`Initial reward pool balance: ${initialRewardPoolBalance}`);

    const initialContributorBalance = await provider.connection.getBalance(
      eligibleContributor.publicKey
    );
    console.log(`Initial contributor balance: ${initialContributorBalance}`);

    const initialOracleContractStateBalance =
      await provider.connection.getBalance(oracleContractState.publicKey);
    console.log(
      `Initial oracle contract state balance: ${initialOracleContractStateBalance}`
    );

    try {
      // Request reward for the eligible contributor
      const requestRewardTxSignature = await program.methods
        .requestReward(eligibleContributor.publicKey)
        .accountsPartial({
          rewardPoolAccount: rewardPoolAccountPDA,
          oracleContractState: oracleContractState.publicKey,
          contributorDataAccount: contributorDataAccountPDA,
          contributor: eligibleContributor.publicKey,
          systemProgram: web3.SystemProgram.programId,
        })
        .rpc();
      await measureComputeUnitsAndStorage(requestRewardTxSignature);

      console.log("Reward request successful");
    } catch (error) {
      console.error("Error requesting reward:", error);
      throw error;
    }

    // Wait for the transaction to be confirmed
    await new Promise((resolve) => setTimeout(resolve, 250));

    // Get updated balances
    const updatedRewardPoolBalance = await provider.connection.getBalance(
      rewardPoolAccountPDA
    );
    console.log(`Updated reward pool balance: ${updatedRewardPoolBalance}`);

    const updatedContributorBalance = await provider.connection.getBalance(
      eligibleContributor.publicKey
    );
    console.log(`Updated contributor balance: ${updatedContributorBalance}`);

    const updatedOracleContractStateBalance =
      await provider.connection.getBalance(oracleContractState.publicKey);
    console.log(
      `Updated oracle contract state balance: ${updatedOracleContractStateBalance}`
    );

    // Check if the reward pool balance decreased by the correct amount
    const rewardPoolDifference =
      initialRewardPoolBalance - updatedRewardPoolBalance;
    expect(rewardPoolDifference).to.equal(
      BASE_REWARD_AMOUNT_IN_LAMPORTS,
      "Reward pool should decrease by the reward amount"
    );

    // Check if the contributor balance increased by the correct amount
    const contributorDifference =
      updatedContributorBalance - initialContributorBalance;
    expect(contributorDifference).to.equal(
      BASE_REWARD_AMOUNT_IN_LAMPORTS,
      "Contributor balance should increase by the reward amount"
    );

    // The oracle contract state balance should not change
    expect(updatedOracleContractStateBalance).to.equal(
      initialOracleContractStateBalance,
      "Oracle contract state balance should not change"
    );
  });
});

describe("Request Reward for Ineligible Contributor", () => {
  it("should not allow reward requests from ineligible contributors", async () => {
    // Choose an ineligible contributor
    const ineligibleContributor = contributors[contributors.length - 1]; // Assuming the last contributor is ineligible

    // Find the PDA for the RewardPoolAccount
    const [rewardPoolAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("reward_pool")],
      program.programId
    );

    const [contributorDataAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("contributor_data")],
      program.programId
    );

    try {
      // Attempt to request reward
      const requestRewardTxSignature = await program.methods
        .requestReward(ineligibleContributor.publicKey)
        .accountsPartial({
          rewardPoolAccount: rewardPoolAccountPDA,
          oracleContractState: oracleContractState.publicKey,
          contributorDataAccount: contributorDataAccountPDA,
        })
        .rpc();
      await measureComputeUnitsAndStorage(requestRewardTxSignature);

      throw new Error(
        "Reward request should have failed for ineligible contributor"
      );
    } catch (error) {
      const anchorError = anchor.AnchorError.parse(error.logs);
      if (anchorError) {
        assert.equal(
          anchorError.error.errorCode.code,
          "NotEligibleForReward",
          "Should throw NotEligibleForReward error"
        );
      } else {
        console.error(`Error parsing error code: ${error.toString()}`);
        // Decide on handling unparsed errors
      }
    }
  });
});

describe("Reallocation of Oracle State", () => {
  it("Simulates account reallocation when capacity is reached", async () => {
    // Find PDAs
    const [contributorDataAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("contributor_data")],
      program.programId
    );

    const [feeReceivingContractAccountPDA] =
      web3.PublicKey.findProgramAddressSync(
        [Buffer.from("fee_receiving_contract")],
        program.programId
      );

    const [rewardPoolAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("reward_pool")],
      program.programId
    );

    // Get initial sizes of accounts
    const initialContributorDataSize = (
      await provider.connection.getAccountInfo(contributorDataAccountPDA)
    ).data.length;
    console.log("Initial ContributorData size:", initialContributorDataSize);

    // We'll add contributors until we need reallocation
    const contributorsToAdd = 60;
    const newContributors = [];

    // Pre-generate all contributor keypairs
    for (let i = 0; i < contributorsToAdd; i++) {
      newContributors.push(web3.Keypair.generate());
    }

    // Track when reallocation happens
    let reallocated = false;

    for (let i = 0; i < contributorsToAdd; i++) {
      const newContributor = newContributors[i];

      // Fund the fee receiving account for registration
      const fundTx = new web3.Transaction().add(
        web3.SystemProgram.transfer({
          fromPubkey: admin.publicKey,
          toPubkey: feeReceivingContractAccountPDA,
          lamports: REGISTRATION_ENTRANCE_FEE_SOL * web3.LAMPORTS_PER_SOL,
        })
      );

      await provider.sendAndConfirm(fundTx);

      try {
        // Try to register new contributor
        const tx = await program.methods
          .registerNewDataContributor()
          .accountsStrict({
            contributorDataAccount: contributorDataAccountPDA,
            contributorAccount: newContributor.publicKey,
            rewardPoolAccount: rewardPoolAccountPDA,
            feeReceivingContractAccount: feeReceivingContractAccountPDA,
            systemProgram: SystemProgram.programId,
          })
          .signers([newContributor])
          .rpc();

        console.log(`Successfully registered contributor ${i + 1}`);
      } catch (error) {
        // If we get a specific error about account size, trigger reallocation
        console.log("Registration error:", error.toString());

        if (
          error.toString().includes("AccountDidNotFit") ||
          error.toString().includes("0x1") ||
          error.toString().includes("insufficient account size")
        ) {
          console.log("Account capacity reached, performing reallocation...");

          // Find all required PDAs for reallocation
          const [tempReportAccountPDA] = web3.PublicKey.findProgramAddressSync(
            [Buffer.from("temp_tx_status_report")],
            program.programId
          );

          const [txidSubmissionCountsAccountPDA] =
            web3.PublicKey.findProgramAddressSync(
              [Buffer.from("txid_submission_counts")],
              program.programId
            );

          const [aggregatedConsensusDataAccountPDA] =
            web3.PublicKey.findProgramAddressSync(
              [Buffer.from("aggregated_consensus_data")],
              program.programId
            );

          // Trigger reallocation with all required accounts
          const reallocateTx = await program.methods
            .reallocateOracleState()
            .accountsStrict({
              oracleContractState: oracleContractState.publicKey,
              adminPubkey: admin.publicKey,
              tempReportAccount: tempReportAccountPDA,
              contributorDataAccount: contributorDataAccountPDA,
              txidSubmissionCountsAccount: txidSubmissionCountsAccountPDA,
              aggregatedConsensusDataAccount: aggregatedConsensusDataAccountPDA,
              systemProgram: web3.SystemProgram.programId,
            })
            .rpc();

          console.log("Reallocation transaction completed");
          reallocated = true;

          // Wait a bit for the reallocation to take effect
          await new Promise((resolve) => setTimeout(resolve, 1000));

          // Retry registration after reallocation
          await program.methods
            .registerNewDataContributor()
            .accountsStrict({
              contributorDataAccount: contributorDataAccountPDA,
              contributorAccount: newContributor.publicKey,
              rewardPoolAccount: rewardPoolAccountPDA,
              feeReceivingContractAccount: feeReceivingContractAccountPDA,
              systemProgram: SystemProgram.programId,
            })
            .signers([newContributor])
            .rpc();

          console.log("Successfully registered after reallocation");
        } else {
          throw error;
        }
      }

      // Add delay between registrations to avoid rate limiting
      await new Promise((resolve) => setTimeout(resolve, 200));
    }

    // Verify final account size and successful registrations
    const finalContributorDataSize = (
      await provider.connection.getAccountInfo(contributorDataAccountPDA)
    ).data.length;
    console.log("Final ContributorData size:", finalContributorDataSize);

    // Verify the data
    const contributorData = await program.account.contributorDataAccount.fetch(
      contributorDataAccountPDA
    );

    // Only assert size increase if reallocation happened
    if (reallocated) {
      assert(
        finalContributorDataSize > initialContributorDataSize,
        "Account size should have increased after reallocation"
      );
    }

    assert(
      contributorData.contributors.length >= contributorsToAdd,
      "All contributors should be registered after reallocation"
    );
  });
});

describe("Withdrawal of Funds", () => {
  it("Allows admin to withdraw funds from reward pool and fee-receiving contract", async () => {
    // Find PDAs for accounts
    const [rewardPoolAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("reward_pool")],
      program.programId
    );

    const [feeReceivingContractAccountPDA] =
      web3.PublicKey.findProgramAddressSync(
        [Buffer.from("fee_receiving_contract")],
        program.programId
      );

    // Fund the rewardPoolAccount PDA with 1 SOL for testing
    const fundingAmount = web3.LAMPORTS_PER_SOL; // 1 SOL
    const fundRewardPoolTx = new web3.Transaction().add(
      web3.SystemProgram.transfer({
        fromPubkey: admin.publicKey,
        toPubkey: rewardPoolAccountPDA,
        lamports: fundingAmount,
      })
    );

    const fundRewardPoolTxSignature = await provider.sendAndConfirm(
      fundRewardPoolTx
    );
    await measureComputeUnitsAndStorage(fundRewardPoolTxSignature);
    console.log(
      `Funded reward_pool_account PDA with ${fundingAmount} lamports`
    );

    // Fund the fee receiving contract for testing
    const fundFeeReceivingTx = new web3.Transaction().add(
      web3.SystemProgram.transfer({
        fromPubkey: admin.publicKey,
        toPubkey: feeReceivingContractAccountPDA,
        lamports: fundingAmount,
      })
    );

    const fundFeeReceivingTxSignature = await provider.sendAndConfirm(
      fundFeeReceivingTx
    );
    await measureComputeUnitsAndStorage(fundFeeReceivingTxSignature);
    console.log(
      `Funded fee_receiving_contract PDA with ${fundingAmount} lamports`
    );

    // Wait for funding transactions to be confirmed
    await new Promise((resolve) => setTimeout(resolve, 500));

    // Get initial balances
    const initialRewardPoolBalance = await provider.connection.getBalance(
      rewardPoolAccountPDA
    );
    const initialFeeReceivingBalance = await provider.connection.getBalance(
      feeReceivingContractAccountPDA
    );
    const initialAdminBalance = await provider.connection.getBalance(
      admin.publicKey
    );

    console.log(`Initial reward pool balance: ${initialRewardPoolBalance}`);
    console.log(`Initial fee receiving balance: ${initialFeeReceivingBalance}`);
    console.log(`Initial admin balance: ${initialAdminBalance}`);

    // Define withdrawal amounts as half of initial balances
    const rewardPoolWithdrawalAmount = Math.floor(initialRewardPoolBalance / 2);
    const feeReceivingWithdrawalAmount = Math.floor(
      initialFeeReceivingBalance / 2
    );

    try {
      // Withdraw funds
      const withdrawFundsTxSignature = await program.methods
        .withdrawFunds(
          new BN(rewardPoolWithdrawalAmount),
          new BN(feeReceivingWithdrawalAmount)
        )
        .accountsStrict({
          oracleContractState: oracleContractState.publicKey,
          adminAccount: admin.publicKey,
          rewardPoolAccount: rewardPoolAccountPDA,
          feeReceivingContractAccount: feeReceivingContractAccountPDA,
          systemProgram: SystemProgram.programId,
        })
        .rpc();
      await measureComputeUnitsAndStorage(withdrawFundsTxSignature);

      console.log("Withdrawal transaction completed successfully");
    } catch (error) {
      console.error("Error during withdrawal:", error);
      throw error;
    }

    // Wait for the withdrawal transaction to be confirmed
    await new Promise((resolve) => setTimeout(resolve, 500));

    // Get updated balances
    const updatedRewardPoolBalance = await provider.connection.getBalance(
      rewardPoolAccountPDA
    );
    const updatedFeeReceivingBalance = await provider.connection.getBalance(
      feeReceivingContractAccountPDA
    );
    const updatedAdminBalance = await provider.connection.getBalance(
      admin.publicKey
    );

    console.log(`Updated reward pool balance: ${updatedRewardPoolBalance}`);
    console.log(`Updated fee receiving balance: ${updatedFeeReceivingBalance}`);
    console.log(`Updated admin balance: ${updatedAdminBalance}`);

    // Calculate the expected total withdrawal amount
    const expectedTotalWithdrawal =
      rewardPoolWithdrawalAmount + feeReceivingWithdrawalAmount;

    // Calculate the actual withdrawal amount, accounting for transaction fees
    const actualWithdrawal = updatedAdminBalance - initialAdminBalance;

    // Allow for a small margin due to transaction fees
    const margin = 10_000;

    // Assertions with detailed error messages
    assert(
      Math.abs(actualWithdrawal - expectedTotalWithdrawal) <= margin,
      `Admin balance should increase by approximately ${expectedTotalWithdrawal} lamports (allowing for transaction fees), but increased by ${actualWithdrawal} lamports`
    );

    assert.equal(
      updatedRewardPoolBalance,
      initialRewardPoolBalance - rewardPoolWithdrawalAmount,
      `Reward pool balance should decrease by exactly ${rewardPoolWithdrawalAmount} lamports`
    );

    assert.equal(
      updatedFeeReceivingBalance,
      initialFeeReceivingBalance - feeReceivingWithdrawalAmount,
      `Fee receiving contract balance should decrease by exactly ${feeReceivingWithdrawalAmount} lamports`
    );

    console.log(`
      Withdrawal test summary:
      - Reward pool withdrawal: ${rewardPoolWithdrawalAmount} lamports
      - Fee receiving withdrawal: ${feeReceivingWithdrawalAmount} lamports
      - Total withdrawal: ${expectedTotalWithdrawal} lamports
      - Actual admin balance increase: ${actualWithdrawal} lamports
      Test completed successfully!
    `);
  });

  it("Should not allow non-admin to withdraw funds", async () => {
    // Create a non-admin keypair
    const nonAdmin = web3.Keypair.generate();

    // Fund the non-admin account with some SOL for transaction fees
    const fundTx = new web3.Transaction().add(
      web3.SystemProgram.transfer({
        fromPubkey: admin.publicKey,
        toPubkey: nonAdmin.publicKey,
        lamports: web3.LAMPORTS_PER_SOL * 0.1, // 0.1 SOL
      })
    );

    const fundTxSignature = await provider.sendAndConfirm(fundTx);
    await measureComputeUnitsAndStorage(fundTxSignature);

    // Find PDAs
    const [rewardPoolAccountPDA] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("reward_pool")],
      program.programId
    );

    const [feeReceivingContractAccountPDA] =
      web3.PublicKey.findProgramAddressSync(
        [Buffer.from("fee_receiving_contract")],
        program.programId
      );

    try {
      // Attempt to withdraw funds as non-admin
      await program.methods
        .withdrawFunds(new BN(1000000), new BN(1000000))
        .accountsStrict({
          oracleContractState: oracleContractState.publicKey,
          adminAccount: nonAdmin.publicKey,
          rewardPoolAccount: rewardPoolAccountPDA,
          feeReceivingContractAccount: feeReceivingContractAccountPDA,
          systemProgram: SystemProgram.programId,
        })
        .signers([nonAdmin])
        .rpc();

      assert.fail("Transaction should have failed for non-admin");
    } catch (error) {
      const anchorError = anchor.AnchorError.parse(error.logs);
      if (anchorError) {
        assert.equal(
          anchorError.error.errorCode.code,
          "UnauthorizedWithdrawalAccount",
          "Should throw UnauthorizedWithdrawalAccount error"
        );
        console.log("Non-admin withdrawal correctly rejected");
      } else {
        throw error;
      }
    }
  });
});

// After all tests
after(async function () {
  console.log(`Total compute units used: ${totalComputeUnitsUsed}`);
  console.log(`Max account storage used: ${maxAccountStorageUsed} bytes`);
});
