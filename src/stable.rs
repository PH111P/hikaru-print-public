use std::{
    sync::mpsc::channel,
    thread::sleep,
    time::Duration,
};
use solana_sdk::{
    signature::{ Signer },
    commitment_config::CommitmentConfig,
    signer::keypair::Keypair,
};
use solana_client::{
    rpc_config::{
        RpcAccountInfoConfig,
    },
    rpc_response::{
        Response as RpcResponse
    },
};
use solana_rpc::{
    rpc_pubsub::gen_client::Client as PubsubClient,
};
use solana_account_decoder::{
    UiAccount, UiAccountEncoding
};
use spl_token::{
    solana_program::{
        instruction::{ Instruction },
    },
};

use jsonrpc_core::futures::StreamExt;
use jsonrpc_client_transports::transports::ws;
use tokio::runtime::Runtime;

use crate::{
    communication::*,
    config::*,
    price::*,
};

// Structs

pub struct StablePrinter {
    pub money: u64,
    pub current_currency: usize,
    pub debug: bool,
    pub currencies: Vec<Currency>,
    pub pools: Vec<Pool>
}

// Implementations

impl StablePrinter {
    pub fn init( comm: &Communication, currencies: &Vec<Currency>, pools: &Vec<Pool>,
                 debug: bool ) -> Self {
        let mut res = StablePrinter {
            money:            0,
            current_currency: pools.len( ) + 1,
            debug:            debug,
            currencies:       currencies.clone( ),
            pools:            pools.clone( ),
        };
        res.recompute_balance( comm );
        res
    }

    pub fn recompute_balance( &mut self, comm: &Communication ) {
        let mut max_money = 0;
        let mut argmax = self.pools.len( ) + 1;
        let mut i = 0;
        for c in &self.currencies {
            let money = comm.get_current_balance_for_currency( c );
            if money > max_money {
                max_money = money;
                argmax = i;
            }
            i = i + 1;
        }

        self.current_currency = argmax;
        self.money = max_money;
    }

    pub fn run( &mut self, comm: &Communication, comm_send: &Communication,
                config: &Config, simulate: bool ) {
        // check if rpc is good
        comm_send.get_blockhash( );

        // initialize pool prizes
        let mut pool_prices = Vec::new( );
        for p in &self.pools {
            pool_prices.push( PoolPrice::init( comm, &p ) );
        }

        // set up subscriptions
        let ( account_sender, account_receiver )
            = channel::<RpcResponse<( usize, usize, UiAccount )>>( );

        let config_clone = config.clone( );
        let pools_clone = self.pools.clone( );

        // Create the pub sub runtime
        let rt = Runtime::new( ).unwrap( );
        rt.spawn( async move {
            let connect = ws::try_connect::<PubsubClient>( &config_clone.cluster_url ).unwrap( );
            let client = connect.await.unwrap( );

            // Subscribe to account notifications

            let mut idx = 0;
            for p in pools_clone {
                match p {
                    Pool::Swap( pool ) => {
                        for i in 0 ..= 1 {
                            let account_sender = account_sender.clone( );
                            let mut client_sub = client
                                .account_subscribe(
                                    pool.get_currency( i ).account.to_string( ),
                                    Some( RpcAccountInfoConfig {
                                        commitment: Some( CommitmentConfig::confirmed( ) ),
                                        encoding: Some( UiAccountEncoding::Base64Zstd ),
                                        ..RpcAccountInfoConfig::default( )
                                    } ),
                                    ).unwrap_or_else( |err| panic!( "acct sub err: {:#?}", err ) );
                            tokio::spawn( async move {
                                loop {
                                    match client_sub.next( ).await {
                                        Some( response_ab ) => {
                                            let response = response_ab.unwrap( );
                                            let n_response = solana_client::rpc_response::Response{
                                                context: response.context,
                                                value: ( idx, i, response.value )
                                            };
                                            account_sender.send( n_response ).unwrap( );
                                        }
                                        None => { }
                                    }
                                }
                            } );
                        }
                    },
                    Pool::Raydium( _ ) => { }
                }
                idx = idx + 1;
            }
        } );

        println!( "Initiating print sequence." );

        // TODO: add counter for scheduled abort to reset pool information to counteract skew
        loop {
            // Get all updates from the channel
            loop {
                match account_receiver.try_recv( ) {
                    Ok( solana_client::rpc_response::Response{ value: ( pool, tkn, result ), ..} ) => {
                        // update / recalculate costs
                        match self.pools[ pool ] {
                            Pool::Swap( _ ) => {
                                pool_prices[ pool ].token_price[ tkn ].update(
                                    &self.pools[ pool ].get_currency( tkn ), &result );

                                if pool_prices[ pool ].token_updated[ 1 - tkn ] {
                                    pool_prices[ pool ].token_updated[ tkn ] = false;
                                    pool_prices[ pool ].token_updated[ 1 - tkn ] = false;
                                    pool_prices[ pool ].sanity = true;
                                } else {
                                    pool_prices[ pool ].token_updated[ tkn ] = true;
                                    pool_prices[ pool ].sanity = false;
                                }
                            },
                            Pool::Raydium( _ ) => {
                                // TODO
                            }
                        }
                    },
                    Err( _err ) => {
                        // nothing new anymore
                        break;
                    }
                }
            }

            // for each pool
            // - check if currently applicable
            // - if so, compute/check if using pool yields
            // - if so, pick highest yielding pool and swap

            self.recompute_balance( comm );

            let gamble_money = self.get_gamble_money( config );
            let curr_a = &self.currencies[ self.current_currency ];
            let decs_a = curr_a.decimals as usize;

            let mut max_value = gamble_money;
            let mut max_value_n = gamble_money;
            let mut arg_max = self.pools.len( );
            let mut arg_max_dir = 2;

            if self.debug {
                println!( "Balance: {} {}",
                          ( gamble_money as f64 ) / POWERS_OF_TEN[ decs_a ],
                          self.currencies[ self.current_currency ].name );
            }

            for i in 0 .. self.pools.len( ) {
                for w in 0 ..= 1 {
                    if self.pools[ i ].get_currency( w ).currency_idx == self.current_currency {
                        // compute yield if this pool is used
                        let pool_price = pool_prices[ i ];
                        if !pool_price.sanity { continue; }

                        let curr_b = self.pools[ i ].get_currency( 1 - w );
                        let decs_b = self.currencies[ curr_b.currency_idx ].decimals as usize;

                        let ( toys_out, _ ) = pool_price.swap( gamble_money as u128,
                                                               w, &self.pools[ i ] );
                        let mut toys_out = ( toys_out as f64 * ( 1.0 - config.slippage ) ) as u128;
                        let toys_out_n = toys_out;

                        if decs_a != decs_b {
                            toys_out = ( ( toys_out as f64 ) / POWERS_OF_TEN[ decs_a ]
                                         * POWERS_OF_TEN[ decs_b ] ) as u128;
                        }

                        if toys_out_n > max_value_n as u128 {
                            max_value_n = toys_out_n as u64;
                            max_value = toys_out as u64;
                            arg_max = i;
                            arg_max_dir = w;
                        }

                        if self.debug {
                            println!( "{}: {} {}", i,
                                      ( toys_out as f64 ) / POWERS_OF_TEN[ decs_b ],
                                      self.currencies[ curr_b.currency_idx ].name );
                        }
                    }
                }
            }

            if arg_max < self.pools.len( )
                &&  max_value_n > ( ( gamble_money as f64 ) * config.minimum_gain_p ) as u64 {
                // enough profit, execute
                if self.debug {
                    let curr_b = self.pools[ arg_max ].get_currency( 1 - arg_max_dir );
                    println!( "Executing swap to {}.",
                              self.currencies[ curr_b.currency_idx ].name );
                }

                let extra_signer = Keypair::new( );
                let hash = comm_send.get_blockhash( );
                let mut instructions: Vec<Instruction> = Vec::new( );

                if !self.pools[ arg_max ].swap( &mut instructions, &comm_send.wallet.pubkey( ),
                                                &extra_signer.pubkey( ),
                                                gamble_money as u128,
                                                max_value as u128,
                                                arg_max_dir,
                                                config,
                                                &self.currencies ) {
                    if self.debug {
                        println!( "Creating tx failed." );
                    }
                    continue;
                }

                let signers = vec![ &comm.wallet ];

                match comm_send.send_transaction( &instructions, &signers, simulate, hash ) {
                    Ok( _ ) => {
                        if self.debug {
                            println!( "===== transaction completed =====" );
                        }
                    },
                    Err( err ) => {
                        if self.debug {
                            println!( "Error: {:?}", err );
                        }
                    }
                }

                // sleep some time to wait for tx
                sleep( Duration::from_millis( 1000 ) );
            }

            if self.debug {
                println!( "Waiting for updates.." );
            }

            match account_receiver.recv( ) {
                Ok( solana_client::rpc_response::Response{ value: ( pool, tkn, result ), ..} ) => {
                    // update / recalculate costs
                    match self.pools[ pool ] {
                        Pool::Swap( _ ) => {
                            pool_prices[ pool ].token_price[ tkn ].update(
                                &self.pools[ pool ].get_currency( tkn ), &result );

                            if pool_prices[ pool ].token_updated[ 1 - tkn ] {
                                pool_prices[ pool ].token_updated[ tkn ] = false;
                                pool_prices[ pool ].token_updated[ 1 - tkn ] = false;
                                pool_prices[ pool ].sanity = true;
                            } else {
                                pool_prices[ pool ].token_updated[ tkn ] = true;
                                pool_prices[ pool ].sanity = false;
                            }
                        },
                        Pool::Raydium( _ ) => {
                            // TODO
                        }
                    }
                },
                Err( err ) => {
                    println!( "Error: {:?}; reinit", err.to_string( ) );
                    std::process::exit( 1 )
                }
            }
        }
    }

    fn get_gamble_money( &self, config: &Config ) -> u64 {
        return ( self.money as f64 * config.safety_percentage ) as u64;
    }
}

