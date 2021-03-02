#[macro_use]
extern crate log;

use docopt::Docopt;
use serde::Deserialize;
use std::{env, io};

use martin::config::{read_config, Config, ConfigBuilder};
use martin::db::{check_postgis_version, get_connection, setup_connection_pool, Pool};
use martin::function_source::get_function_sources;
use martin::server;
use martin::table_source::get_table_sources;
use martin::utils::prettify_error;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const REQUIRED_POSTGIS_VERSION: &str = ">= 2.4.0";

pub const USAGE: &str = "
Martin - PostGIS Mapbox Vector Tiles server.

Usage:
  martin [options] [<connection>]
  martin -h | --help
  martin -v | --version

Options:
  -h --help                         Show this screen.
  -v --version                      Show version.
  --config=<path>                   Path to config file.
  --keep-alive=<n>                  Connection keep alive timeout [default: 75].
  --listen-addresses=<n>            The socket address to bind [default: 0.0.0.0:3000].
  --pool-size=<n>                   Maximum connections pool size [default: 20].
  --watch                           Scan for new sources on sources list requests.
  --workers=<n>                     Number of web server workers.
  --danger-accept-invalid-certs     Trust invalid certificates. This introduces significant vulnerabilities, and should only be used as a last resort.
  --jwt                             Enable secure endpoints with JWT.
  --jwt-secret=<secret>             A secret to verify signature (need JWT enabled).
  --jwt-algorithm=<algorithm>       Algorithm can be: empty (autodetect from jwt token dangerous), HS256, HS384, HS512, ES256, ES384, RS256, RS384, RS512, PS256, PS384 or PS512.
  --jwt-check-exp-time              Enable check expiration time in claims, by default disabled.
";

#[derive(Debug, Deserialize)]
pub struct Args {
    pub arg_connection: Option<String>,
    pub flag_config: Option<String>,
    pub flag_help: bool,
    pub flag_keep_alive: Option<usize>,
    pub flag_listen_addresses: Option<String>,
    pub flag_pool_size: Option<u32>,
    pub flag_watch: bool,
    pub flag_version: bool,
    pub flag_workers: Option<usize>,
    pub flag_danger_accept_invalid_certs: bool,
    pub flag_jwt: bool,
    pub flag_jwt_secret: Option<String>,
    pub flag_jwt_algorithm: Option<String>,
    pub flag_jwt_check_exp_time: bool,
}

pub fn generate_config(args: Args, pool: &Pool) -> io::Result<Config> {
    let connection_string = args.arg_connection.clone().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::Other,
            "Database connection string is not set",
        )
    })?;

    let mut connection = get_connection(pool)?;
    let table_sources = get_table_sources(&mut connection)?;
    let function_sources = get_function_sources(&mut connection)?;

    let config = ConfigBuilder {
        connection_string,
        watch: Some(args.flag_watch),
        keep_alive: args.flag_keep_alive,
        listen_addresses: args.flag_listen_addresses,
        pool_size: args.flag_pool_size,
        worker_processes: args.flag_workers,
        table_sources: Some(table_sources),
        function_sources: Some(function_sources),
        danger_accept_invalid_certs: Some(args.flag_danger_accept_invalid_certs),
        jwt: Some(args.flag_jwt),
        jwt_secret: args.flag_jwt_secret,
        jwt_algorithm: args.flag_jwt_algorithm,
        jwt_check_exp_time: Some(args.flag_jwt_check_exp_time),
    };

    let config = config.finalize();
    Ok(config)
}

fn setup_from_config(file_name: String) -> io::Result<(Config, Pool)> {
    let config = read_config(&file_name).map_err(prettify_error("Can't read config"))?;

    let pool = setup_connection_pool(
        &config.connection_string,
        Some(config.pool_size),
        config.danger_accept_invalid_certs,
    )
    .map_err(prettify_error("Can't setup connection pool"))?;

    info!("Connected to {}", config.connection_string);

    Ok((config, pool))
}

fn setup_from_args(args: Args) -> io::Result<(Config, Pool)> {
    let connection_string = args.arg_connection.clone().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::Other,
            "Database connection string is not set",
        )
    })?;

    info!("Connecting to database");
    let pool = setup_connection_pool(
        &connection_string,
        args.flag_pool_size,
        args.flag_danger_accept_invalid_certs,
    )
    .map_err(prettify_error("Can't setup connection pool"))?;

    info!("Scanning database");
    let config = generate_config(args, &pool).map_err(prettify_error("Can't generate config"))?;

    Ok((config, pool))
}

fn parse_env(args: Args) -> Args {
    let arg_connection = args.arg_connection.or_else(|| {
        env::var_os("DATABASE_URL").and_then(|connection| connection.into_string().ok())
    });

    let flag_danger_accept_invalid_certs = args.flag_danger_accept_invalid_certs
        || env::var_os("DANGER_ACCEPT_INVALID_CERTS").is_some();

    let flag_watch = args.flag_watch || env::var_os("WATCH_MODE").is_some();

    Args {
        arg_connection,
        flag_watch,
        flag_danger_accept_invalid_certs,
        ..args
    }
}

fn start(args: Args) -> io::Result<actix::SystemRunner> {
    info!("Starting martin v{}", VERSION);

    let (config, pool) = match args.flag_config {
        Some(config_file_name) => {
            info!("Using {}", config_file_name);
            setup_from_config(config_file_name)?
        }
        None => {
            info!("Config is not set");
            setup_from_args(args)?
        }
    };

    let matches = check_postgis_version(REQUIRED_POSTGIS_VERSION, &pool)
        .map_err(prettify_error("Can't check PostGIS version"))?;

    if !matches {
        std::process::exit(-1);
    }

    let listen_addresses = config.listen_addresses.clone();
    let server = server::new(pool, config);
    info!("Martin has been started on {}.", listen_addresses);

    Ok(server)
}

fn main() -> io::Result<()> {
    let env = env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "martin=info");
    env_logger::Builder::from_env(env).init();

    let args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize::<Args>())
        .map_err(prettify_error("Can't parse CLI arguments"))?;

    let args = parse_env(args);

    if args.flag_help {
        println!("{}", USAGE);
        std::process::exit(0);
    }

    if args.flag_version {
        println!("v{}", VERSION);
        std::process::exit(0);
    }

    if args.flag_danger_accept_invalid_certs {
        warn!("Danger accept invalid certs enabled. You should think very carefully before using this option. If invalid certificates are trusted, any certificate for any site will be trusted for use. This includes expired certificates. This introduces significant vulnerabilities, and should only be used as a last resort.");
    }

    if args.flag_watch {
        info!("Watch mode enabled");
    }

    let server = match start(args) {
        Ok(server) => server,
        Err(error) => {
            error!("{}", error);
            std::process::exit(-1);
        }
    };

    server.run()
}
