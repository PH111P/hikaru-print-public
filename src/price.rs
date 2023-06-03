use std::{
    cmp::max,
};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    account::Account as SdkAccount,
};
use solana_account_decoder::{
    UiAccount
};
use spl_token::{
    solana_program::{
        program_pack::Pack,
    },
    state::Account,
};

use crate::{
    communication::*,
    config::*,
};

// Structs

#[derive(Debug, Copy, Clone)]
pub struct TokenPrice {
    pub token_amount:       (f64, u8),
//    last_update:    Instant,
}

#[derive(Debug, Copy, Clone)]
pub struct PoolPrice {
    pub sanity:        bool,
    pub token_price:   [ TokenPrice; 2 ],
    pub token_updated: [ bool; 2 ],
}

// Implementations

impl PoolPrice {
    pub fn init( comm: &Communication, pool: &Pool ) -> Self {
        PoolPrice{
            sanity: true,
            token_price: [
                TokenPrice::init( &pool.get_currency( 0 ), comm ),
                TokenPrice::init( &pool.get_currency( 1 ), comm )
            ],
            token_updated: [ false, false ],
        }
    }

    pub fn dump( _comm: &Communication, pool: &Pool ) {
        // print token prices
        // TODO

        println!( "Dumping pool {}", pool.get_name( ) );

        // let price = Self::init( comm, pool );

    }

    pub fn swap( &self, toys_in: u128, direction: usize, pool_info: &Pool ) -> ( u128, u128 ) {
        let ( a_val, a_decs ) = self.token_price[ direction ].token_amount;
        let ( b_val, b_decs ) = self.token_price[ 1 - direction ].token_amount;

        let decs = max( a_decs, b_decs );
        let a_val = ( a_val * POWERS_OF_TEN[ decs as usize ] ) as u128;
        let b_val = ( b_val * POWERS_OF_TEN[ decs as usize ] ) as u128;

        return pool_info.predict_swap( toys_in as u128, a_val, b_val );
    }

    pub fn token_amount( &self, direction: usize ) -> f64 {
        let ( val, decs ) = self.token_price[ direction ].token_amount;

        val * POWERS_OF_TEN[ decs as usize ]
    }
}

impl TokenPrice {
    pub fn init( token: &Token, comm: &Communication ) -> Self {
        TokenPrice {
            token_amount: comm.get_current_balance_for_pubkey_with_commitment(
                              &token.account,
                              CommitmentConfig::finalized( ) )
        }
//        self.last_update = Instant::now( );
    }

    pub fn update( &mut self, token: &Token, account_data: &UiAccount ) {
        match token.currency_idx {
            /* SOL_IDX => {
                // just use the provided lamports value.
                let ( _old_amt, decs ) = self.token_amount;
                self.token_amount = ( account_data.lamports as f64 / POWERS_OF_TEN[ decs as usize],
                                      decs );
            }, */
            _ => {
                match account_data.decode::<SdkAccount>( ) {
                    Some( sdk_acc ) => {
                        let ( _old_amt, decs ) = self.token_amount;
                        // here we need to parse the account data.
                        let account = Account::unpack_unchecked( &sdk_acc.data ).unwrap( );
                        self.token_amount = ( account.amount as f64 / POWERS_OF_TEN[ decs as usize ], decs );
                    },
                    None => {
                        println!( "Malformed uiaccount {:?}", account_data );
                    }
                }
            },
        }
    }
}
