use anchor_lang::prelude::*;

// Modular design for separation of concerns
pub mod mint;    // Contains logic related to minting NFTs
pub mod listing; // Contains logic related to listing, buying, and delisting NFTs

use mint::*;
use listing::*;

// Unique program ID for deployment
declare_id!("38Ej9ddMjMPZAnzcWFNsLUWAXFeqKmX9bsSYpYAAy8us");

// Platform fee wallet where marketplace commissions (if any) are directed
pub const PLATFORM_FEE_WALLET: Pubkey = pubkey!("Platform_admin_wallet_address");
m
#[program]
pub mod nft_marketplace {
    use super::*;

    /// Mint a new NFT with custom metadata and royalty settings
    ///
    /// # Arguments
    /// * `ctx` - Context containing all required accounts for minting
    /// * `nft_name` - Name of the NFT
    /// * `nft_symbol` - Symbol for the NFT
    /// * `nft_uri` - URI pointing to the NFT metadata (usually hosted off-chain)
    /// * `royalty_basis_points` - Royalty in basis points (e.g., 200 = 2%)
    pub fn mint_nft(
        ctx: Context<CreateToken>,
        nft_name: String,
        nft_symbol: String,
        nft_uri: String,
        royalty_basis_points: u16,
    ) -> Result<()> {
        mint::mint_nft(
            ctx,
            nft_name,
            nft_symbol,
            nft_uri,
            royalty_basis_points,
        )
    }

    /// Buy a listed NFT
    ///
    /// Transfers ownership from seller to buyer and handles payment
    pub fn buy(
        ctx: Context<SellListedNft>
    ) -> Result<()> {
        listing::sell(ctx)
    }

    /// List an NFT for sale
    ///
    /// # Arguments
    /// * `ctx` - Context containing NFT and authority accounts
    /// * `price` - Price at which NFT will be listed (in lamports)
    pub fn list_nft(
        ctx: Context<ListNft>,
        price: u64
    ) -> Result<()> {
        listing::list_nft(ctx, price)
    }

    /// Delist a previously listed NFT
    ///
    /// Ensures only the seller can delist. Refunds rent and closes listing account.
    pub fn delist_nft(ctx: Context<DelistNft>) -> Result<()> {
        let listing = &ctx.accounts.listing;

        // Ensure that only the original seller can delist the NFT
        require_keys_eq!(
            listing.seller,
            ctx.accounts.seller.key(),
            CustomError::ListingNotActive
        );

        // Remove the NFT from the listing and handle any cleanup
        listing::delist_nft(ctx)?;

        // Listing account will be closed within `delist_nft` logic
        Ok(())
    }
}
