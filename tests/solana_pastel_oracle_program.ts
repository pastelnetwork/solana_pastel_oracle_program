import * as anchor from '@project-serum/anchor';
import { Program, web3, BN } from '@project-serum/anchor';
import { AnchorProvider } from '@project-serum/anchor';
import { SolanaPastelOracleProgram, IDL } from '../target/types/solana_pastel_oracle_program';
import { assert } from 'chai';

const provider = AnchorProvider.env();
anchor.setProvider(provider);
const programID = new web3.PublicKey('Your Program PublicKey Here');
const program = new Program<SolanaPastelOracleProgram>(IDL, programID, provider);

const admin = web3.Keypair.generate();
const oracleContractState = web3.Keypair.generate();
const rewardPoolAccount = web3.Keypair.generate();
const feeReceivingContractAccount = web3.Keypair.generate();
const REGISTRATION_ENTRANCE_FEE_SOL = 0.1;
let testContributor: web3.Keypair; // Contributor Keypair used across tests

async function airdropSOL(account: web3.PublicKey, amount: number) {
  const airdropSignature = await provider.connection.requestAirdrop(
    account,
    amount * web3.LAMPORTS_PER_SOL // Convert SOL to lamports
  );

  // Fetch latest blockhash and block height
  const latestBlockhash = await provider.connection.getLatestBlockhash();

  // Construct the TransactionConfirmationStrategy
  const confirmationStrategy: anchor.web3.BlockheightBasedTransactionConfirmationStrategy = {
    signature: airdropSignature,
    blockhash: latestBlockhash.blockhash,
    lastValidBlockHeight: latestBlockhash.lastValidBlockHeight,
  };

  // Confirm the transaction
  await provider.connection.confirmTransaction(confirmationStrategy, 'finalized');
}

// Airdrop SOL to your accounts before running tests
before(async () => {
  await airdropSOL(admin.publicKey, 10); // Airdrop 10 SOL to the admin account
});


describe('Initialization', () => {
  it('Initializes the oracle contract state', async () => {
    // New method call syntax
    await program.methods.initialize(admin.publicKey)
      .accounts({
        oracleContractState: oracleContractState.publicKey,
        user: admin.publicKey,
        rewardPoolAccount: rewardPoolAccount.publicKey,
        feeReceivingContractAccount: feeReceivingContractAccount.publicKey,
        systemProgram: web3.SystemProgram.programId,
      })
      .signers([admin, oracleContractState])
      .rpc();

    const state = await program.account.oracleContractState.fetch(oracleContractState.publicKey);
    assert.ok(state.isInitialized);
    assert.equal(state.adminPubkey.toString(), admin.publicKey.toString());
  });
});

describe('Contributor Registration', () => {
  it('Registers a new data contributor', async () => {
    testContributor = web3.Keypair.generate();

    // Transfer the registration fee to feeReceivingContractAccount
    const transaction = new web3.Transaction().add(
      web3.SystemProgram.transfer({
        fromPubkey: admin.publicKey,
        toPubkey: feeReceivingContractAccount.publicKey,
        lamports: REGISTRATION_ENTRANCE_FEE_SOL * web3.LAMPORTS_PER_SOL,
      })
    );

    // Create a VersionedTransaction
    const versionedTransaction = new web3.VersionedTransaction(transaction.compileMessage());
    versionedTransaction.sign([admin]);

    // Send the transaction
    const signature = await provider.connection.sendTransaction(versionedTransaction);

    // Confirm the transaction
    const latestBlockhash = await provider.connection.getLatestBlockhash();
    await provider.connection.confirmTransaction({
      signature: signature,
      blockhash: latestBlockhash.blockhash,
      lastValidBlockHeight: latestBlockhash.lastValidBlockHeight,
    });

    // Call the RPC method with the new syntax
    await program.methods.registerNewDataContributor()
      .accounts({
        oracleContractState: oracleContractState.publicKey,
        contributorAccount: testContributor.publicKey,
        rewardPoolAccount: rewardPoolAccount.publicKey,
        feeReceivingContractAccount: feeReceivingContractAccount.publicKey,
      })
      .signers([testContributor])
      .rpc();

    const state = await program.account.oracleContractState.fetch(oracleContractState.publicKey);

    const contributors: { rewardAddress: web3.PublicKey }[] = state.contributors as any;
    const registeredContributor = contributors.find(c => c.rewardAddress.equals(testContributor.publicKey));
    assert.exists(registeredContributor, 'Contributor should be registered');
  });
});

describe('Data Report Submission', () => {
  it('Submits a data report', async () => {
    const report = {
      txid: "some_txid",
      txidStatus: { minedActivated: {} }, // Assuming 'MinedActivated' is an enum variant
      pastelTicketType: { nft: {} }, // Assuming 'Nft' is an enum variant
      first6CharactersOfSha3256HashOfCorrespondingFile: "abc123",
      timestamp: new anchor.BN(Date.now() / 1000),
      contributorRewardAddress: testContributor.publicKey
    };
    
    await program.methods.submitDataReport(report).accounts({
      oracleContractState: oracleContractState.publicKey,
      contributor: testContributor.publicKey,
    }).signers([testContributor]).rpc();

    const state = await program.account.oracleContractState.fetch(oracleContractState.publicKey);

    // Accessing the reports Map
    const reportsMap = new Map(Object.entries(state.reports));
    const submittedReport = reportsMap.get(report.txid);
    assert.exists(submittedReport, 'Report should be submitted');
  });
});