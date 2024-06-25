# Solana-Pastel Oracle Program

## Overview
This program serves as an oracle service, creating a bridge between the Solana blockchain and the Pastel Network. It focuses on verifying and monitoring Pastel transaction IDs (TXIDs) and related blockchain tickets, as well as the files associated with these tickets.

## Functionality
- **Transaction Monitoring**: Tracks Pastel TXIDs, updating their status (e.g., pending, mined) within the Solana blockchain.
- **Data Compliance and Reward System**: Implements a system for contributors to submit reports on Pastel TXID statuses, with rewards based on report accuracy and reliability.
- **Consensus Algorithm**: Uses a consensus mechanism to ascertain the most accurate status of Pastel TXIDs, based on multiple oracle reports.
- **Ban Mechanism**: Features temporary and permanent bans for contributors based on non-consensus report submissions.

## Core Components
- **OracleContractState**: Maintains the central state of the oracle, including contributor data, consensus data, and TXID submission counts.
- **Contributor Management**: Registers new data contributors and manages their compliance and reliability scores. Contributors must meet certain criteria to be eligible for rewards.
- **Report Submission and Processing**: Manages the submission of Pastel TXID status reports by contributors and processes these reports to update consensus data and contributor scores.
- **Consensus Calculation**: Determines the consensus on TXID status based on aggregated data from multiple reports.
- **Reward and Penalty Mechanisms**: Manages rewards distribution to contributors and applies ban penalties in case of report inconsistencies.
- **Fee and Payment Management**: Handles the addition of new Pastel TXIDs for monitoring, including payment processing for such additions.

## Key Constants and Variables
- **Registration and Reward Fees**: Specifies the fees for contributor registration and the base reward amount for report submissions.
- **Ban Thresholds**: Defines the thresholds for temporary and permanent bans based on report submission accuracy.
- **Consensus Parameters**: Establishes the minimum requirements for consensus calculation among oracle reports.

## Security and Validation
- Ensures data integrity through rigorous checks at every step, from contributor registration to report submission and consensus calculation.
- Implements a robust validation mechanism to prevent incorrect or fraudulent data submissions.

## Setup Instructions

### Install Solana Testnet on Ubuntu

#### Install Rustup
```bash
curl https://sh.rustup.rs -sSf | sh
rustup default nightly  
rustup update nightly   
rustc --version 
```

#### Install Solana
```bash
sudo apt update && sudo apt upgrade -y && sudo apt autoremove -y  
sudo apt install libssl-dev libudev-dev pkg-config zlib1g-dev llvm clang make -y         
sh -c "$(curl -sSfL https://release.solana.com/v1.17.13/install)"      
export PATH="/home/ubuntu/.local/share/solana/install/active_release/bin:$PATH" 
source ~/.zshrc   # If you use Zsh
solana --version  
```

### Setup Anchor

```bash
sudo apt-get update && sudo apt-get upgrade && sudo apt-get install -y pkg-config build-essential libudev-dev
cargo install --git https://github.com/coral-xyz/anchor avm --locked --force
avm install latest
avm use latest
anchor --version
```

### Get Code and Test

```bash
git clone https://github.com/pastelnetwork/solana_pastel_oracle_program.git
cd solana_pastel_oracle_program
anchor test
```

These steps will set up the necessary environment for running the Solana-Pastel Oracle Program, including the installation of Rust, Solana, and Anchor, as well as cloning the repository and running tests to ensure everything is correctly set up.


## Testing Code

The testing code for the Solana-Pastel Oracle Program, written in TypeScript using the Anchor framework, performs comprehensive tests to validate the functionality of the program. It uses Mocha for structuring tests and Chai for assertions. Here's an overview of its components and functionalities:

1. **Setup and Configuration**: 
    - Initializes Anchor with the Solana testnet and sets various configurations.
    - Defines constants and variables such as program ID, contributor details, TXID counts, and error codes.

2. **Initialization Tests**: 
    - Tests the initialization and expansion of the oracle contract state.
    - Uses PDAs (Program-Derived Addresses) for various accounts like reward pool, fee-receiving contract, and contributor data.
    - Confirms the contract state's initialization and checks the reallocation of the oracle state.

3. **Set Bridge Contract Tests**: 
    - Ensures the bridge contract address is correctly set to the admin address.

4. **Contributor Registration Tests**: 
    - Registers new data contributors and validates their registration.
    - Transfers the registration fee to the required accounts and confirms the contributor's registration in the system.

5. **TXID Monitoring Tests**: 
    - Adds multiple TXIDs for monitoring and checks if they are correctly monitored.
    - Verifies the creation of corresponding `PendingPayment` structs for each TXID.

6. **Data Report Submission Tests**: 
    - Submits data reports for different TXIDs to simulate both consensus and dissent among contributors.
    - Determines the status of TXIDs (e.g., MinedActivated, Invalid) and submits reports accordingly.
    - Validates the consensus on TXID status after the reports are submitted.

7. **Data Cleanup Verification Tests**: 
    - Ensures that data related to TXIDs is properly cleaned up from the `TempTxStatusReportAccount` after consensus is reached.

8. **Payment Processing Tests**: 
    - Simulates the processing of payments for monitored TXIDs by the bridge contract.
    - Transfers payments and confirms that the payments are correctly recorded in the system.

9. **Eligibility for Rewards Tests**: 
    - Checks if contributors meet the eligibility criteria for rewards based on compliance, reliability scores, and report submissions.

10. **Reward Distribution Tests**: 
    - Tests the distribution of rewards from the reward pool to eligible contributors.
    - Verifies that ineligible contributors cannot request rewards.

Throughout these tests, the code interacts with various accounts and PDAs, simulating real-world scenarios of contributor activities and interactions with the oracle contract. The tests cover various aspects of the program, including state initialization, contributor management, transaction monitoring, consensus mechanism, and reward distribution, ensuring the program functions as intended.

## Consensus Process and Contributor Scoring System

### Consensus Process

The consensus process in the Solana-Pastel Oracle Program is designed to determine the most reliable status of a Pastel transaction ID (TXID) based on reports from multiple contributors. Here's how it works:

1. **Aggregating Data**: The `get_aggregated_data` function retrieves the aggregated consensus data for a given TXID, which includes the weighted status and hash values reported by contributors.

2. **Computing Consensus**: The `compute_consensus` function calculates the consensus status and hash for the TXID. It identifies the status and hash with the highest accumulated weight as the consensus outcome.

3. **Consensus Decision**: The consensus status represents the most agreed-upon state of the TXID, and the consensus hash represents the most agreed-upon hash of the corresponding file.

### Rationale Behind the Consensus Design

- **Robustness**: Using a weighted approach where each contributor's report influences the consensus based on their reliability and compliance score ensures robustness against inaccurate or malicious reports.
- **Flexibility**: The mechanism can adapt to different scenarios, like new information or changing network conditions, by recalculating the consensus with each new report.

### Contributor Scoring System

The scoring system for contributors is a crucial aspect of the program, affecting their influence in the consensus process and their eligibility for rewards.

#### Score Adjustment Mechanisms

1. **Updating Scores (`update_scores`)**: Contributors’ scores are dynamically adjusted based on their report accuracy. Accurate reports increase their compliance and reliability scores, while inaccurate reports lead to a decrease.

2. **Applying Bans (`apply_bans`)**: Contributors with a pattern of inaccurate reporting are subject to temporary or permanent bans, reducing the risk of bad actors influencing the consensus process.

3. **Time-Based Decay**: The system implements a decay factor on scores over time, encouraging continuous and consistent participation.

#### Rationale Behind Scoring Components

- **Accuracy and Streaks**: Rewarding accuracy and consistency (streaks) incentivizes contributors to provide reliable data. The scaling bonus for consecutive accurate reports encourages sustained high-quality participation.
  
- **Time Weight**: The time since the last active contribution is factored into score adjustments. This ensures that active contributors have a more significant impact on the consensus and rewards.

- **Decay Mechanism**: To maintain a dynamic and responsive system, scores decay over time. This prevents long-inactive contributors from retaining high influence and encourages regular participation.

- **Reliability Integration**: A contributor’s overall reliability (ratio of accurate reports to total reports) is integrated into their compliance score, ensuring that consistently reliable contributors have more influence.

#### Purpose of the Scoring System

- **Quality Control**: Ensures that the data influencing the consensus process is of high quality and from reliable sources.
- **Incentive Alignment**: Aligns contributors' incentives with the network's goal of accurate and reliable data reporting.
- **Adaptability**: Allows the system to adapt to changing participant behaviors and network conditions by recalibrating contributor influence dynamically.
- **Security**: Protects against manipulation or attacks by devaluing the influence of malicious or consistently inaccurate contributors.


## Step-by-Step Narrative of How the System Works:

### 1. Initialization

**Objective**: Set up the initial state of the oracle contract, including various PDAs and admin public key.

#### Initialize Program:

1. **Function Definition**:
   - The `initialize` function is defined in the `solana_pastel_oracle_program` module.
   - It takes a `Context<Initialize>` and an `admin_pubkey: Pubkey` as parameters.

2. **Oracle Contract State Setup**:
   - Inside the `initialize` function, the `initialize_oracle_state` method is called on the `ctx.accounts`.
   - This method checks if the oracle contract state is already initialized. If it is, an `OracleError::AccountAlreadyInitialized` error is returned.
   - The `is_initialized` flag is set to `true`, and the `admin_pubkey` is assigned to the `admin_pubkey` field of the oracle contract state.
   - The `monitored_txids` vector is initialized as an empty vector.
   - The `bridge_contract_pubkey` is set to the default public key.

3. **PDA Initialization**:
   - Several accounts are initialized using PDAs (Program Derived Accounts):
     - `RewardPool` and `FeeReceivingContract` accounts are initialized with seeds "reward_pool" and "fee_receiving_contract", respectively.
     - `TempTxStatusReportAccount` is initialized with the seed "temp_tx_status_report".
     - `ContributorDataAccount` is initialized with the seed "contributor_data".
     - `TxidSubmissionCountsAccount` is initialized with the seed "txid_submission_counts".
     - `AggregatedConsensusDataAccount` is initialized with the seed "aggregated_consensus_data".
   - These accounts are allocated with a specific amount of space (e.g., 10,240 bytes) to store relevant data.

4. **Logging**:
   - Messages are logged to indicate successful initialization and the public keys of the various accounts.

### 2. Contributor Registration

**Objective**: Allow new contributors to register and start participating in data submission.

#### Register New Contributors:

1. **Function Definition**:
   - The `register_new_data_contributor` function is defined in the `solana_pastel_oracle_program` module.
   - It takes a `Context<RegisterNewDataContributor>` as a parameter.

2. **Check for Existing Registration**:
   - Inside the `register_new_data_contributor_helper` function, the program first checks if the contributor is already registered by iterating through the `contributors` vector in the `ContributorDataAccount`.
   - If the contributor's public key already exists, an `OracleError::ContributorAlreadyRegistered` error is returned.

3. **Verify Registration Fee**:
   - The program verifies that the registration fee has been paid by checking the lamports in the `fee_receiving_contract_account`.
   - If the fee is not present, an `OracleError::RegistrationFeeNotPaid` error is returned.

4. **Transfer Registration Fee**:
   - The registration fee is deducted from the `fee_receiving_contract_account` and added to the `reward_pool_account`.

5. **Initialize Contributor Data**:
   - A new `Contributor` struct is created with initial values:
     - `reward_address`: Contributor's public key.
     - `registration_entrance_fee_transaction_signature`: Initially empty string.
     - `compliance_score`: Set to 1.0.
     - `last_active_timestamp`: Set to the current Unix timestamp.
     - `total_reports_submitted`: Initially 0.
     - `accurate_reports_count`: Initially 0.
     - `current_streak`: Initially 0.
     - `reliability_score`: Set to 1.0.
     - `consensus_failures`: Initially 0.
     - `ban_expiry`: Initially 0.
     - `is_eligible_for_rewards`: Initially false.
     - `is_recently_active`: Initially false.
     - `is_reliable`: Initially false.
   - This new contributor is then appended to the `contributors` vector in the `ContributorDataAccount`.

6. **Logging**:
   - Messages are logged to indicate successful registration and the contributor's public key and registration timestamp.

### 3. TXID Monitoring

**Objective**: Add TXIDs to be monitored by the oracle and track them.

#### Add TXID for Monitoring:

1. **Function Definition**:
   - The `add_txid_for_monitoring` function is defined in the `solana_pastel_oracle_program` module.
   - It takes a `Context<AddTxidForMonitoring>` and an `AddTxidForMonitoringData` as parameters.

2. **Parameter Validation**:
   - Inside the `add_txid_for_monitoring_helper` function, the `data.txid` parameter is validated to ensure it does not exceed the maximum length (`MAX_TXID_LENGTH`). If it does, an `OracleError::InvalidTxid` error is returned.

3. **Verify Caller**:
   - The program checks that the caller's public key matches the `bridge_contract_pubkey` in the Oracle Contract State. If not, an `OracleError::NotBridgeContractAddress` error is returned. This ensures only authorized entities can add TXIDs for monitoring.

4. **Add TXID to Monitored List**:
   - The TXID is added to the `monitored_txids` vector in the Oracle Contract State. This vector keeps track of all TXIDs currently being monitored by the oracle.

5. **Initialize Pending Payment Account**:
   - A pending payment account is initialized for the TXID using the `HandlePendingPayment` context.
   - The pending payment account is initialized with the expected amount for monitoring (`COST_IN_LAMPORTS_OF_ADDING_PASTEL_TXID_FOR_MONITORING`) and the payment status is set to `Pending`.

6. **Logging**:
   - Messages are logged to indicate the successful addition of the TXID for monitoring and the initialization of the pending payment account.

### 4. Data Report Submission

**Objective**: Collect and validate data reports from contributors for the monitored TXIDs.

#### Submit Data Report:

1. **Function Definition**:
   - The `submit_data_report` function is defined in the `solana_pastel_oracle_program` module.
   - It takes a `Context<SubmitDataReport>`, `txid: String`, `txid_status_str: String`, `pastel_ticket_type_str: String`, `first_6_characters_hash: String`, and `contributor_reward_address: Pubkey` as parameters.

2. **Report Creation**:
   - Inside the `submit_data_report_helper` function, the parameters are used to create a `PastelTxStatusReport` struct.
   - The `txid_status_str` is converted to the `TxidStatus` enum, and the `pastel_ticket_type_str` is converted to the `PastelTicketType` enum.
   - The current Unix timestamp is fetched and included in the report.

3. **Validation**:
   - The `validate_data_contributor_report` function is called to validate the report.
   - It ensures the TXID is not empty, the TXID status and pastel ticket type are valid, and the file hash is the correct length and contains only hex characters. If any of these validations fail, appropriate errors (e.g., `OracleError::InvalidTxid`, `OracleError::InvalidTxidStatus`, `OracleError::InvalidFileHashLength`, `OracleError::MissingFileHash`) are returned.

4. **Contributor Verification**:
   - The program checks if the contributor is registered and not banned. If the contributor's public key is not found in the `ContributorDataAccount`, an `OracleError::ContributorNotRegistered` error is returned.
   - The `calculate_is_banned` method on the `Contributor` struct is called to determine if the contributor is currently banned. If so, an `OracleError::ContributorBanned` error is returned.

5. **Common and Specific Report Data**:
   - Common report data (`CommonReportData`) is extracted from the report and either found or added to the `TempTxStatusReportAccount`.
   - Specific report data (`SpecificReportData`) is created with the contributor's reward address, timestamp, and a reference to the common data.

6. **Temporary Report Entry**:
   - A temporary report (`TempTxStatusReport`) is created with the common data reference and specific data.
   - This temporary report is added to the `TempTxStatusReportAccount`.

7. **Update Submission Count**:
   - The `update_submission_count` function is called to update the submission count for the TXID in the `TxidSubmissionCountsAccount`.

8. **Aggregate Consensus Data**:
   - The `aggregate_consensus_data` function is called to update the consensus data based on the submitted report. The contributor's compliance and reliability scores are used to weight the report.

9. **Consensus Calculation Check**:
   - The `should_calculate_consensus` function is called to determine if enough reports have been submitted to calculate consensus. If so, the `calculate_consensus` function is called to compute the consensus and update contributor scores.

10. **Logging**:
    - Messages are logged to indicate successful report submission, updated submission counts, and consensus calculation if triggered.

### 5. Consensus Calculation

**Objective**: Reach a consensus on the status and hash of each TXID based on the submitted reports.

#### Update Submission Count:

1. **Function Definition**:
   - The `update_submission_count` function is responsible for updating the submission count for a TXID.
   - It takes the `txid_submission_counts_account` and `txid` as parameters.

2. **Timestamp Retrieval**:
   - The current Unix timestamp is fetched using `Clock::get()?.unix_timestamp as u64`.

3. **Check for Existing TXID**:
   - The function checks if the TXID already exists in the submission counts.
   - If it does, the existing count is incremented, and the `last_updated` timestamp is updated.
   - If the TXID does not exist, a new `TxidSubmissionCount` entry is created with an initial count of 1 and the current timestamp.

4. **Return**:
   - The function returns `Ok(())` after updating the submission count.

#### Compute Consensus:

1. **Function Definition**:
   - The `compute_consensus` function is responsible for determining the consensus status and hash for a TXID.
   - It takes `aggregated_data` as a parameter.

2. **Status Aggregation**:
   - The function iterates over the `status_weights` array in `aggregated_data`.
   - It selects the status with the highest weight (i.e., the status reported by the most contributors) as the consensus status.

3. **Hash Aggregation**:
   - The function iterates over the `hash_weights` vector in `aggregated_data`.
   - It selects the hash with the highest weight as the consensus hash.

4. **Return**:
   - The function returns a tuple containing the consensus status and hash.

#### Update Contributor Scores:

1. **Function Definition**:
   - The `update_contributor` function is responsible for updating a contributor's compliance and reliability scores based on the accuracy of their reports.
   - It takes a mutable reference to a `Contributor`, the current timestamp, and a boolean indicating whether the report was accurate.

2. **Score Updates**:
   - If the report is accurate:
     - The contributor's `total_reports_submitted` and `accurate_reports_count` are incremented.
     - The contributor's `current_streak` is incremented.
     - The compliance score is increased based on dynamic scaling and time weight.
   - If the report is inaccurate:
     - The contributor's `total_reports_submitted` is incremented.
     - The `current_streak` is reset to 0.
     - The `consensus_failures` count is incremented.
     - The compliance score is decreased based on a penalty.

3. **Decay and Reliability Factor**:
   - The compliance score is decayed over time.
   - The reliability score is calculated based on the ratio of accurate reports to total reports.
   - The compliance score is scaled based on the reliability factor and logistic scaling.

4. **Log Scores**:
   - The updated scores are logged for debugging purposes.

#### Post-Consensus Cleanup:

1. **Function Definition**:
   - The `post_consensus_tasks` function is responsible for cleaning up old or unnecessary data after consensus is reached.
   - It takes references to `txid_submission_counts_account`, `aggregated_data_account`, `temp_report_account`, `contributor_data_account`, and the `txid`.

2. **Apply Permanent Bans**:
   - The `apply_permanent_bans` function removes contributors who have been permanently banned from the `ContributorDataAccount`.

3. **Cleanup Temporary Reports**:
   - The function retains only the temporary reports that do not match the `txid` and are within the `DATA_RETENTION_PERIOD`.

4. **Cleanup Aggregated Consensus Data**:
   - The function retains only the consensus data that is within the `DATA_RETENTION_PERIOD`.

5. **Cleanup Submission Counts**:
   - The function retains only the submission counts that are within the `SUBMISSION_COUNT_RETENTION_PERIOD`.

6. **Logging**:
   - Messages are logged to indicate the completion of cleanup tasks.

### 6. Payment Processing

**Objective**: Process payments for monitoring TXIDs and update payment status.

#### Process Payment:

1. **Function Definition**:
   - The `process_payment` function is defined in the `solana_pastel_oracle_program` module.
   - It takes a `Context<ProcessPayment>`, `txid: String`, and `amount: u64` as parameters.

2. **Context Definition**:
   - The `ProcessPayment` context defines the accounts involved in the payment process.
   - This includes the source account, oracle contract state, and pending payment account.

3. **Function Execution**:
   - The `process_payment_helper` function is called within `process_payment` to handle the payment logic.
   - The function takes the context, TXID, and payment amount as parameters.

4. **Pending Payment Account Validation**:
   - The function first checks that the `pending_payment_account` corresponds to the provided TXID.
   - If the TXID in the `pending_payment_account` does not match the provided TXID, an error `OracleError::PaymentNotFound` is returned.

5. **Payment Amount Validation**:
   - The function ensures that the payment amount matches the expected amount in the `pending_payment_account`.
   - If the payment amount does not match, an error `OracleError::InvalidPaymentAmount` is returned.

6. **Update Payment Status**:
   - Once the payment is validated, the payment status in the `pending_payment_account` is updated to `Received`.
   - This indicates that the expected payment amount has been successfully received.

7. **Logging**:
   - Throughout the process, various log messages (`msg!`) are used to track the progress and any issues encountered.
   - This includes logging when the payment is successfully processed and if there are any validation errors.

### 7. Reward Distribution

**Objective**: Distribute rewards to eligible contributors based on their compliance and reliability scores.

#### Request Reward:

1. **Function Definition**:
   - The `request_reward` function is defined in the `solana_pastel_oracle_program` module.
   - It takes a `Context<RequestReward>` and a `contributor_address: Pubkey` as parameters.

2. **Context Definition**:
   - The `RequestReward` context defines the accounts involved in the reward distribution process.
   - This includes the reward pool account, the oracle contract state, the contributor data account, and the contributor’s account.

3. **Function Execution**:
   - The `request_reward_helper` function is called with the context and contributor's public key.

4. **Contributor Validation**:
   - The function first finds the contributor in the `ContributorDataAccount` using the provided public key.
   - If the contributor is not found, an error `OracleError::UnregisteredOracle` is returned.

5. **Eligibility Check**:
   - The function checks if the contributor meets the eligibility criteria, which include:
     - A minimum number of reports submitted.
     - Compliance score above `MIN_COMPLIANCE_SCORE_FOR_REWARD`.
     - Reliability score above `MIN_RELIABILITY_SCORE_FOR_REWARD`.
   - If the contributor is not eligible, an error `OracleError::NotEligibleForReward` is returned.

6. **Reward Amount Calculation**:
   - The reward amount is determined based on the `BASE_REWARD_AMOUNT_IN_LAMPORTS`.
   - This base amount can be scaled based on additional criteria if needed.

7. **Funds Availability Check**:
   - The function ensures that the reward pool account has sufficient funds to cover the reward amount.
   - If the reward pool has insufficient funds, an error `OracleError::InsufficientFunds` is returned.

8. **Transfer Reward**:
   - The function transfers the reward amount from the reward pool account to the contributor's account.
   - This involves decrementing the lamports in the reward pool account and incrementing the lamports in the contributor's account.

## Explanation of Typescript Testing Script

### Initialization

1. **Environment Setup**:
   - The program sets up environment variables for the Anchor provider URL and logging.
   - It initializes the Anchor provider and sets it as the default provider for the program.

2. **Program and Account Setup**:
   - The program ID and the oracle contract state account are defined.
   - Public Key Address (PDA) accounts for various purposes (e.g., reward pool, fee receiving contract, contributor data, TXID submission counts, and aggregated consensus data) are generated using `web3.PublicKey.findProgramAddressSync`.

3. **Funding the Oracle Contract State Account**:
   - The oracle contract state account is funded with enough SOL to cover the rent-exempt minimum balance required by the Solana network.

4. **Initialization and Reallocation**:
   - The program initializes the oracle contract state, setting up initial parameters and PDAs.
   - The contract state is expanded incrementally to accommodate additional data, up to a maximum size of 200KB.

### Contributor Registration

1. **Contributor Registration**:
   - Contributors are registered by generating new keypairs and paying a registration entrance fee.
   - The program transfers the registration fee to the fee receiving contract account PDA.
   - New contributors are added to the ContributorDataAccount PDA.

### TXID Monitoring

1. **Adding TXIDs for Monitoring**:
   - The program generates random TXIDs and adds them for monitoring.
   - Each TXID is associated with a pending payment account, which is initialized with the expected payment amount.
   - The TXID is then added to the monitored list in the oracle contract state.

2. **Verification of Monitored TXIDs**:
   - The program verifies that all monitored TXIDs have corresponding pending payment structs.
   - It checks the payment status and ensures the expected amount matches the defined cost.

### Data Report Submission

1. **Submitting Data Reports**:
   - Contributors submit data reports for the monitored TXIDs.
   - Reports include TXID status, pastel ticket type, and a random file hash.
   - The program handles both correct and incorrect submissions based on a predefined error probability.

2. **Consensus Data**:
   - The program aggregates consensus data for each TXID based on the submitted reports.
   - It determines the majority consensus status and logs the results.

### Payment Processing

1. **Processing Payments**:
   - The program processes payments for the monitored TXIDs by transferring the payment amount from the admin account to the fee-receiving contract PDA.
   - It updates the payment status to "Received" and verifies the changes.

### Reward Distribution

1. **Eligibility Check and Reward Distribution**:
   - The program checks if contributors meet the criteria for reward eligibility based on their compliance and reliability scores.
   - Eligible contributors receive rewards from the reward pool account PDA.
   - The program verifies that reward distribution is accurate and logs the changes in account balances.
