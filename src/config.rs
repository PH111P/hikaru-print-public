use solana_sdk::{
    pubkey::Pubkey,
    instruction::{ AccountMeta, Instruction }
};
use spl_token_swap::{
    curve::{
        base::{ SwapCurve, CurveType as SCurveType, SwapResult },
        fees::Fees,
        stable::StableCurve,
        calculator::{ TradeDirection },
    },
};
use serde::{ Serialize, Deserialize };
use std::{
    str::FromStr,
    error::Error,
    fs::File,
    io::BufReader,
    path::Path,
};

use crate::*;

macro_rules! pkey {
    ($e:expr) => ( Pubkey::from_str( &$e ).unwrap( ) );
}

pub const POWERS_OF_TEN: [f64; 13] = [ 1.0, 10.0, 100.0, 1_000.0, 10_000.0, 100_000.0, 1_000_000.0,
    10_000_000.0, 100_000_000.0, 1_000_000_000.0, 10_000_000_000.0, 100_000_000_000.0,
    1_000_000_000_000.0 ];

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrencySD {
    name:     String,
    mint:     String,
    decimals: u8,
    account:  String,
}

#[derive(Debug, Clone)]
pub struct Currency {
    pub name:     String,
    pub mint:     Pubkey,
    pub decimals: u8,
    pub account:  Pubkey,
}


#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CurveType {
    Stable( u64 ),
    ConstantProduct( ),
}

fn default_orca_curve( ) -> String {
    "constant-product".to_string( )
}

pub const DEFAULT_ORCA_FEES: Fees = Fees {
    trade_fee_numerator:            1 * 251,
    trade_fee_denominator:          100000,
    owner_trade_fee_numerator:      1 * 5,
    owner_trade_fee_denominator:    10000,
    owner_withdraw_fee_numerator:   0,
    owner_withdraw_fee_denominator: 0,
    host_fee_numerator:             0,
    host_fee_denominator:           0,
};

pub const DEFAULT_SWAP_FEES: Fees = Fees {
    trade_fee_numerator:            1 * 271,
    trade_fee_denominator:          100000,
    owner_trade_fee_numerator:      1 * 5,
    owner_trade_fee_denominator:    10000,
    owner_withdraw_fee_numerator:   0,
    owner_withdraw_fee_denominator: 0,
    host_fee_numerator:             0,
    host_fee_denominator:           0,
};

pub const DEFAULT_ORCA_STABLE_FEES: Fees = Fees {
    trade_fee_numerator:            1 * 70,
    trade_fee_denominator:          100000,
    owner_trade_fee_numerator:      1 * 5,
    owner_trade_fee_denominator:    10000,
    owner_withdraw_fee_numerator:   0,
    owner_withdraw_fee_denominator: 0,
    host_fee_numerator:             0,
    host_fee_denominator:           0,
};

pub const DEFAULT_RAYDIUM_FEES: Fees = Fees {
    trade_fee_numerator:            1 * 221,
    trade_fee_denominator:          100000,
    owner_trade_fee_numerator:      1 * 3,
    owner_trade_fee_denominator:    10000,
    owner_withdraw_fee_numerator:   0,
    owner_withdraw_fee_denominator: 0,
    host_fee_numerator:             0,
    host_fee_denominator:           0,
};


#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenSD {
    currency_idx:       usize, // index in currency vector
    account:            String,
    extra_account:      Option<String>,
}
#[derive(Debug, Clone, Copy)]
pub struct Token {
    pub currency_idx:       usize, // index in currency vector
    pub account:            Pubkey, // account used in pool
    pub extra_account:      Option<Pubkey>, // account used by raydium for serum
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SwapPoolSD {
    name:               String,
    account:            String,
    authority:          String,
    pool_token_mint:    String,
    fee_account:        String,

    tokens:             [ TokenSD; 2 ],

    #[serde(default = "default_orca_curve")]
    curve:              String,
    #[serde(default)]
    curve_param:        u64,

    #[serde(default)]
    needs_approve:      bool,
    // #[serde(default = "default_orca_fees")]
    // fees:               Fees,
}

#[derive(Debug, Clone)]
pub struct SwapPool {
    swap_program:       Pubkey,
    swap_type:          String,

    name:               String,
    account:            Pubkey,
    authority:          Pubkey,
    pool_token_mint:    Pubkey,
    fee_account:        Pubkey,

    tokens:             [ Token; 2 ],

    needs_approve:      bool,
    is_step:            bool,

    curve:              CurveType,
    fees:               Fees,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RaydiumPoolSD {
    name:               String,
    pool_version:       u64,

    account:            String, // amm
    authority:          String, // ammAuthority
    open_orders:        String, // ammOpenOrders
    target_orders:      String,

    serum_version:      u64,

    serum_market:       String,
    serum_bids:         String,
    serum_asks:         String,
    serum_events:       String,
    serum_signer:       String,

    tokens:             [ TokenSD; 2 ],
}

#[derive(Debug, Clone)]
pub struct RaydiumPool {
    name:               String,
    pub pool_version:       u64,

    pub account:            Pubkey,
    pub authority:          Pubkey,
    pub open_orders:        Pubkey,
    pub target_orders:      Pubkey,

    pub serum_version:      u64,
    pub serum_market:       Pubkey,
    pub serum_bids:         Pubkey,
    pub serum_asks:         Pubkey,
    pub serum_events:       Pubkey,
    pub serum_signer:       Pubkey,

    tokens:             [ Token; 2 ],

    curve:              CurveType,
    fees:               Fees,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
enum PoolSD {
    Raydium( RaydiumPoolSD ),
    Orca( SwapPoolSD ),
    OrcaV2( SwapPoolSD ),
    Swap( SwapPoolSD ),
    Step( SwapPoolSD ),
}

#[derive(Debug, Clone)]
pub enum Pool {
    Raydium( RaydiumPool ),
    Swap( SwapPool ),
}


#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrencyConfigSD {
    wallet_path: String,
    currencies:  Vec<CurrencySD>,
}
#[derive(Debug, Clone)]
pub struct CurrencyConfig {
    pub wallet_path: String,
    pub currencies:  Vec<Currency>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PoolConfigSD {
    pools:      Vec<PoolSD>,
}
#[derive(Debug, Clone)]
pub struct PoolConfig {
    pools:      Vec<Pool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConfigSD {
    pub cluster_url:        String,
    pub cluster_url_send:   String,

    pub start_currency:     usize,
    pub safety_percentage:  f64,
    pub minimum_gain:       u128,
    #[serde(default)]
    pub minimum_gain_p:     f64,
    pub minimum_money:      u64,
    pub slippage:           f64,
    pub max_cycle_length:   u64,
    pub minimum_display:    f64,
    pub cooldown:           u64,

    #[serde(default)]
    pub greed:              f64,

    #[serde(default)]
    pub extra_budget:       u64,

    pub token_program:           String,
    pub associate_token_program: String,

    pub swap_program:         String,
    pub step_swap_program:    String,
    pub orca_swap_program:    String,
    pub orca_swap_program_v2: String,

    pub raydium_liquidity_program_v2: String,
    pub raydium_liquidity_program_v3: String,
    pub raydium_liquidity_program_v4: String,

    pub serum_program_v2:   String,
    pub serum_program_v3:   String,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub cluster_url:        String,
    pub cluster_url_send:   String,

    pub start_currency:     usize,
    pub safety_percentage:  f64,
    pub minimum_gain:       u128,
    pub minimum_gain_p:     f64,
    pub minimum_money:      u64,
    pub slippage:           f64,
    pub max_cycle_length:   u64,
    pub minimum_display:    f64,
    pub cooldown:           u64,

    pub greed:              f64,
    pub extra_budget:      u64,

    pub token_program:        Pubkey,
    pub swap_program:         Pubkey,
    pub step_swap_program:    Pubkey,
    pub orca_swap_program:    Pubkey,
    pub orca_swap_program_v2: Pubkey,

    pub associate_token_program: Pubkey,

    pub raydium_liquidity_program_v2: Pubkey,
    pub raydium_liquidity_program_v3: Pubkey,
    pub raydium_liquidity_program_v4: Pubkey,

    pub serum_program_v2:   Pubkey,
    pub serum_program_v3:   Pubkey,
}

#[derive(Debug, Clone)]
pub struct Cycle {
    pub needs_approval: bool,
    pub path:           Vec<(usize, usize)>, // List of ( pool indices, idx of input token)
}

// Implementations

pub fn print_cycle( cyc: &Cycle, pools: &Vec<Pool>, currencies: &Vec<Currency> ) {
    let mut i = 0;
    for ( p, idx ) in &cyc.path {
        if i == 0 {
            // print start currency
            print!( "{}", currencies[ pools[ *p ].get_currency( *idx ).currency_idx ].name );
        }

        // print pool type
        print!( " -{}- ", pools[ *p ].type_name( ) );

        // print next currency
        print!( "{}", currencies[ pools[ *p ].get_currency( 1 - *idx ).currency_idx ].name );

        i = i + 1;
    }
}

impl From<CurrencySD> for Currency {
    fn from( crcy: CurrencySD ) -> Self {
        Currency {
            name:     crcy.name,
            decimals: crcy.decimals,
            mint:     pkey!( crcy.mint ),
            account:  pkey!( crcy.account )
        }
    }
}

impl From<TokenSD> for Token {
    fn from( tkn: TokenSD ) -> Self {
        Token {
            currency_idx: tkn.currency_idx,
            account: pkey!( tkn.account ),
            extra_account: if let Some( acc ) = tkn.extra_account { Some( pkey!( acc ) ) } else { None }
        }
    }
}

impl From<RaydiumPoolSD> for RaydiumPool {
    fn from( pool: RaydiumPoolSD ) -> Self {
        RaydiumPool {
            name:           pool.name,
            pool_version:   pool.pool_version,

            account:        pkey!( pool.account ),
            authority:      pkey!( pool.authority ),
            open_orders:    pkey!( pool.open_orders ),
            target_orders:  pkey!( pool.target_orders ),

            serum_version:  pool.serum_version,
            serum_market:   pkey!( pool.serum_market ),
            serum_bids:     pkey!( pool.serum_bids ),
            serum_asks:     pkey!( pool.serum_asks ),
            serum_events:   pkey!( pool.serum_events ),
            serum_signer:   pkey!( pool.serum_signer ),

            tokens:         [ Token::from( pool.tokens[ 0 ].clone( ) ),
                              Token::from( pool.tokens[ 1 ].clone( ) ) ],

            curve:          CurveType::ConstantProduct( ),
            fees:           DEFAULT_RAYDIUM_FEES,

        }
    }
}

impl SwapPool {
    fn from( pool: SwapPoolSD, program: &Pubkey, tp: &str, is_step: bool ) -> Self {
        SwapPool {
            swap_program:    program.clone( ),
            swap_type:       tp.to_string( ),

            name:            pool.name,
            account:         pkey!( pool.account ),
            authority:       pkey!( pool.authority ),
            pool_token_mint: pkey!( pool.pool_token_mint ),
            fee_account:     pkey!( pool.fee_account ),

            tokens:          [ Token::from( pool.tokens[ 0 ].clone( ) ),
                               Token::from( pool.tokens[ 1 ].clone( ) ) ],

            needs_approve:   pool.needs_approve,
            is_step:         is_step,

            curve:           match pool.curve.as_str( ) {
                "stable" => { CurveType::Stable( pool.curve_param ) },
                _ => { CurveType::ConstantProduct( ) },
            },
            fees:            match pool.curve.as_str( ) {
                "stable" => { DEFAULT_ORCA_STABLE_FEES },
                _ => {
                    if tp == "orca" || tp == "orcaV2" || tp == "step" {
                        DEFAULT_ORCA_FEES
                    } else {
                        DEFAULT_SWAP_FEES
                    }
                },
            }
        }
    }
}

impl Pool {
    fn from( pool: PoolSD, config: &Config ) -> Self {
        match pool {
            PoolSD::Raydium( r ) => { Self::Raydium( RaydiumPool::from( r ) ) }
            PoolSD::Orca( o ) => { Self::Swap( SwapPool::from( o,
                                               &config.orca_swap_program, "orca", false ) ) }
            PoolSD::OrcaV2( o ) => { Self::Swap( SwapPool::from( o,
                                                 &config.orca_swap_program_v2, "orcaV2", false ) ) }
            PoolSD::Swap( o ) => { Self::Swap( SwapPool::from( o,
                                               &config.swap_program, "swap", false ) ) }
            PoolSD::Step( o ) => { Self::Swap( SwapPool::from( o,
                                               &config.step_swap_program, "step", true ) ) }
        }
    }
}

impl From<CurrencyConfigSD> for CurrencyConfig {
    fn from( cfg: CurrencyConfigSD ) -> Self {
        CurrencyConfig {
            wallet_path: cfg.wallet_path,
            currencies:  cfg.currencies.into_iter( ).map( Currency::from ).collect( )
        }
    }
}

impl PoolConfig {
    fn from( cfg: PoolConfigSD, config: &Config ) -> Self {
        PoolConfig {
            pools: cfg.pools.into_iter( ).map( |p: PoolSD| Pool::from( p, config ) ).collect( )
        }
    }
}

impl From<ConfigSD> for Config {
    fn from( con: ConfigSD ) -> Self {
        Config {
            cluster_url:        con.cluster_url,
            cluster_url_send:   con.cluster_url_send,

            start_currency:     con.start_currency,
            safety_percentage:  con.safety_percentage,
            minimum_gain:       con.minimum_gain,
            minimum_gain_p:     if con.minimum_gain_p < 1.0 { 1.0 } else { con.minimum_gain_p },
            minimum_money:      con.minimum_money,
            slippage:           con.slippage,
            max_cycle_length:   con.max_cycle_length,
            minimum_display:    con.minimum_display,
            cooldown:           con.cooldown,

            greed:              con.greed,
            extra_budget:       con.extra_budget,

            token_program:                pkey!( con.token_program ),
            swap_program:                 pkey!( con.swap_program ),
            orca_swap_program:            pkey!( con.orca_swap_program ),
            step_swap_program:            pkey!( con.step_swap_program ),
            associate_token_program:      pkey!( con.associate_token_program ),
            orca_swap_program_v2:         pkey!( con.orca_swap_program_v2 ),
            raydium_liquidity_program_v2: pkey!( con.raydium_liquidity_program_v2 ),
            raydium_liquidity_program_v3: pkey!( con.raydium_liquidity_program_v3 ),
            raydium_liquidity_program_v4: pkey!( con.raydium_liquidity_program_v4 ),
            serum_program_v2:             pkey!( con.serum_program_v2 ),
            serum_program_v3:             pkey!( con.serum_program_v3 ),
        }
    }
}

impl CurveType {
    fn get_curve( &self ) -> SwapCurve {
        match self {
            Self::Stable( amp ) => {
                SwapCurve {
                    curve_type:     SCurveType::Stable,
                    calculator:     Box::new( StableCurve{
                        amp: *amp
                    } )
                }
            },
            Self::ConstantProduct( ) => {
                SwapCurve::default( )
            }
        }
    }
}

impl CurrencyConfig {
    pub fn read_from_file<P: AsRef<Path>>( path: P ) -> Result<CurrencyConfig, Box<dyn Error>> {
        let file = File::open( path )?;
        let reader = BufReader::new( file );
        let c: CurrencyConfigSD = serde_json::from_reader( reader )?;
        Ok( Self::from( c ) )
    }
}

impl PoolConfig {
    pub fn read_from_file<P: AsRef<Path>>( path: P,
                                           config: &Config ) -> Result<Vec<Pool>, Box<dyn Error>> {
        let file = File::open( path )?;
        let reader = BufReader::new( file );
        let c: PoolConfigSD = serde_json::from_reader( reader )?;
        Ok( Self::from( c, config ).pools )
    }
}

impl Config {
    pub fn read_from_file<P: AsRef<Path>>( path: P ) -> Result<Self, Box<dyn Error>> {
        let file = File::open( path )?;
        let reader = BufReader::new( file );
        let c: ConfigSD = serde_json::from_reader( reader )?;
        Ok( Self::from( c ) )
    }
}

impl SwapPool {
    pub fn get_currency( &self, index: usize ) -> Token {
        return self.tokens[ index ];
    }
}

impl RaydiumPool {
    pub fn get_currency( &self, index: usize ) -> Token {
        return self.tokens[ index ];
    }
}

impl Pool {
    fn approximate_fees( fees: &Fees ) -> f64 {
        ( fees.trade_fee_numerator as f64 ) / ( fees.trade_fee_denominator as f64 )
            + ( fees.owner_trade_fee_numerator as f64 ) / ( fees.owner_trade_fee_denominator as f64 )
    }

    pub fn fees( &self ) -> f64 {
        match self {
            Self::Swap( SwapPool{ fees: f, .. } )
            | Self::Raydium( RaydiumPool{ fees: f, .. } ) => {
                1.0 - Self::approximate_fees( f )
            }
        }
    }

    pub fn get_currency( &self, index: usize ) -> Token {
        match self {
            Self::Swap( SwapPool{ tokens: t, .. } )
            | Self::Raydium( RaydiumPool{ tokens: t, .. } ) => {
                return t[ index ];
            }
        }
    }

    pub fn type_name( &self ) -> &str {
        match self {
            Self::Swap( SwapPool{ swap_type: t, .. } ) => { t }
            Self::Raydium( _ ) => { "RayV4" }
        }
    }

    pub fn get_name( &self ) -> &String {
        match self {
            Self::Swap( SwapPool{ name: n, .. } )
            | Self::Raydium( RaydiumPool{ name: n, .. } ) => {
                return n;
            }
        }
    }

    pub fn needs_approval( &self ) -> bool {
        match self {
            Self::Swap( SwapPool{ needs_approve: appr, .. } ) => { *appr },
            _ => { false }
        }
    }

    pub fn predict_swap( &self, toys_in: u128, swap_source_amount: u128,
                         swap_destination_amount: u128 ) -> ( u128, u128 ) {
         match self {
            Self::Swap( SwapPool{ curve: c, fees: f, .. } )
            | Self::Raydium( RaydiumPool{ curve: c, fees: f, .. } ) => {
                match c.get_curve( ).swap( toys_in, swap_source_amount,
                    swap_destination_amount, TradeDirection::AtoB /*unused*/, &f ) {
                    Some( SwapResult {
                        source_amount_swapped: source_amount,
                        destination_amount_swapped: amount_swapped,
                        ..
                    } ) => {
                        ( amount_swapped, source_amount )
                    },
                    _ => {
                        ( 0, 0 )
                    }
                }
            }
        }
    }

    pub fn swap( &self, instructions: &mut Vec<Instruction>,
                 payer: &Pubkey, extra_payer: &Pubkey,
                 toys_in: u128, toys_out: u128,
                 direction: usize, config: &Config, currencies: &Vec<Currency> ) -> bool {
        let tkn_a = self.get_currency( direction );
        let tkn_b = self.get_currency( 1 - direction );

        match self {
            Self::Swap( SwapPool{ authority: auth, account: acc, pool_token_mint: pmt,
                fee_account: fees, swap_program: program, needs_approve: appr, is_step, .. } ) => {
                if *appr {
                    // create an approve instruction

                    instructions.push(
                        spl_token::instruction::approve(
                            &config.token_program,
                            &currencies[ tkn_a.currency_idx ].account,
                            extra_payer,
                            payer,
                            &[],
                            toys_in as u64
                    ).unwrap( ) );
                }

                if *is_step {
                    let ins =
                        spl_token_swap::instruction::swap(
                            &program,
                            &config.token_program,
                            &acc,
                            &auth,
                            if *appr { extra_payer } else { payer },
                            // &config.wallet.pubkey( ),
                            &currencies[ tkn_a.currency_idx ].account,
                            &tkn_a.account,
                            &tkn_b.account,
                            &currencies[ tkn_b.currency_idx ].account,
                            &pmt,
                            &fees,
                            None,
                            spl_token_swap::instruction::Swap{
                                amount_in: toys_in as u64,
                                minimum_amount_out: toys_out as u64,
                            }
                        ).unwrap( );
                    let mut accs = ins.accounts;
                    let tmp = accs.pop( ).unwrap( );
                    accs.push( AccountMeta::new( *payer, false ) );
                    accs.push( tmp );

                    instructions.push(
                        Instruction{
                            program_id: ins.program_id,
                            accounts: accs,
                            data: ins.data
                        }
                    );
                } else {
                    instructions.push(
                        spl_token_swap::instruction::swap(
                            &program,
                            &config.token_program,
                            &acc,
                            &auth,
                            if *appr { extra_payer } else { payer },
                            // &config.wallet.pubkey( ),
                            &currencies[ tkn_a.currency_idx ].account,
                            &tkn_a.account,
                            &tkn_b.account,
                            &currencies[ tkn_b.currency_idx ].account,
                            &pmt,
                            &fees,
                            None,
                            spl_token_swap::instruction::Swap{
                                amount_in: toys_in as u64,
                                minimum_amount_out: toys_out as u64,
                            }
                            ).unwrap( ) );
                }
                true
            }
            Self::Raydium( RaydiumPool{
                pool_version: ray_v, account: amm_id, authority: amm_authority,
                open_orders: amm_open_orders, target_orders: amm_target_orders,
                serum_version: ser_v, serum_market: s_market, serum_bids: s_bids,
                serum_asks: s_asks, serum_events: s_events, serum_signer: s_signer, ..
            } ) => {
                match raydium::swap_base_in(
                    if *ray_v == 4 {
                        &config.raydium_liquidity_program_v4
                    } else if *ray_v == 3 {
                        &config.raydium_liquidity_program_v3
                    } else {
                        &config.raydium_liquidity_program_v2
                    },
                    &amm_id,
                    &amm_authority,
                    &amm_open_orders,
                    &amm_target_orders,
                    &tkn_a.account,
                    &tkn_b.account,
                    if *ser_v == 3 {
                        &config.serum_program_v3
                    } else {
                        &config.serum_program_v2
                    },
                    &s_market,
                    &s_bids,
                    &s_asks,
                    &s_events,
                    &if let Some( exa ) = tkn_a.extra_account {
                        exa
                    } else {
                        return false;
                    },
                    &if let Some( exb ) = tkn_b.extra_account {
                        exb
                    } else {
                        return false;
                    },
                    &s_signer,
                    &currencies[ tkn_a.currency_idx ].account,
                    &currencies[ tkn_b.currency_idx ].account,
                    payer, // TODO: is this correct?

                        toys_in as u64,
                        toys_out as u64
                ) {
                    Ok( ins ) => { instructions.push( ins ); true },
                    Err( err ) => {
                        println!( "creating instruction failed {:?}", err );
                        std::process::exit( 1 )
                    }
                }
            }
        }
    }
}


pub fn construct_cycles( config: &Config, pools: &Vec<Pool> ) -> Vec<Cycle> {
    let start = config.start_currency;
    let mut results: Vec<Cycle> = Vec::new( );

    let mut tmp: Vec<Cycle> = Vec::new( );
    for p in 0 .. pools.len( ) {
        for w in 0 ..= 1 {
            if pools[ p ].get_currency( w ).currency_idx == start {
                let mut cpy = Vec::new( );
                cpy.push(( p, w ));
                tmp.push( Cycle{ needs_approval: pools[ p ].needs_approval( ), path: cpy } );
            }
        }
    }

    for _i in 1 .. config.max_cycle_length {
        let mut tmp2: Vec<Cycle> = Vec::new( );

        for Cycle{ path: c, needs_approval: n } in &tmp {
            let ( lst_pool, lst_in_tkn_idx ) = c.last( ).unwrap( );
            let lst_out_tkn = pools[ *lst_pool ].get_currency( 1 - lst_in_tkn_idx );

            'pools: for p in 0 .. pools.len( ) {
                // avoid using a pool twice
                for ( cc, _ ) in c { if *cc == p { continue 'pools; } }

                for w in 0 ..= 1 {
                    if pools[ p ].get_currency( w ).currency_idx == lst_out_tkn.currency_idx {
                        let nn = *n || pools[ p ].needs_approval( );
                        let mut cpy = c.clone( );
                        cpy.push(( p, w ));
                        if pools[ p ].get_currency( 1 - w ).currency_idx == config.start_currency {
                            results.push( Cycle{ path: cpy.clone( ), needs_approval: nn } );
                            continue;
                        }
                        tmp2.push( Cycle{ path: cpy, needs_approval: nn } );
                    }
                }
            }
        }

        tmp = tmp2;
    }

    return results;
}
