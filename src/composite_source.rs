use std::io;

use tilejson::{TileJSON, TileJSONBuilder};

use crate::db::Connection;
use crate::source::{Query, Source, Tile, XYZ};
use crate::table_source::TableSource;

#[derive(Clone, Debug)]
pub struct CompositeSource {
    pub id: String,
    pub table_sources: Vec<TableSource>,
}

impl Source for CompositeSource {
    fn get_id(&self) -> &str {
        self.id.as_str()
    }

    fn get_tilejson(&self) -> Result<TileJSON, io::Error> {
        let mut tilejson_builder = TileJSONBuilder::new();

        tilejson_builder.scheme("xyz");
        tilejson_builder.name(&self.id);

        Ok(tilejson_builder.finalize())
    }

    fn get_tile(
        &self,
        conn: &mut Connection,
        xyz: &XYZ,
        _query: &Option<Query>,
    ) -> Result<Tile, io::Error> {
        let tile: Tile = self
            .table_sources
            .clone()
            .into_iter()
            .filter_map(|source| source.get_tile(conn, xyz, _query).ok())
            .collect::<Vec<Tile>>()
            .concat();

        Ok(tile)
    }
}
