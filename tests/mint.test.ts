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
import { getAssociatedTokenAddress } from "@solana/spl-token";

// Import your generated IDL/typings if needed
// import { GoUsd } from "../target/types/go_usd";

describe("Test", () => {
  it("mintToken", async () => {
    const provider = anchor.AnchorProvider.env();
    anchor.setProvider(provider);

    const pg = anchor.workspace.GoUsd;

    // Generate your mint & metadata accounts
    const state = new PublicKey("BSvWY91xE3Gy4StCEHrhPifPkAc3EjnCarNwXa5RZxaG");
    const gousd_mint = new PublicKey(
      "31h8snQ7h4qErAJtxrJL3PfYSrnmQVDMZ1G82YySb5Jr"
    );
    const proof_of_reserve = new PublicKey(
      "HovQMDrbAgAYPCmHVSrezcSmkMtXSSUsLDFANExrZh2J"
    );

    const signer = provider.wallet;

    const tokenAccount = await getAssociatedTokenAddress(
      gousd_mint,
      signer.publicKey
    );

    const mintAmount = new BN(1_000_000);

    const tx = await pg.methods

      .mint(mintAmount)
      .accounts({
        state: state,
        gousdMint: gousd_mint,
        proofOfReserveFeed: proof_of_reserve,
        mintAuthority: signer.publicKey,
        recipient: tokenAccount,
        authority: signer.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        priceUpdate: proof_of_reserve,
        systemProgram: SystemProgram.programId,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      })
      .rpc();

    console.log("mint() tx:", tx);
  });
});
