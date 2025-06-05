use {
    anchor_lang::{
        prelude::*,
        system_program,
    },
    anchor_spl::{
        associated_token::{self, get_associated_token_address},
        token::{self, Token, Mint, TokenAccount, Transfer},
    },
    std::str::FromStr,
    mpl_token_metadata::accounts::Metadata,
};

// Public constant: wallet where platform fee will be sent.
pub const PLATFORM_FEE_WALLET: &str = "Platform_admin_wallet_address";

/// Handle NFT sale: validates metadata, calculates royalties and fees, and transfers SOL and NFT.
pub fn sell(ctx: Context<SellListedNft>) -> Result<()> {
    let listing = &mut ctx.accounts.nft_listing;

    // Ensure buyer has sufficient balance
    require!(
        ctx.accounts.buyer_authority.lamports() >= listing.price,
        MarketplaceError::InsufficientFunds
    );

    // Validate NFT format
    require!(ctx.accounts.mint.supply == 1, MarketplaceError::InvalidNFT);
    require!(ctx.accounts.mint.decimals == 0, MarketplaceError::InvalidNFT);

    // === Platform fee ===
    let platform_fee = listing
        .price
        .checked_mul(2).ok_or(MarketplaceError::NumericalOverflow)?
        .checked_div(100).ok_or(MarketplaceError::NumericalOverflow)?;

    let remaining = listing.price
        .checked_sub(platform_fee)
        .ok_or(MarketplaceError::NumericalOverflow)?;

    let platform_wallet_pubkey = Pubkey::from_str(PLATFORM_FEE_WALLET)
        .map_err(|_| error!(MarketplaceError::InvalidPlatformWallet))?;
    require_keys_eq!(
        ctx.accounts.platform_fee_wallet.key(),
        platform_wallet_pubkey,
        MarketplaceError::InvalidPlatformWallet
    );

    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.buyer_authority.to_account_info(),
                to: ctx.accounts.platform_fee_wallet.to_account_info(),
            },
        ),
        platform_fee,
    )?;

    // === Royalty handling ===
    let metadata = Metadata::safe_deserialize(&mut &**ctx.accounts.metadata_account.data.borrow())?;
    let creators = metadata.creators.as_ref().ok_or(MarketplaceError::NoVerifiedCreator)?;
    let verified_creator = creators.iter().find(|c| c.verified).ok_or(MarketplaceError::NoVerifiedCreator)?;
    require_keys_eq!(
        ctx.accounts.creator_account.key(),
        verified_creator.address,
        MarketplaceError::InvalidCreatorAccount
    );

    let royalty_bps = metadata.seller_fee_basis_points as u64;
    let royalty_amount = if royalty_bps > 0 {
        remaining.checked_mul(royalty_bps).unwrap() / 10_000
    } else {
        0
    };

    let seller_amount = remaining.checked_sub(royalty_amount).ok_or(MarketplaceError::NumericalOverflow)?;

    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.buyer_authority.to_account_info(),
                to: ctx.accounts.seller.to_account_info(),
            },
        ),
        seller_amount,
    )?;

    if royalty_amount > 0 {
        system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                system_program::Transfer {
                    from: ctx.accounts.buyer_authority.to_account_info(),
                    to: ctx.accounts.creator_account.to_account_info(),
                },
            ),
            royalty_amount,
        )?;
    }

    // === NFT Transfer ===
    let expected_token_account = get_associated_token_address(
        &ctx.accounts.buyer_authority.key(),
        &ctx.accounts.mint.key(),
    );
    require_keys_eq!(
        ctx.accounts.buyer_token_account.key(),
        expected_token_account,
        MarketplaceError::InvalidBuyerTokenAccount
    );

    require_keys_eq!(
        ctx.accounts.escrow_nft_account.owner,
        listing.key(),
        MarketplaceError::InvalidEscrowAccount
    );
    require!(
        ctx.accounts.escrow_nft_account.amount == 1
            && ctx.accounts.escrow_nft_account.mint == ctx.accounts.mint.key(),
        MarketplaceError::InvalidEscrowAccount
    );

    let signer_seeds: &[&[&[u8]]] = &[&[
        b"listing",
        ctx.accounts.mint.key().as_ref(),
        &[ctx.bumps.nft_listing],
    ]];

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            token::Transfer {
                from: ctx.accounts.escrow_nft_account.to_account_info(),
                to: ctx.accounts.buyer_token_account.to_account_info(),
                authority: listing.to_account_info(),
            },
            signer_seeds,
        ),
        1,
    )?;

    listing.seller = ctx.accounts.buyer_token_account.key();

    msg!("Sale completed: Platform Fee {}, Seller Receives {}", platform_fee, seller_amount);
    Ok(())
}

/// Create listing by transferring NFT to escrow
pub fn list_nft(ctx: Context<ListNft>, price: u64) -> Result<()> {
    let listing = &mut ctx.accounts.listing;

    listing.seller = ctx.accounts.seller.key();
    listing.nft_mint = ctx.accounts.nft_mint.key();
    listing.price = price;
    listing.bump = ctx.bumps.listing;

    let cpi_accounts = Transfer {
        from: ctx.accounts.seller_nft_account.to_account_info(),
        to: ctx.accounts.escrow_nft_account.to_account_info(),
        authority: ctx.accounts.seller.to_account_info(),
    };

    let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts);
    token::transfer(cpi_ctx, 1)?;
    msg!("ListedPrice: {}", listing.price);
    Ok(())
}

/// Delist the NFT and return it to seller
pub fn delist_nft(ctx: Context<DelistNft>) -> Result<()> {
    require!(ctx.accounts.listing.seller == ctx.accounts.seller.key(), CustomError::ListingNotActive);

    let seeds = &[
        b"listing",
        ctx.accounts.nft_mint.key().as_ref(),
        &[ctx.bumps.listing],
    ];
    let signer_seeds = &[&seeds[..]];

    let cpi_accounts = Transfer {
        from: ctx.accounts.escrow_token_account.to_account_info(),
        to: ctx.accounts.seller_token_account.to_account_info(),
        authority: ctx.accounts.listing.to_account_info(),
    };

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer_seeds,
        ),
        1,
    )?;
    msg!("NFTAddress:{}", ctx.accounts.nft_mint.key());
    Ok(())
}

#[account]
pub struct Listing {
    pub seller: Pubkey,
    pub nft_mint: Pubkey,
    pub price: u64,
    pub bump: u8,
}

#[error_code]
pub enum CustomError {
    #[msg("The listing is not active.")]
    ListingNotActive,
}

#[error_code]
pub enum MarketplaceError {
    #[msg("This listing is no longer active.")]
    ListingInactive,
    #[msg("You are not authorized to sell this NFT.")]
    Unauthorized,
    #[msg("Invalid platform wallet")]
    InvalidPlatformWallet,
    #[msg("Insufficient funds")]
    InsufficientFunds,
    #[msg("NFT is invalid")]
    InvalidNFT,
    #[msg("No verified creator found in metadata.")]
    NoVerifiedCreator,
    #[msg("Invalid creator account")]
    InvalidCreatorAccount,
    #[msg("Numerical overflow")]
    NumericalOverflow,
    #[msg("Invalid escrow account")]
    InvalidEscrowAccount,
    #[msg("Invalid buyer token account")]
    InvalidBuyerTokenAccount,
    #[msg("NFT already sold")]
    AlreadySold,
}