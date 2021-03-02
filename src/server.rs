use serde::Deserialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use actix::{Actor, Addr, SyncArbiter, SystemRunner};
use actix_cors::Cors;
use actix_web::{
    dev, error, http, middleware, web, App, Error, HttpRequest, HttpResponse, HttpServer, Result,
};
use actix_web_httpauth::{extractors::bearer::BearerAuth, middleware::HttpAuthentication};

use crate::config::Config;
use crate::coordinator_actor::CoordinatorActor;
use crate::db::Pool;
use crate::db_actor::DBActor;
use crate::function_source::FunctionSources;
use crate::messages;
use crate::source::{Source, XYZ};
use crate::table_source::TableSources;
use crate::worker_actor::WorkerActor;

// For JWT
use jsonwebtokens as jwt;
use jwt::{raw, Algorithm, AlgorithmID, Verifier};
use std::str::FromStr;
use std::time::SystemTime;

pub struct JWTConfig {
    pub jwt_secret: String,
    pub jwt_algorithm: String,
    pub jwt_check_exp_time: bool,
}

pub struct AppState {
    pub db: Addr<DBActor>,
    pub coordinator: Addr<CoordinatorActor>,
    pub table_sources: Rc<RefCell<Option<TableSources>>>,
    pub function_sources: Rc<RefCell<Option<FunctionSources>>>,
    pub watch_mode: bool,
}

#[derive(Deserialize)]
struct SourceRequest {
    source_id: String,
}

#[derive(Deserialize)]
struct TileRequest {
    source_id: String,
    z: u32,
    x: u32,
    y: u32,
    #[allow(dead_code)]
    format: String,
}

async fn get_health() -> Result<HttpResponse, Error> {
    let response = HttpResponse::Ok().body("OK");
    return Ok(response);
}

async fn get_table_sources(state: web::Data<AppState>) -> Result<HttpResponse, Error> {
    if !state.watch_mode {
        let table_sources = state.table_sources.borrow().clone();
        let response = HttpResponse::Ok().json(table_sources);
        return Ok(response);
    }

    info!("Scanning database for table sources");

    let table_sources = state
        .db
        .send(messages::GetTableSources {})
        .await
        .map_err(|_| HttpResponse::InternalServerError())?
        .map_err(|_| HttpResponse::InternalServerError())?;

    state.coordinator.do_send(messages::RefreshTableSources {
        table_sources: Some(table_sources.clone()),
    });

    Ok(HttpResponse::Ok().json(table_sources))
}

async fn get_table_source(
    req: HttpRequest,
    path: web::Path<SourceRequest>,
    state: web::Data<AppState>,
) -> Result<HttpResponse> {
    let table_sources = state
        .table_sources
        .borrow()
        .clone()
        .ok_or_else(|| error::ErrorNotFound("There is no table sources"))?;

    let source = table_sources.get(&path.source_id).ok_or_else(|| {
        error::ErrorNotFound(format!("Table source '{}' not found", path.source_id))
    })?;

    let mut tilejson = source
        .get_tilejson()
        .map_err(|e| error::ErrorBadRequest(format!("Can't build TileJSON: {}", e)))?;

    let tiles_path = req
        .headers()
        .get("x-rewrite-url")
        .map_or(Ok(req.path().trim_end_matches(".json")), |header| {
            let header_str = header.to_str()?;
            Ok(header_str.trim_end_matches(".json"))
        })
        .map_err(|e: http::header::ToStrError| {
            error::ErrorBadRequest(format!("Can't build TileJSON: {}", e))
        })?;

    let query_string = req.query_string();
    let query = if query_string.is_empty() {
        query_string.to_owned()
    } else {
        format!("?{}", query_string)
    };

    let connection_info = req.connection_info();

    let tiles_url = format!(
        "{}://{}{}/{{z}}/{{x}}/{{y}}.pbf{}",
        connection_info.scheme(),
        connection_info.host(),
        tiles_path,
        query
    );

    tilejson.tiles = vec![tiles_url];
    Ok(HttpResponse::Ok().json(tilejson))
}

async fn get_table_source_tile(
    path: web::Path<TileRequest>,
    state: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    let table_sources = state
        .table_sources
        .borrow()
        .clone()
        .ok_or_else(|| error::ErrorNotFound("There is no table sources"))?;

    let source = table_sources.get(&path.source_id).ok_or_else(|| {
        error::ErrorNotFound(format!("Table source '{}' not found", path.source_id))
    })?;

    let xyz = XYZ {
        z: path.z,
        x: path.x,
        y: path.y,
    };

    let message = messages::GetTile {
        xyz,
        query: None,
        source: source.clone(),
    };

    let tile = state
        .db
        .send(message)
        .await
        .map_err(|_| HttpResponse::InternalServerError())?
        .map_err(|_| HttpResponse::InternalServerError())?;

    match tile.len() {
        0 => Ok(HttpResponse::NoContent()
            .content_type("application/x-protobuf")
            .body(tile)),
        _ => Ok(HttpResponse::Ok()
            .content_type("application/x-protobuf")
            .body(tile)),
    }
}

async fn get_function_sources(state: web::Data<AppState>) -> Result<HttpResponse, Error> {
    if !state.watch_mode {
        let function_sources = state.function_sources.borrow().clone();
        let response = HttpResponse::Ok().json(function_sources);
        return Ok(response);
    }

    info!("Scanning database for function sources");

    let function_sources = state
        .db
        .send(messages::GetFunctionSources {})
        .await
        .map_err(|_| HttpResponse::InternalServerError())?
        .map_err(|_| HttpResponse::InternalServerError())?;

    state.coordinator.do_send(messages::RefreshFunctionSources {
        function_sources: Some(function_sources.clone()),
    });

    Ok(HttpResponse::Ok().json(function_sources))
}

async fn get_function_source(
    req: HttpRequest,
    path: web::Path<SourceRequest>,
    state: web::Data<AppState>,
) -> Result<HttpResponse> {
    let function_sources = state
        .function_sources
        .borrow()
        .clone()
        .ok_or_else(|| error::ErrorNotFound("There is no function sources"))?;

    let source = function_sources.get(&path.source_id).ok_or_else(|| {
        error::ErrorNotFound(format!("Function source '{}' not found", path.source_id))
    })?;

    let mut tilejson = source
        .get_tilejson()
        .map_err(|e| error::ErrorBadRequest(format!("Can't build TileJSON: {}", e)))?;

    let tiles_path = req
        .headers()
        .get("x-rewrite-url")
        .map_or(Ok(req.path().trim_end_matches(".json")), |header| {
            let header_str = header.to_str()?;
            Ok(header_str.trim_end_matches(".json"))
        })
        .map_err(|e: http::header::ToStrError| {
            error::ErrorBadRequest(format!("Can't build TileJSON: {}", e))
        })?;

    let query_string = req.query_string();
    let query = if query_string.is_empty() {
        query_string.to_owned()
    } else {
        format!("?{}", query_string)
    };

    let connection_info = req.connection_info();

    let tiles_url = format!(
        "{}://{}{}/{{z}}/{{x}}/{{y}}.pbf{}",
        connection_info.scheme(),
        connection_info.host(),
        tiles_path,
        query
    );

    tilejson.tiles = vec![tiles_url];
    Ok(HttpResponse::Ok().json(tilejson))
}

async fn get_function_source_tile(
    path: web::Path<TileRequest>,
    query: web::Query<HashMap<String, String>>,
    state: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    let function_sources = state
        .function_sources
        .borrow()
        .clone()
        .ok_or_else(|| error::ErrorNotFound("There is no function sources"))?;

    let source = function_sources.get(&path.source_id).ok_or_else(|| {
        error::ErrorNotFound(format!("Function source '{}' not found", path.source_id))
    })?;

    let xyz = XYZ {
        z: path.z,
        x: path.x,
        y: path.y,
    };

    let message = messages::GetTile {
        xyz,
        query: Some(query.into_inner()),
        source: source.clone(),
    };

    let tile = state
        .db
        .send(message)
        .await
        .map_err(|_| HttpResponse::InternalServerError())?
        .map_err(|_| HttpResponse::InternalServerError())?;

    match tile.len() {
        0 => Ok(HttpResponse::NoContent()
            .content_type("application/x-protobuf")
            .body(tile)),
        _ => Ok(HttpResponse::Ok()
            .content_type("application/x-protobuf")
            .body(tile)),
    }
}

pub fn router(cfg: &mut web::ServiceConfig) {
    cfg.route("/index.json", web::get().to(get_table_sources))
        .route("/{source_id}.json", web::get().to(get_table_source))
        .route("/healthz", web::get().to(get_health))
        .route(
            "/{source_id}/{z}/{x}/{y}.{format}",
            web::get().to(get_table_source_tile),
        )
        .route("/rpc/index.json", web::get().to(get_function_sources))
        .route("/rpc/{source_id}.json", web::get().to(get_function_source))
        .route(
            "/rpc/{source_id}/{z}/{x}/{y}.{format}",
            web::get().to(get_function_source_tile),
        );
}

fn create_state(
    db: Addr<DBActor>,
    coordinator: Addr<CoordinatorActor>,
    config: Config,
) -> AppState {
    let table_sources = Rc::new(RefCell::new(config.table_sources));
    let function_sources = Rc::new(RefCell::new(config.function_sources));

    let worker_actor = WorkerActor {
        table_sources: table_sources.clone(),
        function_sources: function_sources.clone(),
    };

    let worker: Addr<_> = worker_actor.start();
    coordinator.do_send(messages::Connect { addr: worker });

    AppState {
        db,
        coordinator,
        table_sources,
        function_sources,
        watch_mode: config.watch,
    }
}

async fn bearer_auth_validator(
    req: dev::ServiceRequest,
    credentials: BearerAuth,
) -> Result<dev::ServiceRequest, Error> {
    let jwt_config = req.app_data::<JWTConfig>().unwrap();

    let try_catch_block = || -> Result<(Verifier, Algorithm, bool), jwt::error::Error> {
        let header_json;
        let raw::TokenSlices { header, claims, .. } = raw::split_token(credentials.token())?;
        let claims_json = raw::decode_json_token_slice(claims)?;
        let alg_name = if jwt_config.jwt_algorithm.is_empty() {
            header_json = raw::decode_json_token_slice(header)?;
            header_json["alg"].as_str().unwrap_or("")
        } else {
            jwt_config.jwt_algorithm.as_str()
        };
        let alg_id = AlgorithmID::from_str(alg_name)?;

        Ok((
            Verifier::create().build()?,
            Algorithm::new_hmac(alg_id, jwt_config.jwt_secret.as_str())?,
            claims_json["exp"].is_null(),
        ))
    };

    match try_catch_block() {
        Ok((verifier, alg, exp_is_null)) => {
            let result = if jwt_config.jwt_check_exp_time {
                if exp_is_null {
                    return Err(error::ErrorForbidden("Claim exp does not exist."));
                }

                let now_unixtimestamp = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                match verifier.verify_for_time(&credentials.token(), &alg, now_unixtimestamp) {
                    Ok(_) => Ok(true),
                    Err(e) => Err(e),
                }
            } else {
                match verifier.verify(&credentials.token(), &alg) {
                    Ok(_) => Ok(true),
                    Err(e) => Err(e),
                }
            };
            match result {
                Ok(_) => Ok(req),
                Err(e) => {
                    info!(
                        "Error verify JWT: token \"{}\" error \"{}\".",
                        credentials.token(),
                        e.to_string()
                    );
                    Err(error::ErrorForbidden(e.to_string()))
                }
            }
        }
        Err(e) => {
            info!(
                "Error generate algorith and verifier JWT: token \"{}\" error \"{}\".",
                credentials.token(),
                e.to_string()
            );
            Err(error::ErrorForbidden(e.to_string()))
        }
    }
}

pub fn new(pool: Pool, config: Config) -> SystemRunner {
    let sys = actix_rt::System::new("server");

    let db = SyncArbiter::start(3, move || DBActor(pool.clone()));
    let coordinator: Addr<_> = CoordinatorActor::default().start();

    let keep_alive = config.keep_alive;
    let worker_processes = config.worker_processes;
    let listen_addresses = config.listen_addresses.clone();

    HttpServer::new(move || {
        let state = create_state(db.clone(), coordinator.clone(), config.clone());

        let jwt_config = JWTConfig {
            jwt_secret: config.jwt_secret.clone(),
            jwt_algorithm: config.jwt_algorithm.clone(),
            jwt_check_exp_time: config.jwt_check_exp_time,
        };

        let cors_middleware = Cors::default().allow_any_origin();
        let auth = HttpAuthentication::bearer(bearer_auth_validator);

        App::new()
            .app_data(jwt_config)
            .data(state)
            .wrap(cors_middleware)
            .wrap(middleware::NormalizePath::new(
                middleware::normalize::TrailingSlash::MergeOnly,
            ))
            .wrap(middleware::Logger::default())
            .wrap(middleware::Compress::default())
            .wrap(middleware::Condition::new(config.jwt, auth))
            .configure(router)
    })
    .bind(listen_addresses.clone())
    .unwrap_or_else(|_| panic!("Can't bind to {}", listen_addresses))
    .keep_alive(keep_alive)
    .shutdown_timeout(0)
    .workers(worker_processes)
    .run();

    sys
}
