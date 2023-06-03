use std::{
    sync::mpsc::channel,
    time::{ SystemTime, UNIX_EPOCH },
};
use solana_sdk::{
    signature::{ Signer, Signature },
    commitment_config::CommitmentConfig,
    signer::keypair::Keypair,
    hash::Hash,
    compute_budget::ComputeBudgetInstruction,
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
use bit_vec::BitVec;

use crate::{
    communication::*,
    config::*,
    price::*,
};

// Structs

pub struct Printer {
    pub money: u64,
    pub debug: bool,
    pub currencies: Vec<Currency>,
    pub pools: Vec<Pool>,
    pub cycles: Vec<Cycle>
}

// Implementations

impl Printer {
    pub fn init( comm: &Communication, config: &Config,
                 currencies: &Vec<Currency>, pools: &Vec<Pool>, cycles: &Vec<Cycle>,
                 debug: bool ) -> Self {
        let money = comm.get_current_balance( config, currencies );
        Printer {
            money:      money,
            debug:      debug,
            currencies: currencies.clone( ),
            pools:      pools.clone( ),
            cycles:     cycles.clone( )
        }
    }

    pub fn test_path( &self, comm: &Communication, comm_send: &Communication, config: &Config,
                      cycle_idx: usize, simulate: bool ) {
        let cycle = &self.cycles[ cycle_idx ];

        // initialize pool prizes
        let mut pool_prices = Vec::new( );
        for p in &self.pools {
            pool_prices.push( PoolPrice::init( comm, &p ) );
        }

        let gamble_money = self.get_best_gamble_money( config, cycle, &pool_prices );

        if gamble_money < config.minimum_money {
            eprintln!( "Insufficient balance, aborting." );
            std::process::exit( 1 );
        }


        let toys_out = self.compute_potential( config, cycle, &pool_prices, gamble_money );

        if self.debug {
            println!( "Executing path for {} toys:", gamble_money );
            print_cycle( &cycle, &self.pools, &self.currencies );
            println!( " yields {}.", toys_out );
        }

        // execute path

        let hash = comm_send.get_blockhash( );
        self.execute_path( comm_send, &cycle, gamble_money, config, &pool_prices, simulate, hash );
    }

    pub fn list_path( &self, comm: &Communication, config: &Config ) {
        // initialize pool prizes
        let mut pool_prices = Vec::new( );
        for p in &self.pools {
            pool_prices.push( PoolPrice::init( comm, &p ) );
        }

        let mut gamble_money = self.get_gamble_money( config );

        println!( "Printing paths and estimated gains." );

        if gamble_money < config.minimum_money {
            println!( "WARNING: Insufficient balance." );
            gamble_money = config.minimum_money;
        }

        println!( "Testing for {} toys:", gamble_money );
        let mut idx = 0;

        for cycle in &self.cycles {
            print!( "{}:", idx );


            let opt_gamble_money = self.get_best_gamble_money( config, cycle, &pool_prices );

            let toys_out = self.compute_potential( config, cycle, &pool_prices, opt_gamble_money );

            print_cycle( cycle, &self.pools, &self.currencies );
            print!( " yields {}.", toys_out );

            println!( " (Opt gamble: {})", opt_gamble_money );

            idx = idx + 1;
        }
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
                    Pool::Raydium( pool ) => {
                        // subscribe to pool token account changes
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
                        // ammOpenOrders
                        {
                            let account_sender = account_sender.clone( );
                            let mut client_sub = client
                                .account_subscribe(
                                    pool.open_orders.to_string( ),
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
                                                value: ( idx, 3, response.value )
                                            };
                                            account_sender.send( n_response ).unwrap( );
                                        }
                                        None => { }
                                    }
                                }
                            } );
                        }
                        // serum market
                        {
                            let account_sender = account_sender.clone( );
                            let mut client_sub = client
                                .account_subscribe(
                                    pool.serum_market.to_string( ),
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
                                                value: ( idx, 4, response.value )
                                            };
                                            account_sender.send( n_response ).unwrap( );
                                        }
                                        None => { }
                                    }
                                }
                            } );
                        }
                    }
                }
                idx = idx + 1;
            }
        } );

        let mut ath = -( self.get_gamble_money( config ) as i128 );
        let mut ath_cyc = 0;
        let mut ath_date = SystemTime::now( ).duration_since( UNIX_EPOCH ).unwrap( );

        let mut cycle_cooldown = vec![ config.cooldown; self.cycles.len( ) ];
        let mut cycle_gain = vec![ 0; self.cycles.len( ) ];
        let mut cycle_money = vec![ 0; self.cycles.len( ) ];
        let mut cycle_needs_update = BitVec::from_elem( self.cycles.len( ), true );

        let mut pool_update = vec![ BitVec::from_elem( self.cycles.len( ), false );
                                    self.pools.len( ) ];
        for i in 0 .. self.cycles.len( ) {
            for ( idx, _ ) in &self.cycles[ i ].path {
                pool_update[ *idx ].set( i, true );
            }
        }

        println!( "Initiating print sequence." );

        // TODO: add counter for scheduled abort to reset pool information to counteract skew
        loop {
            // Get all updates from the channel
            loop {
                match account_receiver.try_recv( ) {
                    Ok( solana_client::rpc_response::Response{ value: ( pool, tkn, result ), ..} ) => {
                        cycle_needs_update.or( &pool_update[ pool ] );
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


            let mut cng = false;
            for i in 0 .. self.cycles.len( ) {
                if !cycle_needs_update[ i ]
                    && cycle_cooldown[ i ] == 0 { continue; }
                else if !cycle_needs_update[ i ] {
                    let opt_gamble_money = cycle_money[ i ];
                    let rs = cycle_gain[ i ];

                    if opt_gamble_money >= config.minimum_money
                        &&  rs > opt_gamble_money + config.minimum_gain as u64 {
                            cng = true;
                    }
                } else {
                    cng = true;
                }
            }

            if cng {
                let hash = comm_send.get_blockhash( );
                for i in 0 .. self.cycles.len( ) {
                    if !cycle_needs_update[ i ]
                        && cycle_cooldown[ i ] == 0 { continue; }
                    else if !cycle_needs_update[ i ] {
                        cycle_cooldown[ i ] = cycle_cooldown[ i ] - 1;
                        let opt_gamble_money = cycle_money[ i ];
                        let rs = cycle_gain[ i ];

                        if opt_gamble_money >= config.minimum_money
                            &&  rs > opt_gamble_money + config.minimum_gain as u64 {
                                // ensure that a cycle is executed only a limited number of times to avoid
                                // losses due to too many failed transactions.
                                self.execute_path( comm_send, &self.cycles[ i ],
                                                   opt_gamble_money as u64,
                                                   config, &pool_prices, simulate, hash );
                            }
                    } else {
                        cycle_needs_update.set( i, false );
                        let opt_gamble_money =  self.get_best_gamble_money( config, &self.cycles[ i ],
                                                                            &pool_prices );
                        if opt_gamble_money == cycle_money[ i ] { continue; }
                        cycle_money[ i ] = opt_gamble_money;
                        if opt_gamble_money < config.minimum_money { continue; }
                        let rs = self.compute_potential( config, &self.cycles[ i ],
                                                         &pool_prices, opt_gamble_money );
                        if cycle_gain[ i ] == rs as u64 { continue; }
                        cycle_gain[ i ] = rs as u64;
                        cycle_cooldown[ i ] = config.cooldown;
                        if rs > opt_gamble_money as u128  + config.minimum_gain {
                            // ensure that a cycle is executed only a limited number of times to avoid
                            // losses due to too many failed transactions.
                            self.execute_path( comm_send, &self.cycles[ i ],
                                               opt_gamble_money as u64,
                                               config, &pool_prices, simulate, hash );
                        }
                    }
                }

                if  self.debug {
                    for i in 0 .. self.cycles.len( ) {
                        if cycle_gain[ i ] > ( cycle_money[ i ] as f64 / config.minimum_display ) as u64 {
                            print!( "{}:", i );
                            print_cycle( &self.cycles[ i ], &self.pools, &self.currencies );
                            println!( " yields {} ({}) for {}.  cooldown {}.", cycle_gain[ i ], ( cycle_gain[ i ] as i128 )
                                      - ( cycle_money[ i ] as i128 ), cycle_money[ i ], cycle_cooldown[ i ] );

                            if cycle_gain[ i ] as i128 - cycle_money[ i ] as i128 > ath {
                                ath = cycle_gain[ i ] as i128 - cycle_money[ i ] as i128;
                                ath_cyc = i;
                                ath_date = SystemTime::now( ).duration_since( UNIX_EPOCH ).unwrap( );
                            }
                        }
                    }
                    print!( "Highest yield observed so far: {} on cycle {} ", ath, ath_cyc );
                    print_cycle( &self.cycles[ ath_cyc ], &self.pools, &self.currencies );
                    println!( " at {:?}.", ath_date.as_secs( ) );
                }
            }

            match account_receiver.recv( ) {
                Ok( solana_client::rpc_response::Response{ value: ( pool, tkn, result ), ..} ) => {
                    // update / recalculate costs
                    cycle_needs_update.or( &pool_update[ pool ] );
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

    fn execute_path( &self, comm: &Communication, cycle: &Cycle, gamble_money: u64, config: &Config,
                     pool_prices: &Vec<PoolPrice>, simulate: bool,
                     hash: Hash ) -> Option<Signature> {
        if self.debug {
            print!( "Executing " );
            print_cycle( cycle, &self.pools, &self.currencies );
            println!( " on {} SOL toy money.", gamble_money as f64 / POWERS_OF_TEN[ 9 ] );
        }

        /*
         * Execute path by creating a single transaction with all steps.
         * Estimate money obtained in each step to use as amount in for next transaction
         */

        let mut toys_in = gamble_money as u128;
        let mut instructions: Vec<Instruction> = Vec::new( );
        let mut decs = 0;

        // compute budget
        if config.extra_budget > 0 {
            // cook up extra budget instruction
            instructions.push( ComputeBudgetInstruction::set_compute_unit_price( config.extra_budget ) );
        }

        // extra signer required for some marketplaces. Only used if required.
        let extra_signer = Keypair::new( );

        let path = &cycle.path;
        for i in 0 .. path.len( ) {
            let ( curr_pool, dir ) = path[ i ];

            let pool_price = pool_prices[ curr_pool ];
            let pool = &self.pools[ curr_pool ];

            let curr_a = &self.currencies[ pool.get_currency( dir ).currency_idx ];
            let curr_b = &self.currencies[ pool.get_currency( 1 - dir ).currency_idx ];

            let ndecs = curr_b.decimals as usize;
            if decs == 0 {
                decs = curr_a.decimals as usize;
            }

            let ( toys_out, traded ) = pool_price.swap( toys_in, dir, &self.pools[ curr_pool ] );
//            println!( "Before sl {}", toys_out );
            let toys_out = ( toys_out as f64 * ( 1.0 - config.slippage ) ) as u128;
//            println!( "After sl {}", toys_out );


            if traded > toys_in {
                println!( "Ran out of toys. The evil squids must have eaten them." );
                break;
            }

            /*
               println!( "Swapping {} ({}) {} for {} ({}) {}.",
               toys_in_ui, toys_in, config.currencies[ path.nodes[ i - 1 ] ].name,
               toys_out as f64 / POWERS_OF_TEN[ out_decs as usize ],
               toys_out, config.currencies[ path.nodes[ i ] ].name );
               */
            let mut nout = toys_out as u128;

            if decs != 0 && decs != ndecs {
                nout = ( ( nout as f64 ) / POWERS_OF_TEN[ decs ] * POWERS_OF_TEN[ ndecs ] )
                    as u128;
            }
            decs = ndecs;

            let out = if i + 1 < path.len( ) {
                0
            } else  {
                gamble_money as u128
            };

            if self.debug {
                println!( "step {:?}: in {:?} (traded {:?}) out {:?}", i, toys_in, traded, nout );
            }

            if !self.pools[ curr_pool ].swap( &mut instructions,
                                              &comm.wallet.pubkey( ),
                                              &extra_signer.pubkey( ),
                                              toys_in as u128,
                                              out, dir, config, &self.currencies ) {
                return None;
            }

            toys_in = nout;
        }
        /*
           if toys_in < gamble_money as u128 {
           println!( "This cycle kinda sucks, you know…" );
           false
           } else {*/
        // actually run the transaction
        let signers = if cycle.needs_approval {
            vec![ &comm.wallet, &extra_signer ]
        } else {
            vec![ &comm.wallet ]
        };
        match comm.send_transaction( &instructions, &signers, simulate, hash ) {
            Ok( signature ) => {
                if self.debug {
                    println!( "===== transaction completed =====" );
                }
                Some( signature )
            },
            Err( err ) => {
                if self.debug {
                    println!( "Error: {:?}", err );
                }
                None
            }
        }
        //        }
    }

    fn get_gamble_money( &self, config: &Config ) -> u64 {
        return ( self.money as f64 * config.safety_percentage ) as u64;
    }

    fn get_best_gamble_money( &self, config: &Config, cycle: &Cycle,
                              pool_prices: &Vec<PoolPrice> ) -> u64 {
        let max_gamble_money = self.get_gamble_money( config );
        let path = &cycle.path;

        // assumes constant product

        // TODO: use integer arithmetic

        let mut alpha = 1.0;
        let mut beta  = 1.0;
        let mut gamma = 0.0;

        for i in 0 .. path.len( ) {
            let ( pool, dir ) = path[ i ];
            let pp = &pool_prices[ pool ];
            let pi = &self.pools[ pool ];
            let a = pp.token_amount( dir ); // pool in
            let b = pp.token_amount( 1 - dir ); // pool out
            let mut f = pi.fees( ) * ( 1.0 - config.slippage ); // pool fees
            for _j in 0 .. i {
                f = f * ( 1.0 - config.slippage );
            }

            gamma = gamma * a + alpha * f;
            alpha = alpha * b * f;
            beta  = beta * a;

            // println!( "Values after pool {}: alpha {}, beta {}, gamma {}", i, alpha, beta, gamma );
            // println!( "Values after pool {}: a {}, b {}, f {}", i, a, b, f );
        }

        let gamble_money_f = ( ( alpha * beta ).sqrt( ) - beta ) / gamma;

        let gamble_money = ( gamble_money_f.floor( ) * config.greed ) as i64;

        // println!( "Gamble money = {} = {}", gamble_money_f, gamble_money );


        // println!( "Predicted yield for {} = {}", max_gamble_money,
        //          alpha * ( max_gamble_money as f64 )
        //          / ( beta + gamma * ( max_gamble_money as f64 ) ) );


        // print!( "Not using optimal value {} for cycle ", gamble_money );
        // print_cycle( path, pools, currencies );

        if gamble_money < config.minimum_money as i64
            || gamble_money as u64 > max_gamble_money {
                max_gamble_money
            } else {
                gamble_money as u64
            }
    }

    fn compute_potential( &self, config: &Config,
                          cycle: &Cycle, pool_prices: &Vec<PoolPrice>, gamble_money: u64 ) -> u128 {
        // directly comput how much toys this path will yield.

//        print!( "Computing potential for {} toys along ", gamble_money );
//        print_cycle( path, pools );

        let path = &cycle.path;

        let mut toys_in = gamble_money as u128;
        let mut decs = 0;
        for i in 0 .. path.len( ) {
            let ( curr_pool, dir ) = path[ i ];
            let pool_price = pool_prices[ curr_pool ];

            if !pool_price.sanity {
                // pool is not properly updated
                return 0;
            }

            let pool = &self.pools[ curr_pool ];

            let curr_a = &self.currencies[ pool.get_currency( dir ).currency_idx ];
            let curr_b = &self.currencies[ pool.get_currency( 1 - dir ).currency_idx ];

            let ndecs = curr_b.decimals as usize;
            if decs == 0 {
                decs = curr_a.decimals as usize;
            }

            let ( toys_out, _ ) = pool_price.swap( toys_in, dir, &self.pools[ curr_pool ] );
            let toys_out = ( toys_out as f64 * ( 1.0 - config.slippage ) ) as u128;

            toys_in = toys_out as u128;
            if decs != 0 && decs != ndecs {
                toys_in = ( ( toys_in as f64 ) / POWERS_OF_TEN[ decs ] * POWERS_OF_TEN[ ndecs ] )
                    as u128;
            }
            decs = ndecs;

            //            print!( "\n {} -> {}", toys_in, toys_out );

            // adjust for different decimals
        }
//        println!( "" );
        toys_in
    }

    /*
    fn best_path( &self, pool_prices: &Vec<PoolPrice>,
                  config: &Config ) -> Vec<( usize, u128, u64 )> {
        // returns (index, result) pairs for all cycles that are profitable
        let mut res = Vec::new( );

        for i in 0 .. self.cycles.len( ) {
            let opt_gamble_money = self.get_best_gamble_money( config, &self.cycles[ i ],
                                                               pool_prices );

            let rs = self.compute_potential( config, &self.cycles[ i ],
                                             pool_prices, opt_gamble_money );
            // Filter out most garbage cycles
            if rs > ( opt_gamble_money as f64 / config.minimum_display ) as u128 {
                res.push(( i, rs, opt_gamble_money ));
            }
            // TODO: add log msg here?
        }
        // TODO: sort paths according to yield?
        res
    }
    */
}

