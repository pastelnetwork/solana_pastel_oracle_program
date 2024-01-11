import * as anchor from '@coral-xyz/anchor';
import { Program, web3, AnchorProvider} from  '@coral-xyz/anchor';
import { SolanaPastelOracleProgram, IDL } from '../target/types/solana_pastel_oracle_program';
import { assert } from 'chai';

process.env.ANCHOR_PROVIDER_URL = "http://127.0.0.1:8899";
const provider = AnchorProvider.env();
anchor.setProvider(provider);
const programID = new anchor.web3.PublicKey("AfP1c4sFcY1FeiGjQEtyxCim8BRnw22okNbKAsH2sBsB");
const program = new Program<SolanaPastelOracleProgram>(IDL, programID, provider);
const admin = provider.wallet; // Use the provider's wallet
const oracleContractState = web3.Keypair.generate();

console.log("Program ID:", programID.toString()); // Log the public key of the program
console.log("Admin ID:", admin.publicKey.toString()); // Log the public key of the admin wallet


describe('Initialization', () => {
  it('Initializes the oracle contract state', async () => {
    // Find the PDAs for the RewardPoolAccount and FeeReceivingContractAccount
    const [rewardPoolAccountPDA, rewardPoolAccountBump] = await web3.PublicKey.findProgramAddress(
      [Buffer.from("reward_pool")],
      program.programId
    );
    const [feeReceivingContractAccountPDA, feeReceivingContractAccountBump] = await web3.PublicKey.findProgramAddress(
      [Buffer.from("fee_receiving_contract")],
      program.programId
    );

    // Calculate the rent-exempt minimum balance
    const minBalanceForRentExemption = await provider.connection.getMinimumBalanceForRentExemption(8 + 5000000);

    // Initialize the OracleContractState
    console.log("Initializing Oracle Contract State");
    await program.methods.initialize(admin.publicKey)
    .accounts({
      oracleContractState: oracleContractState.publicKey,
      user: admin.publicKey,
      rewardPoolAccount: rewardPoolAccountPDA,
      feeReceivingContractAccount: feeReceivingContractAccountPDA,
      systemProgram: web3.SystemProgram.programId,
    })
    .signers([oracleContractState]) // Include the state account as a signer
    .rpc();

    // Fetch the state of the OracleContractState account
    const state = await program.account.oracleContractState.fetch(oracleContractState.publicKey);
    assert.ok(state.isInitialized, "Oracle Contract State should be initialized");
    assert.equal(state.adminPubkey.toString(), admin.publicKey.toString(), "Admin public key should match");
  });
});


// const REGISTRATION_ENTRANCE_FEE_SOL = 0.1;
// const testContributor = web3.Keypair.generate() // Contributor Keypair used across tests

// describe('Contributor Registration', () => {
//   it('Registers a new data contributor', async () => {

//     // Transfer the registration fee to feeReceivingContractAccount
//     const transaction = new web3.Transaction().add(
//       web3.SystemProgram.transfer({
//         fromPubkey: admin.publicKey,
//         toPubkey: feeReceivingContractAccount.publicKey,
//         lamports: REGISTRATION_ENTRANCE_FEE_SOL * web3.LAMPORTS_PER_SOL,
//       })
//     );

//     // Sign and send the transaction
//     await provider.sendAndConfirm(transaction);

//     // Call the RPC method with the new syntax
//     await program.methods.registerNewDataContributor()
//       .accounts({
//         oracleContractState: oracleContractState.publicKey,
//         contributorAccount: testContributor.publicKey,
//         rewardPoolAccount: rewardPoolAccount.publicKey,
//         feeReceivingContractAccount: feeReceivingContractAccount.publicKey,
//       })
//       .signers([testContributor])
//       .rpc();

//     const state = await program.account.oracleContractState.fetch(oracleContractState.publicKey);

//     const contributors: { rewardAddress: web3.PublicKey }[] = state.contributors as any;
//     const registeredContributor = contributors.find(c => c.rewardAddress.equals(testContributor.publicKey));
//     assert.exists(registeredContributor, 'Contributor should be registered');
//   });
// });


// describe('Data Report Submission', () => {
//   it('Submits a data report', async () => {
//     const report = {
//       txid: "some_txid",
//       txidStatus: { minedActivated: {} }, // Assuming 'MinedActivated' is an enum variant
//       pastelTicketType: { nft: {} }, // Assuming 'Nft' is an enum variant
//       first6CharactersOfSha3256HashOfCorrespondingFile: "abc123",
//       timestamp: new anchor.BN(Date.now() / 1000),
//       contributorRewardAddress: testContributor.publicKey
//     };
    
//     await program.methods.submitDataReport(report).accounts({
//       oracleContractState: oracleContractState.publicKey,
//       contributor: testContributor.publicKey,
//     }).signers([testContributor]).rpc();

//     const state = await program.account.oracleContractState.fetch(oracleContractState.publicKey);

//     // Accessing the reports Map
//     const reportsMap = new Map(Object.entries(state.reports));
//     const submittedReport = reportsMap.get(report.txid);
//     assert.exists(submittedReport, 'Report should be submitted');
//   });
// });