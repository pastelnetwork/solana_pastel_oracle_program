use crate::state::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct SetBridgeContract<'info> {
    #[account(mut, has_one = admin_pubkey)]
    pub oracle_contract_state: Account<'info, OracleContractState>,
    pub admin_pubkey: Signer<'info>,
}

impl<'info> SetBridgeContract<'info> {
    pub fn set_bridge_contract(
        ctx: Context<SetBridgeContract>,
        bridge_contract_pubkey: Pubkey,
    ) -> Result<()> {
        let state = &mut ctx.accounts.oracle_contract_state;
        state.bridge_contract_pubkey = bridge_contract_pubkey;
        msg!(
            "Bridge contract pubkey updated: {:?}",
            bridge_contract_pubkey
        );
        Ok(())
    }
}
