use std::str::FromStr;
use std::{env, io};

use native_tls::{Certificate, Identity, TlsConnector};
use postgres_native_tls::MakeTlsConnector;
use r2d2::PooledConnection;
use r2d2_postgres::PostgresConnectionManager;
use semver::Version;
use semver::VersionReq;
use std::fs::File;
use std::io::Read;

use crate::utils::prettify_error;

pub type ConnectionManager = PostgresConnectionManager<MakeTlsConnector>;
pub type Pool = r2d2::Pool<ConnectionManager>;
pub type Connection = PooledConnection<ConnectionManager>;

fn make_tls_connector(danger_accept_invalid_certs: bool) -> io::Result<MakeTlsConnector> {
    let key = "CA_ROOT_FILE";
    let ca_file = match env::var_os(key) {
        Some(s) => s.into_string().unwrap(),
        None => {
            println!("{} is not defined in the environment", key);
            String::default()
        }
    };
    let key = "CLIENT_PKCS12_FILE";
    let client_identity_file = match env::var_os(key) {
        Some(s) => s.into_string().unwrap(),
        None => {
            println!("{} is not defined in the environment", key);
            String::default()
        }
    };
    let key = "CLIENT_PKCS12_PASS";
    let client_identity_pass = match env::var_os(key) {
        Some(s) => s.into_string().unwrap(),
        None => {
            println!("{} is not defined in the environment", key);
            String::default()
        }
    };

    let mut builder = TlsConnector::builder();

    if !client_identity_file.is_empty() {
        let mut file = File::open(&client_identity_file).unwrap();
        let mut identity = vec![];
        file.read_to_end(&mut identity).unwrap();
        let identity = Identity::from_pkcs12(&identity, &client_identity_pass).unwrap();
        builder.identity(identity);
    }
    if !ca_file.is_empty() {
        let mut ca = File::open(&ca_file).unwrap();
        let mut buf = Vec::new();
        ca.read_to_end(&mut buf).unwrap();
        let cert = Certificate::from_pem(&buf).unwrap();
        builder.add_root_certificate(cert);
    }
    let connector = builder
        .danger_accept_invalid_certs(danger_accept_invalid_certs)
        .build()
        .map_err(prettify_error("Can't build TLS connection"))?;

    let tls_connector = MakeTlsConnector::new(connector);
    Ok(tls_connector)
}

pub fn setup_connection_pool(
    cn_str: &str,
    pool_size: Option<u32>,
    danger_accept_invalid_certs: bool,
) -> io::Result<Pool> {
    let config = postgres::config::Config::from_str(cn_str)
        .map_err(prettify_error("Can't parse connection string"))?;

    let tls_connector = make_tls_connector(danger_accept_invalid_certs)?;
    let manager = PostgresConnectionManager::new(config, tls_connector);

    let pool = r2d2::Pool::builder()
        .max_size(pool_size.unwrap_or(20))
        .build(manager)
        .map_err(prettify_error("Can't build connection pool"))?;

    Ok(pool)
}

pub fn get_connection(pool: &Pool) -> io::Result<Connection> {
    let connection = pool
        .get()
        .map_err(prettify_error("Can't retrieve connection from the pool"))?;

    Ok(connection)
}

pub fn select_postgis_verion(pool: &Pool) -> io::Result<String> {
    let mut connection = get_connection(pool)?;

    let version = connection
        .query_one(include_str!("scripts/get_postgis_version.sql"), &[])
        .map(|row| row.get::<_, String>("postgis_version"))
        .map_err(prettify_error("Can't get PostGIS version"))?;

    Ok(version)
}

pub fn check_postgis_version(required_postgis_version: &str, pool: &Pool) -> io::Result<bool> {
    let postgis_version = select_postgis_verion(&pool)?;

    let req = VersionReq::parse(required_postgis_version)
        .map_err(prettify_error("Can't parse required PostGIS version"))?;

    let version = Version::parse(postgis_version.as_str())
        .map_err(prettify_error("Can't parse database PostGIS version"))?;

    let matches = req.matches(&version);

    if !matches {
        error!(
            "Martin requires PostGIS {}, current version is {}",
            required_postgis_version, postgis_version
        );
    }

    Ok(matches)
}
