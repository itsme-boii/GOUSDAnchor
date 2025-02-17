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

describe("Test", () => {
  it("initialize", async () => {
    const provider = anchor.AnchorProvider.env();
    anchor.setProvider(provider);
    const signer = provider.wallet;

    // Send transaction
    const defaultAdmin = Keypair.generate();
    console.log("default admin ", defaultAdmin.publicKey.toBase58());
    const freezer = Keypair.generate();
    console.log("freezer is  ", freezer.publicKey.toBase58());
    const supplyController = signer.publicKey;
    console.log("supply controller is ", supplyController.toBase58());
    const upgrader = Keypair.generate();
    console.log("upgrader is ", upgrader.publicKey.toBase58());
    const rescuer = Keypair.generate();
    console.log("rescuer is ", rescuer.publicKey.toBase58());
    const state = Keypair.generate();
    console.log("state is ", state.publicKey.toBase58());

    const proofofReserve = new PublicKey(
      "HovQMDrbAgAYPCmHVSrezcSmkMtXSSUsLDFANExrZh2J"
    );

    const defaultAdminDelay = new BN(60);
    const tx = await pg.program.methods
      .initialize(defaultAdminDelay)
      .accounts({
        state: state.publicKey,
        payer: provider.publicKey,
        defaultAdmin: defaultAdmin.publicKey,
        freezer: freezer.publicKey,
        supplyController: supplyController,
        upgrader: upgrader.publicKey,
        rescuer: rescuer.publicKey,
        proofOfReserveFeed: proofofReserve,
        systemProgram: SystemProgram.programId,
      })
      .signers([state, defaultAdmin, freezer, upgrader, rescuer])
      .rpc();

    console.log(
      `mint nft tx: https://explorer.solana.com/tx/${tx}?cluster=devnet`
    );
  });
});
