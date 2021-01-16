CREATE TABLE points1(gid SERIAL PRIMARY KEY, geom GEOMETRY(POINT, 4326));

INSERT INTO points1
    SELECT
        generate_series(1, 10000) as id,
        (
            ST_DUMP(
                ST_GENERATEPOINTS(
                    ST_GEOMFROMTEXT('POLYGON ((-180 90, 180 90, 180 -90, -180 -90, -180 90))', 4326),
                    10000
                )
            )
        ).geom;

CREATE INDEX ON points1 USING GIST(geom);
CLUSTER points1_geom_idx ON points1;
