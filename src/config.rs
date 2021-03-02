use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io;
use std::io::prelude::*;

use crate::function_source::FunctionSources;
use crate::table_source::TableSources;

#[derive(Clone, Debug, Serialize)]
pub struct Config {
    pub watch: bool,
    pub pool_size: u32,
    pub keep_alive: usize,
    pub worker_processes: usize,
    pub listen_addresses: String,
    pub connection_string: String,
    pub table_sources: Option<TableSources>,
    pub function_sources: Option<FunctionSources>,
    pub danger_accept_invalid_certs: bool,
    pub jwt: bool,
    pub jwt_secret: String,
    pub jwt_algorithm: String,
    pub jwt_check_exp_time: bool,
}

#[derive(Deserialize)]
pub struct ConfigBuilder {
    pub watch: Option<bool>,
    pub pool_size: Option<u32>,
    pub keep_alive: Option<usize>,
    pub worker_processes: Option<usize>,
    pub listen_addresses: Option<String>,
    pub connection_string: String,
    pub table_sources: Option<TableSources>,
    pub function_sources: Option<FunctionSources>,
    pub danger_accept_invalid_certs: Option<bool>,
    pub jwt: Option<bool>,
    pub jwt_secret: Option<String>,
    pub jwt_algorithm: Option<String>,
    pub jwt_check_exp_time: Option<bool>,
}

impl ConfigBuilder {
    pub fn finalize(self) -> Config {
        Config {
            watch: self.watch.unwrap_or(false),
            pool_size: self.pool_size.unwrap_or(20),
            keep_alive: self.keep_alive.unwrap_or(75),
            worker_processes: self.worker_processes.unwrap_or_else(num_cpus::get),
            listen_addresses: self
                .listen_addresses
                .unwrap_or_else(|| "0.0.0.0:3000".to_owned()),
            connection_string: self.connection_string,
            table_sources: self.table_sources,
            function_sources: self.function_sources,
            danger_accept_invalid_certs: self.danger_accept_invalid_certs.unwrap_or(false),
            jwt: self.jwt.unwrap_or(false),
            jwt_secret: self.jwt_secret.unwrap_or_else(|| "".to_owned()),
            jwt_algorithm: self.jwt_algorithm.unwrap_or_else(|| "".to_owned()),
            jwt_check_exp_time: self.jwt_check_exp_time.unwrap_or(false),
        }
    }
}

pub fn read_config(file_name: &str) -> io::Result<Config> {
    let mut file = File::open(file_name)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    let config_builder: ConfigBuilder = serde_yaml::from_str(contents.as_str())
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;

    Ok(config_builder.finalize())
}
