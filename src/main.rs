use anchor_lang::prelude::*;

declare_id!("CMdhyZcTJNWU44qHryr5WbsTvd5zLZYqSgwG5SrDfDim");

#[program]
pub mod gousd {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        default_admin: Pubkey,
        freezer: Pubkey,
        supply_controller: Pubkey,
        blacklister: Pubkey,
        rescuer: Pubkey,
        aggregator_pubkey: Pubkey,
        acceptable_pof_delay: u64,
        mint_cap_per_transaction: u64,
    ) -> Result<()> {
        let state = &mut ctx.accounts.gousd_state;
        state.roles.default_admin = default_admin;
        state.roles.freezer = freezer;
        state.roles.supply_controller = supply_controller;
        state.roles.blacklister = blacklister;
        state.roles.rescuer = rescuer;
        state.paused = false;
        state.total_supply = 0;
        state.decimals = 6;
        state.proof_of_reserve_feed = aggregator_pubkey;
        state.acceptable_proof_of_reserve_delay = acceptable_pof_delay;
        state.mint_cap_per_transaction = mint_cap_per_transaction;
        emit!(ProofOfReserveFeedSet {
            new_feed: aggregator_pubkey
        });
        emit!(AcceptableProofOfReserveDelaySet {
            new_time_delay: acceptable_pof_delay
        });
        emit!(MintCapPerTransactionSet {
            new_limit: mint_cap_per_transaction
        });
        Ok(())
    }

    pub fn set_proof_of_reserve_feed(ctx: Context<AdminOnly>, new_feed: Pubkey) -> Result<()> {
        let state = &mut ctx.accounts.gousd_state;
        require_keys_eq!(
            ctx.accounts.signer.key(),
            state.roles.default_admin,
            ErrorCode::Unauthorized
        );
        validate_proof_of_reserve(
            &ctx.accounts.aggregator,
            0,
            false,
            state.total_supply,
            state.acceptable_proof_of_reserve_delay,
        )?;
        state.proof_of_reserve_feed = new_feed;
        emit!(ProofOfReserveFeedSet { new_feed });
        Ok(())
    }

    pub fn set_acceptable_proof_of_reserve_time_delay(
        ctx: Context<AdminOnly>,
        new_delay: u64,
    ) -> Result<()> {
        let state = &mut ctx.accounts.gousd_state;
        require_keys_eq!(
            ctx.accounts.signer.key(),
            state.roles.default_admin,
            ErrorCode::Unauthorized
        );
        require!(new_delay > 0, ErrorCode::InvalidTimeDelay);
        state.acceptable_proof_of_reserve_delay = new_delay;
        emit!(AcceptableProofOfReserveDelaySet {
            new_time_delay: new_delay
        });
        Ok(())
    }

    pub fn set_mint_cap_per_transaction(ctx: Context<AdminOnly>, new_limit: u64) -> Result<()> {
        let state = &mut ctx.accounts.gousd_state;
        require_keys_eq!(
            ctx.accounts.signer.key(),
            state.roles.default_admin,
            ErrorCode::Unauthorized
        );
        require!(new_limit > 0, ErrorCode::InvalidAmount);
        state.mint_cap_per_transaction = new_limit;
        emit!(MintCapPerTransactionSet { new_limit });
        Ok(())
    }

    pub fn destroy_blacklisted_funds(ctx: Context<DestroyBlacklist>, user: Pubkey) -> Result<()> {
        let state = &mut ctx.accounts.gousd_state;
        require_keys_eq!(
            ctx.accounts.signer.key(),
            state.roles.supply_controller,
            ErrorCode::Unauthorized
        );
        let user_idx = find_or_create_user_record_index(state, &user)?;
        {
            let user_record = &mut state.balances[user_idx];
            require!(user_record.is_blacklisted, ErrorCode::SenderNotBlacklisted);
            let balance_before = user_record.balance;
            user_record.balance = 0;
            state.total_supply = state.total_supply.saturating_sub(balance_before);
            emit!(BurnEvent {
                from: user,
                amount: balance_before
            });
        }
        Ok(())
    }

    pub fn rescue_tokens(ctx: Context<RescueTokens>, amount: u64) -> Result<()> {
        let state = &ctx.accounts.gousd_state;
        require_keys_eq!(
            ctx.accounts.signer.key(),
            state.roles.rescuer,
            ErrorCode::Unauthorized
        );
        require!(amount > 0, ErrorCode::InvalidAmount);
        let mut_state = &mut ctx.accounts.gousd_state;
        let recipient_idx =
            find_or_create_user_record_index(mut_state, &ctx.accounts.recipient.key())?;
        let vault_record = &mut ctx.accounts.vault;
        require!(
            vault_record.balance >= amount,
            ErrorCode::InsufficientVaultBalance
        );
        {
            let recipient_record = &mut mut_state.balances[recipient_idx];
            require!(
                !recipient_record.is_blacklisted,
                ErrorCode::RecipientBlacklisted
            );
            vault_record.balance = vault_record.balance.saturating_sub(amount);
            recipient_record.balance = recipient_record.balance.saturating_add(amount);
        }
        emit!(TokensRescuedEvent {
            token: ctx.accounts.vault.key(),
            recipient: ctx.accounts.recipient.key(),
            amount
        });
        Ok(())
    }

    pub fn pause(ctx: Context<FreezeOrUnfreeze>) -> Result<()> {
        let state = &mut ctx.accounts.gousd_state;
        require_keys_eq!(
            ctx.accounts.signer.key(),
            state.roles.freezer,
            ErrorCode::Unauthorized
        );
        state.paused = true;
        Ok(())
    }

    pub fn unpause(ctx: Context<FreezeOrUnfreeze>) -> Result<()> {
        let state = &mut ctx.accounts.gousd_state;
        require_keys_eq!(
            ctx.accounts.signer.key(),
            state.roles.freezer,
            ErrorCode::Unauthorized
        );
        state.paused = false;
        Ok(())
    }

    pub fn transfer(ctx: Context<Transfer>, to: Pubkey, amount: u64) -> Result<()> {
        let state = &mut ctx.accounts.gousd_state;
        require!(!state.paused, ErrorCode::ContractPaused);
        let signer_key = ctx.accounts.signer.key();
        let from_idx = find_or_create_user_record_index(state, &signer_key)?;
        let to_idx = find_or_create_user_record_index(state, &to)?;
        {
            let from_rec = &state.balances[from_idx];
            require!(!from_rec.is_blacklisted, ErrorCode::SenderBlacklisted);
            require!(from_rec.balance >= amount, ErrorCode::InsufficientBalance);
            let to_rec = &state.balances[to_idx];
            require!(!to_rec.is_blacklisted, ErrorCode::RecipientBlacklisted);
        }
        {
            let from_balance = &mut state.balances[from_idx].balance;
            *from_balance = from_balance.saturating_sub(amount);
            let to_balance = &mut state.balances[to_idx].balance;
            *to_balance = to_balance.saturating_add(amount);
        }
        emit!(TransferEvent {
            from: signer_key,
            to,
            amount
        });
        Ok(())
    }

    pub fn approve(ctx: Context<Transfer>, spender: Pubkey, amount: u64) -> Result<()> {
        let state = &mut ctx.accounts.gousd_state;
        require!(!state.paused, ErrorCode::ContractPaused);
        let owner = ctx.accounts.signer.key();
        let owner_idx = find_or_create_user_record_index(state, &owner)?;
        {
            let owner_record = &state.balances[owner_idx];
            require!(!owner_record.is_blacklisted, ErrorCode::SenderBlacklisted);
        }
        let allowances = &mut state.allowances;
        if let Some(a) = allowances
            .iter_mut()
            .find(|a| a.owner == owner && a.spender == spender)
        {
            a.amount = amount;
        } else {
            allowances.push(Allowance {
                owner,
                spender,
                amount,
            });
        }
        Ok(())
    }

    pub fn transfer_from(
        ctx: Context<Transfer>,
        from: Pubkey,
        to: Pubkey,
        amount: u64,
    ) -> Result<()> {
        let state = &mut ctx.accounts.gousd_state;
        require!(!state.paused, ErrorCode::ContractPaused);
        let spender = ctx.accounts.signer.key();
        require!(
            !is_blacklisted(state, &spender),
            ErrorCode::SpenderBlacklisted
        );
        require!(!is_blacklisted(state, &from), ErrorCode::SenderBlacklisted);
        let from_idx = find_or_create_user_record_index(state, &from)?;
        let to_idx = find_or_create_user_record_index(state, &to)?;
        let needed_allowance = {
            let allowances = &mut state.allowances;
            let allowance_obj = allowances
                .iter_mut()
                .find(|a| a.owner == from && a.spender == spender)
                .ok_or(ErrorCode::AllowanceNotFound)?;
            require!(
                allowance_obj.amount >= amount,
                ErrorCode::InsufficientAllowance
            );
            allowance_obj.amount.saturating_sub(amount)
        };
        {
            let from_rec = &state.balances[from_idx];
            require!(from_rec.balance >= amount, ErrorCode::InsufficientBalance);
            let to_rec = &state.balances[to_idx];
            require!(!to_rec.is_blacklisted, ErrorCode::RecipientBlacklisted);
        }
        {
            let from_balance = &mut state.balances[from_idx].balance;
            *from_balance = from_balance.saturating_sub(amount);
            let to_balance = &mut state.balances[to_idx].balance;
            *to_balance = to_balance.saturating_add(amount);
            let allowances = &mut state.allowances;
            if let Some(a) = allowances
                .iter_mut()
                .find(|a| a.owner == from && a.spender == spender)
            {
                a.amount = needed_allowance;
            }
        }
        emit!(TransferEvent { from, to, amount });
        Ok(())
    }

    pub fn mint(ctx: Context<SupplyControl>, to: Pubkey, amount: u64) -> Result<()> {
        let state = &mut ctx.accounts.gousd_state;
        let signer = ctx.accounts.signer.key();
        require_keys_eq!(
            signer,
            state.roles.supply_controller,
            ErrorCode::Unauthorized
        );
        require!(!is_blacklisted(state, &to), ErrorCode::RecipientBlacklisted);
        require!(
            amount <= state.mint_cap_per_transaction,
            ErrorCode::ExceedsMintTransactionCap
        );
        validate_proof_of_reserve(
            &ctx.accounts.aggregator,
            amount,
            false,
            state.total_supply,
            state.acceptable_proof_of_reserve_delay,
        )?;
        let to_idx = find_or_create_user_record_index(state, &to)?;
        {
            let to_record = &mut state.balances[to_idx];
            to_record.balance = to_record.balance.saturating_add(amount);
        }
        state.total_supply = state.total_supply.saturating_add(amount);
        emit!(MintEvent { to, amount });
        Ok(())
    }

    pub fn mint_batch(
        ctx: Context<SupplyControl>,
        tos: Vec<Pubkey>,
        amounts: Vec<u64>,
    ) -> Result<()> {
        let state = &mut ctx.accounts.gousd_state;
        let signer = ctx.accounts.signer.key();
        require_keys_eq!(
            signer,
            state.roles.supply_controller,
            ErrorCode::Unauthorized
        );
        require!(tos.len() == amounts.len(), ErrorCode::ArrayLengthsMismatch);
        for &amt in amounts.iter() {
            require!(
                amt <= state.mint_cap_per_transaction,
                ErrorCode::ExceedsMintTransactionCap
            );
        }
        let total_mint: u64 = amounts.iter().sum();
        validate_proof_of_reserve(
            &ctx.accounts.aggregator,
            total_mint,
            true,
            state.total_supply,
            state.acceptable_proof_of_reserve_delay,
        )?;
        for (i, to) in tos.iter().enumerate() {
            require!(!is_blacklisted(state, to), ErrorCode::RecipientBlacklisted);
            let idx = find_or_create_user_record_index(state, to)?;
            {
                let record = &mut state.balances[idx];
                record.balance = record.balance.saturating_add(amounts[i]);
            }
            state.total_supply = state.total_supply.saturating_add(amounts[i]);
            emit!(MintEvent {
                to: *to,
                amount: amounts[i]
            });
        }
        Ok(())
    }

    pub fn burn(ctx: Context<SupplyControl>, from: Pubkey, amount: u64) -> Result<()> {
        let state = &mut ctx.accounts.gousd_state;
        let signer = ctx.accounts.signer.key();
        require_keys_eq!(
            signer,
            state.roles.supply_controller,
            ErrorCode::Unauthorized
        );
        require!(!is_blacklisted(state, &from), ErrorCode::SenderBlacklisted);
        let from_idx = find_or_create_user_record_index(state, &from)?;
        {
            let from_record = &mut state.balances[from_idx];
            require!(
                from_record.balance >= amount,
                ErrorCode::InsufficientBalance
            );
            from_record.balance = from_record.balance.saturating_sub(amount);
        }
        state.total_supply = state.total_supply.saturating_sub(amount);
        emit!(BurnEvent { from, amount });
        Ok(())
    }
}

#[account]
pub struct GoUSDState {
    pub roles: Roles,
    pub paused: bool,
    pub decimals: u8,
    pub total_supply: u64,
    pub proof_of_reserve_feed: Pubkey,
    pub acceptable_proof_of_reserve_delay: u64,
    pub mint_cap_per_transaction: u64,
    pub balances: Vec<UserRecord>,
    pub allowances: Vec<Allowance>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct Roles {
    pub default_admin: Pubkey,
    pub freezer: Pubkey,
    pub supply_controller: Pubkey,
    pub blacklister: Pubkey,
    pub rescuer: Pubkey,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct UserRecord {
    pub user: Pubkey,
    pub balance: u64,
    pub is_blacklisted: bool,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct Allowance {
    pub owner: Pubkey,
    pub spender: Pubkey,
    pub amount: u64,
}

impl GoUSDState {
    pub const MAX_SIZE: usize = 8
        + (5 * 32)
        + 1
        + 1
        + 8
        + 32
        + 8
        + 8
        + (4 + 2000 * (32 + 8 + 1))
        + (4 + 2000 * (32 + 32 + 8));
}

fn find_or_create_user_record_index(state: &mut GoUSDState, user: &Pubkey) -> Result<usize> {
    if let Some(idx) = state.balances.iter().position(|r| r.user == *user) {
        Ok(idx)
    } else {
        state.balances.push(UserRecord {
            user: *user,
            balance: 0,
            is_blacklisted: false,
        });
        Ok(state.balances.len() - 1)
    }
}

fn is_blacklisted(state: &GoUSDState, user: &Pubkey) -> bool {
    state.balances.iter().any(|r| r.user == *user && r.is_blacklisted)
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = signer,
        space = 8 + GoUSDState::MAX_SIZE
    )]
    pub gousd_state: Account<'info, GoUSDState>,
    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AdminOnly<'info> {
    #[account(mut)]
    pub gousd_state: Account<'info, GoUSDState>,
    pub aggregator: UncheckedAccount<'info>,
    pub signer: Signer<'info>,
}

#[derive(Accounts)]
pub struct DestroyBlacklist<'info> {
    #[account(mut)]
    pub gousd_state: Account<'info, GoUSDState>,
    pub signer: Signer<'info>,
}

#[derive(Accounts)]
pub struct RescueTokens<'info> {
    #[account(mut)]
    pub gousd_state: Account<'info, GoUSDState>,
    #[account(mut)]
    pub vault: Account<'info, TokenHolder>,
    #[account(mut)]
    pub recipient: UncheckedAccount<'info>,
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[account]
pub struct TokenHolder {
    pub balance: u64,
}

#[derive(Accounts)]
pub struct FreezeOrUnfreeze<'info> {
    #[account(mut)]
    pub gousd_state: Account<'info, GoUSDState>,
    pub signer: Signer<'info>,
}

#[derive(Accounts)]
pub struct Transfer<'info> {
    #[account(mut)]
    pub gousd_state: Account<'info, GoUSDState>,
    pub signer: Signer<'info>,
}

#[derive(Accounts)]
pub struct SupplyControl<'info> {
    #[account(mut)]
    pub gousd_state: Account<'info, GoUSDState>,
    pub aggregator: UncheckedAccount<'info>,
    pub signer: Signer<'info>,
}

#[account]
pub struct AggregatorAccountData {
    pub reserves: u64,
    pub updated_at: i64,
    pub decimals: u8,
}

fn validate_proof_of_reserve<'info>(
    aggregator_acc: &UncheckedAccount<'info>,
    minted_amount: u64,
    is_batch: bool,
    total_supply: u64,
    acceptable_delay: u64,
) -> Result<()> {
    let aggregator_data = Account::<AggregatorAccountData>::try_from(aggregator_acc)
        .map_err(|_| ErrorCode::InvalidPoRData)?;
    let aggregator: &AggregatorAccountData = aggregator_data.as_ref();
    require!(aggregator.reserves > 0, ErrorCode::InvalidPoRData);
    let clock = Clock::get()?;
    let now_ts = clock.unix_timestamp;
    require!(
        aggregator.updated_at + acceptable_delay as i64 >= now_ts,
        ErrorCode::PoROutdated
    );
    let token_decimals = 6;
    require!(
        aggregator.decimals >= token_decimals && aggregator.decimals <= 18,
        ErrorCode::InvalidDecimals
    );
    let diff = aggregator.decimals as i32 - token_decimals as i32;
    let mut scaled_reserves = aggregator.reserves;
    if diff > 0 {
        scaled_reserves = aggregator.reserves / 10u64.pow(diff as u32);
    }
    let new_supply = if is_batch {
        total_supply
    } else {
        total_supply.saturating_add(minted_amount)
    };
    require!(
        new_supply <= scaled_reserves,
        ErrorCode::SupplyExceedsReserves
    );
    Ok(())
}

#[event]
pub struct ProofOfReserveFeedSet {
    #[index]
    pub new_feed: Pubkey,
}

#[event]
pub struct AcceptableProofOfReserveDelaySet {
    pub new_time_delay: u64,
}

#[event]
pub struct MintCapPerTransactionSet {
    pub new_limit: u64,
}

#[event]
pub struct BurnEvent {
    #[index]
    pub from: Pubkey,
    pub amount: u64,
}

#[event]
pub struct MintEvent {
    #[index]
    pub to: Pubkey,
    pub amount: u64,
}

#[event]
pub struct TokensRescuedEvent {
    #[index]
    pub token: Pubkey,
    pub recipient: Pubkey,
    pub amount: u64,
}

#[event]
pub struct TransferEvent {
    #[index]
    pub from: Pubkey,
    #[index]
    pub to: Pubkey,
    pub amount: u64,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Unauthorized action")]
    Unauthorized,
    #[msg("Invalid address")]
    InvalidAddress,
    #[msg("Invalid time delay")]
    InvalidTimeDelay,
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("Invalid decimals in aggregator")]
    InvalidDecimals,
    #[msg("Invalid proof of reserve data")]
    InvalidPoRData,
    #[msg("Proof of reserve data is outdated")]
    PoROutdated,
    #[msg("Transaction exceeds the mint cap")]
    ExceedsMintTransactionCap,
    #[msg("Supply exceeds the reserves")]
    SupplyExceedsReserves,
    #[msg("Sender is blacklisted")]
    SenderBlacklisted,
    #[msg("Sender is not blacklisted when expected")]
    SenderNotBlacklisted,
    #[msg("Spender is blacklisted")]
    SpenderBlacklisted,
    #[msg("Recipient is blacklisted")]
    RecipientBlacklisted,
    #[msg("Array lengths mismatch")]
    ArrayLengthsMismatch,
    #[msg("Transfer paused")]
    ContractPaused,
    #[msg("Insufficient balance")]
    InsufficientBalance,
    #[msg("Insufficient vault balance")]
    InsufficientVaultBalance,
    #[msg("Allowance record not found")]
    AllowanceNotFound,
    #[msg("Insufficient allowance")]
    InsufficientAllowance,
}
