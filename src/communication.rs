use solana_sdk::{
    signature::{ Keypair, read_keypair_file, Signer, Signature },
    commitment_config::CommitmentConfig,
    pubkey::{ Pubkey },
    transaction::{ Transaction },
    hash::Hash
};
use solana_client::{
    rpc_client::RpcClient,
    rpc_config::{
        RpcSendTransactionConfig,
    },
    client_error::{ Result as ClientResult, ClientError, ClientErrorKind },
};
use solana_account_decoder::{
    parse_token::UiTokenAmount,
};
use spl_token::{
    solana_program::{
        instruction::{ Instruction },
    },
};

use crate::*;

// Structs
pub struct Communication {
    pub rpc_client: RpcClient,
    pub wallet:     Keypair,
}

impl Communication {
    pub fn init( cluster_url: &String, wallet_path: &String ) -> Self {
        let rpc = RpcClient::new_with_commitment(
            cluster_url.to_string( ), CommitmentConfig::confirmed( ) );
        let wallet = read_keypair_file( &*shellexpand::tilde( wallet_path ) )
            .expect( "Need keypair file to print money." );

        Self {
            rpc_client: rpc,
            wallet:     wallet
        }
    }

    pub fn get_blockhash( &self ) -> Hash {
        // let ( hash, _ ) = self.rpc_client.get_recent_blockhash_with_commitment(
        //    CommitmentConfig::finalized( ) )?.value;
        let ( hash, _ ) = self.rpc_client.get_latest_blockhash_with_commitment(
            CommitmentConfig::finalized( ) ).unwrap( );
        hash
    }

    pub fn send_transaction( &self,
                         instructions: &Vec<Instruction>,
                         signers: &Vec<&Keypair>,
                         simulate: bool,
                         recent_blockhash: Hash ) -> ClientResult<Signature> {
        // create transaction
        let tx = Transaction::new_signed_with_payer(
            instructions,
            Some( &self.wallet.pubkey( ) ), // payer
            signers,
            recent_blockhash
        );

        let trans_config = RpcSendTransactionConfig {
            skip_preflight: true,
            // skip_preflight: false, // desquid
            .. RpcSendTransactionConfig::default( )
        };

        if simulate {
            println!( "Simulating transaction." );
            let res = self.rpc_client.simulate_transaction( &tx )?;

            if let Some( logs ) = res.value.logs {
                for l in logs {
                    println!( "{}", l );
                }
            }

            if let Some( err ) = res.value.err {
                println!( "{:?}", err );
                return Err( ClientError::from( err ) );
            }

            return Err( ClientError{ kind: ClientErrorKind::Custom( "OK".to_string( ) ),
                request: None } );
        }

        let signature = self.rpc_client.send_transaction_with_config( &tx, trans_config )?;
        // let now = SystemTime::now( ).duration_since( UNIX_EPOCH ).unwrap( );
        // println!( "{:?}: TX sent, signature: {:?}", now, signature );
        eprintln!( "TX sent, signature: {:?}", signature );

        Ok( signature )
    }

    pub fn get_current_balance_for_currency( &self, currency: &Currency ) -> u64 {
        let (toys_in_ui, decs) =
            self.get_current_balance_for_pubkey_with_commitment(
                &currency.account,
                CommitmentConfig::confirmed( ) );

        ( toys_in_ui * POWERS_OF_TEN[ decs as usize ] ) as u64

        // return Self::get_current_balance_for_pubkey( &self.rpc_client, &self.wallet.pubkey( ) );
    }

    pub fn get_current_balance( &self, config: &Config, currencies: &Vec<Currency> ) -> u64 {
        self.get_current_balance_for_currency( &currencies[ config.start_currency ] )
    }

    pub fn get_current_balance_for_pubkey( &self, pubkey: &Pubkey ) -> u64 {
        match self.rpc_client.get_balance( pubkey ) {
            Err( err ) => {
                eprintln!( "{:?}", err );
                std::process::exit( 1 )
            },
            Ok( balance ) => {
                balance
            }
        }
    }

    /* Returns SPL token balance */
    pub fn get_current_balance_for_pubkey_with_commitment(
        &self, pubkey: &Pubkey, commitment: CommitmentConfig ) -> (f64, u8) {
        match self.rpc_client.get_token_account_balance_with_commitment( pubkey, commitment ) {
            Err( err ) => {
                eprintln!( "{:?}", err );
                std::process::exit( 1 )
            },
            Ok( val ) => {
                match val.value {
                    UiTokenAmount{
                        ui_amount: Some( ui_amnt ),
                        decimals: decs,
                        ..
                    } => {
                        (ui_amnt, decs)
                    },
                    _ => {
                        eprintln!( "Reading token balance failed" );
                        std::process::exit( 1 )
                    }
                }
            }
        }
    }

}


