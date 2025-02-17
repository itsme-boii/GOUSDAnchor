use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    metadata::{
        create_metadata_accounts_v3, mpl_token_metadata::types::DataV2, CreateMetadataAccountsV3,
        Metadata,
    },
    token::{
        self, burn, freeze_account, mint_to, thaw_account, Mint, MintTo, Token, TokenAccount,
        Transfer as SplTransfer,
    },
};
use pyth_sdk_solana::load_price_feed_from_account_info; // Validating Oracle

declare_id!("Eb7G2XzP3bgiRYBsyNJksfn7EhnaCe86N4tuajgL6gzp");

pub const MAXIMUM_AGE: u64 = 60; // One minute
pub const FEED_ID: &str = "0xef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d";
pub const SOL_USDC_FEED: &str = "HovQMDrbAgAYPCmHVSrezcSmkMtXSSUsLDFANExrZh2J";
pub const STALENESS_THRESHOLD: u64 = 300; // staleness threshold in seconds

#[program]
pub mod go_usd {
    use super::*;

    //Initialise the State
    pub fn initialize(ctx: Context<Initialize>, default_admin_delay: i64) -> Result<()> {
        let state = &mut ctx.accounts.state;
        state.default_admin = ctx.accounts.default_admin.key();
        state.default_admin_delay = default_admin_delay;
        state.freezer = ctx.accounts.freezer.key();
        state.supply_controller = ctx.accounts.supply_controller.key();
        state.upgrader = ctx.accounts.upgrader.key();
        state.rescuer = ctx.accounts.rescuer.key();
        state.paused = false;
        state.total_supply = 0;
        state.mint_cap_per_transaction = 1_000_000 * 1_000_000;
        state.acceptable_proof_of_reserve_delay = 24 * 60 * 60; // 24 hours in seconds

        // Validate and set proof of reserve feed
        require!(
            ctx.accounts.proof_of_reserve_feed.key() != Pubkey::default(),
            ErrorCode::InvalidAddress
        );
        state.proof_of_reserve_feed = ctx.accounts.proof_of_reserve_feed.key();

        Ok(())
    }

    //Function to create token metadata and will be called by us
    pub fn create_token(
        ctx: Context<CreateToken>,
        token_decimals: u8,
        token_name: String,
        token_symbol: String,
        token_uri: String,
    ) -> Result<()> {
        msg!("Creating metadata account");

        if token_decimals <= 0 {
            return Err(ErrorCode::InvalidTokenDecimal.into());
        }

        msg!("Mint _token_decimals: {}", &token_decimals);
        create_metadata_accounts_v3(
            CpiContext::new(
                ctx.accounts.token_metadata_program.to_account_info(),
                CreateMetadataAccountsV3 {
                    metadata: ctx.accounts.metadata_account.to_account_info(),
                    mint: ctx.accounts.mint_account.to_account_info(),
                    mint_authority: ctx.accounts.payer.to_account_info(),
                    update_authority: ctx.accounts.payer.to_account_info(),
                    payer: ctx.accounts.payer.to_account_info(),
                    system_program: ctx.accounts.system_program.to_account_info(),
                    rent: ctx.accounts.rent.to_account_info(),
                },
            ),
            DataV2 {
                name: token_name,
                symbol: token_symbol,
                uri: token_uri,
                seller_fee_basis_points: 0,
                creators: None,
                collection: None,
                uses: None,
            },
            false,
            true,
            None,
        )?;

        msg!("Token created successfully.");

        Ok(())
    }

    //Function to mint token with specific amount
    pub fn mint(ctx: Context<MintGousd>, amount: u64) -> Result<()> {
        let state = &ctx.accounts.state;
        require!(!state.paused, ErrorCode::ContractPaused);
        require!(
            ctx.accounts.authority.key() == state.supply_controller,
            ErrorCode::UnauthorizedSupplyController
        );
        require!(
            amount <= state.mint_cap_per_transaction,
            ErrorCode::ExceedsMintTransactionCap
        );

        // Validate proof of reserve
        validate_proof_of_reserve(
            Context::new(
                ctx.program_id,
                &mut ValidateProofOfReserve {
                    price_feed: ctx.accounts.price_update.clone(),
                },
                &[],
                ValidateProofOfReserveBumps {},
            ),
            amount,
            state.total_supply,
            state.acceptable_proof_of_reserve_delay,
            false,
        )?;

        token::mint_to(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token::MintTo {
                    mint: ctx.accounts.gousd_mint.to_account_info(),
                    to: ctx.accounts.recipient.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
            ),
            amount,
        )?;

        // Update state
        let state = &mut ctx.accounts.state;
        state.total_supply = state.total_supply.checked_add(amount).unwrap();

        emit!(MintEvent {
            to: ctx.accounts.recipient.key(),
            amount,
        });

        Ok(())
    }

    //Function to burn the amount of tokens
    pub fn burn(ctx: Context<BurnGousd>, amount: u64) -> Result<()> {
        let state = &ctx.accounts.state;
        require!(!state.paused, ErrorCode::ContractPaused);
        require!(
            ctx.accounts.authority.key() == state.supply_controller,
            ErrorCode::UnauthorizedSupplyController
        );
        // Burn tokens
        token::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token::Burn {
                    mint: ctx.accounts.gousd_mint.to_account_info(),
                    from: ctx.accounts.from_account.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
            ),
            amount,
        )?;

        // Update state
        let state = &mut ctx.accounts.state;
        state.total_supply = state.total_supply.checked_sub(amount).unwrap();

        emit!(BurnEvent {
            from: ctx.accounts.from.key(),
            amount,
        });

        Ok(())
    }

    //this function pause the state
    pub fn pause(ctx: Context<AdminOnly>) -> Result<()> {
        require!(
            ctx.accounts.authority.key() == ctx.accounts.state.freezer,
            ErrorCode::UnauthorizedFreezer
        );
        let state = &mut ctx.accounts.state;
        state.paused = true;
        Ok(())
    }

    pub fn unpause(ctx: Context<AdminOnly>) -> Result<()> {
        require!(
            ctx.accounts.authority.key() == ctx.accounts.state.freezer,
            ErrorCode::UnauthorizedFreezer
        );
        let state = &mut ctx.accounts.state;
        state.paused = false;
        Ok(())
    }

    pub fn freeze_user_account(ctx: Context<FreezeAccount>) -> Result<()> {
        let state = &ctx.accounts.state;
        require!(
            ctx.accounts.authority.key() == state.freezer,
            ErrorCode::UnauthorizedFreezer
        );

        // We assume `mint_authority` is the same as freeze authority, or a PDA with that authority.
        let seeds = &[b"mint_authority".as_ref(), &[state.mint_authority_bump]];
        let signer = &[&seeds[..]];

        freeze_account(CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            anchor_spl::token::FreezeAccount {
                account: ctx.accounts.token_to_freeze.to_account_info(),
                mint: ctx.accounts.gousd_mint.to_account_info(),
                authority: ctx.accounts.mint_authority.to_account_info(),
            },
            signer,
        ))?;

        emit!(FreezeEvent {
            account: ctx.accounts.token_to_freeze.key(),
        });

        Ok(())
    }

    pub fn unfreeze_account(ctx: Context<UnfreezeAccount>) -> Result<()> {
        let state = &ctx.accounts.state;
        require!(
            ctx.accounts.authority.key() == state.freezer,
            ErrorCode::UnauthorizedFreezer
        );

        let seeds = &[b"mint_authority".as_ref(), &[state.mint_authority_bump]];
        let signer = &[&seeds[..]];

        thaw_account(CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            anchor_spl::token::ThawAccount {
                account: ctx.accounts.token_to_unfreeze.to_account_info(),
                mint: ctx.accounts.gousd_mint.to_account_info(),
                authority: ctx.accounts.mint_authority.to_account_info(),
            },
            signer,
        ))?;

        emit!(UnfreezeEvent {
            account: ctx.accounts.token_to_unfreeze.key(),
        });

        Ok(())
    }

    pub fn set_mint_cap_per_transaction(ctx: Context<AdminOnly>, new_cap: u64) -> Result<()> {
        require!(
            ctx.accounts.authority.key() == ctx.accounts.state.default_admin,
            ErrorCode::UnauthorizedAdmin
        );
        require!(new_cap > 0, ErrorCode::InvalidAmount);
        let state = &mut ctx.accounts.state;
        state.mint_cap_per_transaction = new_cap;
        emit!(MintCapEvent { new_cap });
        Ok(())
    }

    pub fn set_proof_of_reserve_delay(ctx: Context<AdminOnly>, new_delay: i64) -> Result<()> {
        require!(
            ctx.accounts.authority.key() == ctx.accounts.state.default_admin,
            ErrorCode::UnauthorizedAdmin
        );
        require!(new_delay > 0, ErrorCode::InvalidTimeDelay);
        let state = &mut ctx.accounts.state;
        state.acceptable_proof_of_reserve_delay = new_delay;
        emit!(ProofOfReserveDelayEvent { new_delay });
        Ok(())
    }

    //pyth oracle to validate proof of reserve
    pub fn validate_proof_of_reserve(
        ctx: Context<ValidateProofOfReserve>,
        mint_amount: u64,
        current_supply: u64,
        acceptable_delay: i64,
        is_batch: bool,
    ) -> Result<()> {
        let price_account_info = &ctx.accounts.price_feed;
        let price_feed = load_price_feed_from_account_info(&price_account_info).unwrap();
        let current_timestamp = Clock::get()?.unix_timestamp;
        let current_price = price_feed
            .get_price_no_older_than(current_timestamp, STALENESS_THRESHOLD)
            .unwrap();

        require!(current_price.price > 0, ErrorCode::InvalidPoRData);
        require!(
            current_timestamp <= current_price.publish_time + acceptable_delay,
            ErrorCode::PoROutdated
        );

        let reserves = current_price.price as u64;
        if is_batch {
            require!(current_supply <= reserves, ErrorCode::SupplyExceedsReserves);
        } else {
            require!(
                current_supply.checked_add(mint_amount).unwrap() <= reserves,
                ErrorCode::SupplyExceedsReserves
            );
        }

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = payer, space = 8 + StateAccount::SIZE)]
    pub state: Account<'info, StateAccount>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub default_admin: Signer<'info>,

    pub freezer: Signer<'info>,
    pub supply_controller: Signer<'info>,
    pub upgrader: Signer<'info>,
    pub rescuer: Signer<'info>,
    /// CHECK: Validated in initialize
    pub proof_of_reserve_feed: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(token_decimals: u8)]
pub struct CreateToken<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        init,
        payer = payer,
        mint::decimals = token_decimals,
        mint::authority = payer.key(),
        mint::freeze_authority = payer.key(),

    )]
    pub mint_account: Account<'info, Mint>,
    /// CHECK: Address validated using constraint
    #[account(mut)]
    pub metadata_account: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
    pub token_metadata_program: Program<'info, Metadata>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct MintGousd<'info> {
    #[account(mut)]
    pub state: Account<'info, StateAccount>,
    #[account(mut)]
    pub gousd_mint: Account<'info, Mint>,
    /// CHECK: Validated in instruction
    pub proof_of_reserve_feed: AccountInfo<'info>,
    /// CHECK: PDA or known authority
    pub mint_authority: AccountInfo<'info>,
    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = gousd_mint,
        associated_token::authority = authority,
    )]
    pub recipient: Account<'info, TokenAccount>,
    #[account(mut, signer)]
    pub authority: AccountInfo<'info>,
    pub token_program: Program<'info, Token>,
    pub price_update: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[derive(Accounts)]
pub struct BurnGousd<'info> {
    #[account(mut)]
    pub state: Account<'info, StateAccount>,
    #[account(mut)]
    pub gousd_mint: Account<'info, Mint>,
    #[account(mut)]
    pub from_account: Account<'info, TokenAccount>,
    /// CHECK: Validated in instruction
    pub from: AccountInfo<'info>,
    pub authority: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct AdminOnly<'info> {
    #[account(mut)]
    pub state: Account<'info, StateAccount>,
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct FreezeAccount<'info> {
    #[account(mut)]
    pub state: Account<'info, StateAccount>,

    /// CHECK: The minted token
    #[account(mut)]
    pub gousd_mint: Account<'info, Mint>,

    /// CHECK: The freeze authority or PDA
    pub mint_authority: AccountInfo<'info>,

    #[account(mut)]
    pub token_to_freeze: Account<'info, TokenAccount>,

    pub authority: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct UnfreezeAccount<'info> {
    #[account(mut)]
    pub state: Account<'info, StateAccount>,

    /// CHECK:
    #[account(mut)]
    pub gousd_mint: Account<'info, Mint>,

    /// CHECK:
    pub mint_authority: AccountInfo<'info>,

    #[account(mut)]
    pub token_to_unfreeze: Account<'info, TokenAccount>,

    pub authority: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ValidateProofOfReserve<'info> {
    pub price_feed: AccountInfo<'info>,
}

#[account]
pub struct StateAccount {
    pub default_admin: Pubkey,
    pub default_admin_delay: i64,
    pub freezer: Pubkey,
    pub supply_controller: Pubkey,
    pub upgrader: Pubkey,
    pub rescuer: Pubkey,
    pub mint_authority_bump: u8,
    pub proof_of_reserve_feed: Pubkey,
    pub acceptable_proof_of_reserve_delay: i64,
    pub mint_cap_per_transaction: u64,
    pub total_supply: u64,
    pub paused: bool,
}

impl StateAccount {
    // Adjust if you changed fields
    pub const SIZE: usize = 32 + 8 + 32 + 32 + 32 + 32 + 1 + 32 + 8 + 8 + 8 + 1;
}

#[event]
pub struct MintEvent {
    pub to: Pubkey,
    pub amount: u64,
}

#[event]
pub struct BurnEvent {
    pub from: Pubkey,
    pub amount: u64,
}

#[event]
pub struct BlacklistEvent {
    pub account: Pubkey,
}

#[event]
pub struct UnblacklistEvent {
    pub account: Pubkey,
}
#[event]
pub struct FreezeEvent {
    pub account: Pubkey,
}

#[event]
pub struct UnfreezeEvent {
    pub account: Pubkey,
}

#[event]
pub struct MintCapEvent {
    pub new_cap: u64,
}

#[event]
pub struct ProofOfReserveDelayEvent {
    pub new_delay: i64,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Invalid address provided")]
    InvalidAddress,
    #[msg("Invalid time delay")]
    InvalidTimeDelay,
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("Contract is paused")]
    ContractPaused,
    #[msg("Unauthorized supply controller")]
    UnauthorizedSupplyController,
    #[msg("Unauthorized freezer")]
    UnauthorizedFreezer,
    #[msg("Unauthorized admin")]
    UnauthorizedAdmin,
    #[msg("Exceeds mint transaction cap")]
    ExceedsMintTransactionCap,
    #[msg("Supply exceeds reserves")]
    SupplyExceedsReserves,
    #[msg("Invalid proof of reserve data")]
    InvalidPoRData,
    #[msg("Proof of reserve data is outdated")]
    PoROutdated,
    #[msg("decimal value invalid")]
    InvalidTokenDecimal,
    #[msg("Arithmetic overflow occurred")]
    ArithmeticOverflow,
    #[msg("Arithmetic underflow occurred")]
    ArithmeticUnderflow,
}
