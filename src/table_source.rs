use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;

use tilejson::{TileJSON, TileJSONBuilder};

use crate::db::Connection;
use crate::source::{Query, Source, Tile, XYZ};
use crate::utils;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TableSource {
    pub id: String,
    pub schema: String,
    pub table: String,
    pub id_column: Option<String>,
    pub geometry_column: String,
    pub srid: u32,
    pub extent: Option<u32>,
    pub buffer: Option<u32>,
    pub clip_geom: Option<bool>,
    pub geometry_type: Option<String>,
    pub properties: HashMap<String, String>,
    pub bounds: Vec<f32>
}

pub type TableSources = HashMap<String, Box<TableSource>>;

impl Source for TableSource {
    fn get_id(&self) -> &str {
        self.id.as_str()
    }

    fn get_tilejson(&self) -> Result<TileJSON, io::Error> {
        let mut tilejson_builder = TileJSONBuilder::new();

        tilejson_builder.scheme("xyz");
        tilejson_builder.name(&self.id);
        tilejson_builder.bounds(self.bounds.to_vec());

        Ok(tilejson_builder.finalize())
    }

    fn get_tile(
        &self,
        conn: &mut Connection,
        xyz: &XYZ,
        _query: &Option<Query>,
    ) -> Result<Tile, io::Error> {
        let mercator_bounds = utils::tilebbox(xyz);

        let (geometry_column_mercator, original_bounds) = if self.srid == 3857 {
            (self.geometry_column.clone(), mercator_bounds.clone())
        } else {
            (
                format!("ST_Transform({0}, 3857)", self.geometry_column),
                format!("ST_Transform({0}, {1})", mercator_bounds, self.srid),
            )
        };

        let properties = if self.properties.is_empty() {
            "".to_string()
        } else {
            let properties = self
                .properties
                .keys()
                .map(|column| format!("\"{0}\"", column))
                .collect::<Vec<String>>()
                .join(",");

            format!(", {0}", properties)
        };

        let id_column = self
            .id_column
            .clone()
            .map_or("".to_string(), |id_column| format!(", '{}'", id_column));

        let query = format!(
            include_str!("scripts/get_tile.sql"),
            id = self.id,
            id_column = id_column,
            geometry_column = self.geometry_column,
            geometry_column_mercator = geometry_column_mercator,
            original_bounds = original_bounds,
            mercator_bounds = mercator_bounds,
            extent = self.extent.unwrap_or(DEFAULT_EXTENT),
            buffer = self.buffer.unwrap_or(DEFAULT_BUFFER),
            clip_geom = self.clip_geom.unwrap_or(DEFAULT_CLIP_GEOM),
            properties = properties
        );

        let tile: Tile = conn
            .query_one(query.as_str(), &[])
            .map(|row| row.get("st_asmvt"))
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;

        Ok(tile)
    }
}

static DEFAULT_EXTENT: u32 = 4096;
static DEFAULT_BUFFER: u32 = 64;
static DEFAULT_CLIP_GEOM: bool = true;

pub fn get_table_sources(conn: &mut Connection) -> Result<TableSources, io::Error> {
    let mut sources = HashMap::new();

    let rows = conn
        .query(include_str!("scripts/get_table_sources.sql"), &[])
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;

    for row in &rows {
        let schema: String = row.get("f_table_schema");
        let table: String = row.get("f_table_name");
        let id = format!("{}.{}", schema, table);

        let geometry_column: String = row.get("f_geometry_column");
        let srid: i32 = row.get("srid");

        let query_bounds = format!("SELECT ST_Extent(geom)::TEXT as bounds from  {}", id);
        let rows_bounds=conn.query(&*query_bounds, &[]).map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;
        let mut bounds: Vec<f32> = Vec::new();
        for row_bounds in &rows_bounds {
            let bounds_string_column:String = row_bounds.get("bounds");
            let new_string:String = bounds_string_column.replace("(", "").replace(")", "").replace("BOX", "").replace(" ", ",");

            let bounds_string:Vec<String>  =new_string.split(',').map(|s| s.to_string()).collect();

            bounds.push(bounds_string[0].parse::<f32>().unwrap());
            bounds.push(bounds_string[1].parse::<f32>().unwrap());
            bounds.push(bounds_string[2].parse::<f32>().unwrap());
            bounds.push(bounds_string[3].parse::<f32>().unwrap()); 
            break;
        }

        info!("Found {} table source", id);

        if srid == 0 {
            warn!("{} has SRID 0, skipping", id);
            continue;
        }

        let properties = utils::json_to_hashmap(&row.get("properties"));

        let source = TableSource {
            id: id.to_string(),
            schema,
            table,
            id_column: None,
            geometry_column,
            srid: srid as u32,
            extent: Some(DEFAULT_EXTENT),
            buffer: Some(DEFAULT_BUFFER),
            clip_geom: Some(DEFAULT_CLIP_GEOM),
            geometry_type: row.get("type"),
            properties,
            bounds
        };

        sources.insert(id, Box::new(source));
    }

    if sources.is_empty() {
        info!("No table sources found");
    }

    Ok(sources)
}
