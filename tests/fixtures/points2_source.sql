CREATE TABLE points2(gid SERIAL PRIMARY KEY, geom GEOMETRY(POINT, 4326));

INSERT INTO points2
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

CREATE INDEX ON points2 USING GIST(geom);
CLUSTER points2_geom_idx ON points2;
