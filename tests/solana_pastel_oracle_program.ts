import * as anchor from '@coral-xyz/anchor';
import { Program, web3, AnchorProvider, BN} from '@coral-xyz/anchor';
import { SolanaPastelOracleProgram, IDL } from '../target/types/solana_pastel_oracle_program';
import { assert } from 'chai';
import * as crypto from 'crypto';

process.env.ANCHOR_PROVIDER_URL = "http://127.0.0.1:8899";
process.env.RUST_LOG = "solana_runtime::system_instruction_processor=trace,solana_runtime::message_processor=trace,solana_bpf_loader=debug,solana_rbpf=debug";
const provider = AnchorProvider.env();
anchor.setProvider(provider);
const programID = new anchor.web3.PublicKey("AfP1c4sFcY1FeiGjQEtyxCim8BRnw22okNbKAsH2sBsB");
const program = new Program<SolanaPastelOracleProgram>(IDL, programID, provider);
const admin = provider.wallet; // Use the provider's wallet
const oracleContractState = web3.Keypair.generate();

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


const REGISTRATION_ENTRANCE_FEE_SOL = 0.001;
const testContributor = web3.Keypair.generate(); // Contributor Keypair used across tests

describe('Contributor Registration', () => {
  it('Registers a new data contributor', async () => {
    // Find the PDAs for the RewardPoolAccount and FeeReceivingContractAccount
    const [rewardPoolAccountPDA] = await web3.PublicKey.findProgramAddressSync(
      [Buffer.from("reward_pool")],
      program.programId
    );
    const [feeReceivingContractAccountPDA] = await web3.PublicKey.findProgramAddressSync(
      [Buffer.from("fee_receiving_contract")],
      program.programId
    );

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
        contributorAccount: testContributor.publicKey,
        rewardPoolAccount: rewardPoolAccountPDA,
        feeReceivingContractAccount: feeReceivingContractAccountPDA,
      })
      .signers([testContributor])
      .rpc();

    const state = await program.account.oracleContractState.fetch(oracleContractState.publicKey);

    const contributors = state.contributors as { rewardAddress: web3.PublicKey }[];
    const registeredContributor = contributors.find(c => c.rewardAddress.equals(testContributor.publicKey));
    assert.exists(registeredContributor, 'Contributor should be registered');
  });
});

const COST_IN_SOL_OF_ADDING_PASTEL_TXID_FOR_MONITORING = 0.01;

describe('TXID Monitoring', () => {
  it('Adds a new TXID for monitoring', async () => {
    // Setup
    const txidToAdd = '9930511c526808e6849a25cb0eb6513f729c2a71ec51fbca084d7c7e4a8dea2f';

    const expectedAmountLamports = COST_IN_SOL_OF_ADDING_PASTEL_TXID_FOR_MONITORING * web3.LAMPORTS_PER_SOL;
    const expectedAmountStr = expectedAmountLamports.toString();

    // Concatenate "pending_payment", txidToAdd, and the byte array of admin's public key
    const preimage = Buffer.concat([
      Buffer.from("pending_payment"),
      Buffer.from(txidToAdd),
      admin.publicKey.toBuffer()  // This gets the raw byte array of the public key
    ]);

    const seedHash = crypto.createHash('sha256').update(preimage).digest();

    // Find the PDA for pendingPaymentAccount using the hashed seed (first 32 bytes)
    const [pendingPaymentAccountPDA, pendingPaymentAccountBump] = await web3.PublicKey.findProgramAddressSync(
      [seedHash.slice(0, 32)],
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