-- Support ingesting live stats from stats.nba.com instead of hand-seeded rows.

-- Key players by their NBA id so repeated ingests update rather than duplicate.
ALTER TABLE players ADD COLUMN nba_player_id INT UNIQUE;
ALTER TABLE players ADD COLUMN team TEXT;

-- Box-score context the league actually cares about when setting a lineup.
ALTER TABLE stats ADD COLUMN games_played INT;
ALTER TABLE stats ADD COLUMN minutes_per_game REAL;
ALTER TABLE stats ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

-- Official usage rate needs team possession totals, which the only reachable
-- stats.nba.com endpoint does not expose. Ingested rows store NULL here rather
-- than a fabricated number; hand-entered rows may still set it.
ALTER TABLE stats ALTER COLUMN usage_rate DROP NOT NULL;

-- One stat row per player per season, so ingest can upsert.
ALTER TABLE stats ADD CONSTRAINT stats_player_season_unique UNIQUE (player_id, season);
