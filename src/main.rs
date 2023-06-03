use std::path::Path;
#[macro_use]
extern crate clap;

use crate::{
    config::*,
    printer::*,
    stable::*,
    communication::*,
};

pub mod raydium;
pub mod config;
pub mod printer;
pub mod stable;
pub mod price;
pub mod communication;

const VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");

fn main( ) {
    // cli param options and stuff

    // let wallet_path = "~/.config/solana/id.json";
    // let wallet_path = "~/.config/solana/main.json";

    // let cluster_url = "http://localhost:8899";
    // let cluster_url = "https://solana-api.projectserum.com";
    // let cluster_url = "https://api.mainnet-beta.solana.com";




    let matches = clap_app!( hikaru_print =>
        ( version: VERSION.unwrap_or( "unknown" ) )
        ( about: "Prints money using the solana blockchain." )
        ( @arg CONFIG_PATH: -c --config +required +takes_value "Sets the config file" )
        ( @arg CURRENCY_PATH: -y --currency_config +required +takes_value "Sets the currency config file" )
        ( @arg POOL_PATH: -p --pool_config +required +takes_value "Sets the pool config file" )
        ( @subcommand list =>
            ( about: "Lists contents of specified config files and corresponding cycles." )
            ( @arg POOL: -P --pool +takes_value "Pool name of a specific pool to list details about." )
        )
        ( @subcommand print =>
            ( about: "Prints money leveraging arbitrage cycles." )
            ( @arg sim: -s --simulate "Don't gamble, just simulate." )
            ( @arg deb: -d --debug "Print debug output to stdout." )
        )
        ( @subcommand stable =>
            ( about: "Prints money by swapping back and forth between different stable coins." )
            ( @arg sim: -s --simulate "Don't gamble, just simulate." )
            ( @arg deb: -d --debug "Print debug output to stdout." )
        )
        ( @subcommand execute =>
            ( about: "Forcibly execute a cycle by sending a corresponding tx (which should fail or yield profit)." )
            ( @arg CYCLE_IDX: +required "The index of the cycle to execute." )
            ( @arg sim: -s --simulate "Don't gamble, just simulate." )
            ( @arg deb: -d --debug "Print debug output to stdout." )
        )
    ).get_matches( );

    let config_path = Path::new( matches.value_of("CONFIG_PATH").unwrap( ) );
    print!( "Reading config from {}.", config_path.display( ) );
    let config = Config::read_from_file( config_path ).expect( "Config is garbage" );

    println!( "..OK" );

    let crcy_path = Path::new( matches.value_of("CURRENCY_PATH").unwrap( ) );
    print!( "Reading currencies from {}.", crcy_path.display( ) );
    let crcy_cfg = CurrencyConfig::read_from_file( crcy_path ).expect( "Currency config is garbage" );
    let currencies = crcy_cfg.currencies;

    let comm = Communication::init( &config.cluster_url, &crcy_cfg.wallet_path );
    let comm_send = if config.cluster_url != config.cluster_url_send {
        Some( Communication::init( &config.cluster_url_send, &crcy_cfg.wallet_path ) )
    } else {
        None
    };
    println!( "..OK" );


    let pool_path = Path::new( matches.value_of("POOL_PATH").unwrap( ) );
    print!( "Reading pools from {}.", pool_path.display( ) );
    let pools = PoolConfig::read_from_file( pool_path, &config ).expect( "Pool config is garbage" );
    println!( "..OK" );

    // don't need cycles for stable printer
    if let Some( scmd_list ) = matches.subcommand_matches( "stable" ) {
        // run the money printer
        if let Some( cs ) = comm_send {
            return StablePrinter::init( &comm, &currencies, &pools,
                                        scmd_list.is_present( "deb" )
                                        || scmd_list.is_present( "sim" ) ).
                run( &comm, &cs, &config, scmd_list.is_present( "sim" ) );
        } else {
            return StablePrinter::init( &comm, &currencies, &pools,
                                        scmd_list.is_present( "deb" )
                                        || scmd_list.is_present( "sim" ) ).
                run( &comm, &comm, &config, scmd_list.is_present( "sim" ) );
        }
    }

    print!( "Constructing cycles." );
    // construct graph out of currencies and pools; compute cycles found
    let cycles = construct_cycles( &config, &pools );
    println!( "..OK, {} cycles constructed.", cycles.len( ) );

    // do what we were instructed to do
    if let Some( scmd_list ) = matches.subcommand_matches( "list" ) {

        if let Some( pl ) = scmd_list.value_of( "POOL" ) {
            // load and parse a specific pool

            for p in &pools {
                if p.get_name( ) == pl.trim( ) {
//                    PoolPrice::dump( &comm, p );
                }
            }

            return;
        }

        println!( "Config:\n{:?}", config );
        // println!( "Currencies:\n{:?}", currencies );
        // println!( "Pools:\n{:?}", pools );

        Printer::init( &comm, &config, &currencies, &pools, &cycles, true ).list_path(
            &comm, &config );

        /*
        println!( "Cycles:" );
        let mut idx = 0;
        for c in &cycles {
            print!( "{}:", idx );
            print_cycle( c, &pools );
            println!( "" );

            idx = idx + 1;
        }
        */

        return;
    }

    if let Some( scmd_list ) = matches.subcommand_matches( "print" ) {
        // run the money printer
        if let Some( cs ) = comm_send {
            return Printer::init( &comm, &config, &currencies, &pools, &cycles,
                                  scmd_list.is_present( "deb" ) || scmd_list.is_present( "sim" ) ).
                run( &comm, &cs, &config, scmd_list.is_present( "sim" ) );
        } else {
            return Printer::init( &comm, &config, &currencies, &pools, &cycles,
                                  scmd_list.is_present( "deb" ) || scmd_list.is_present( "sim" ) ).
                run( &comm, &comm, &config, scmd_list.is_present( "sim" ) );
        }
    }

    if let Some( scmd_list ) = matches.subcommand_matches( "execute" ) {
        // run the money printer
        if let Some( cs ) = comm_send {
            return Printer::init( &comm, &config, &currencies, &pools, &cycles,
                                  scmd_list.is_present( "deb" ) || scmd_list.is_present( "sim" ) ).
                test_path( &comm, &cs, &config,
                           scmd_list.value_of( "CYCLE_IDX" ).unwrap( ).parse::<usize>( ).unwrap( ),
                           scmd_list.is_present( "sim" ) );
        } else {
            return Printer::init( &comm, &config, &currencies, &pools, &cycles,
                                  scmd_list.is_present( "deb" ) || scmd_list.is_present( "sim" ) ).
                test_path( &comm, &comm, &config,
                           scmd_list.value_of( "CYCLE_IDX" ).unwrap( ).parse::<usize>( ).unwrap( ),
                           scmd_list.is_present( "sim" ) );

        }
    }
}
