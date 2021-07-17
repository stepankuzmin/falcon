SELECT
  ST_AsMVTGeom (ST_Transform (ST_CurveToLine({geometry_column}), 3857), {mercator_bounds}, {extent}, {buffer}, {clip_geom}) AS geom {properties} FROM {id}, bounds
  WHERE
    {geometry_column} && bounds.srid_{srid}
