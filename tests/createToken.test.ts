import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { PublicKey, Keypair, SystemProgram } from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  getAssociatedTokenAddressSync,
  createAssociatedTokenAccountInstruction,
  createInitializeMintInstruction,
  MINT_SIZE,
} from "@solana/spl-token";

// Import your generated IDL/typings if needed
// import { GoUsd } from "../target/types/go_usd";

describe("Test", () => {
  it("createToken", async () => {
    const provider = anchor.AnchorProvider.env();
    anchor.setProvider(provider);

    const pg = anchor.workspace.GoUsd;

    const tokenDecimals = 6;
    const tokenName = "GoUSD";
    const tokenSymbol = "GUSD";
    const tokenUri = "https://example.com/token.json";

    // Generate your mint & metadata accounts
    const mint_account = Keypair.generate();
    console.log("mintAccount=", mint_account.publicKey.toBase58());

    const meta_data_account = Keypair.generate();
    console.log("metadataAccount=", meta_data_account.publicKey.toBase58());

    const TOKEN_METADATA_PROGRAM_ID = new anchor.web3.PublicKey(
      "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
    );

    const [metadataPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from("metadata"),
        TOKEN_METADATA_PROGRAM_ID.toBuffer(),
        mint_account.publicKey.toBuffer(),
      ],
      TOKEN_METADATA_PROGRAM_ID
    );

    const tx = await pg.methods
      .createToken(tokenDecimals, tokenName, tokenSymbol, tokenUri)
      .accounts({
        payer: provider.publicKey,
        mintAccount: mint_account.publicKey,
        metadataAccount: metadataPda,
        tokenProgram: TOKEN_PROGRAM_ID,
        tokenMetadataProgram: new PublicKey(
          "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s" // Metaplex metadata program
        ),
        systemProgram: SystemProgram.programId,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .signers([mint_account]) // needed because you're creating that account
      .rpc();

    console.log(
      `createToken() tx: https://explorer.solana.com/tx/${tx}?cluster=devnet`
    );
  });
});
