-- The three hand-seeded players from 20260427230003_seed_data.sql were a
-- placeholder for having no real data. Ingest now keys players on their NBA id,
-- so those rows cannot be matched by ON CONFLICT and reappear as duplicates
-- alongside their ingested counterparts.
--
-- Rows are identified by the absence of an nba_player_id, which at this point in
-- the migration sequence is exactly the seed set. users.favorite_player_id is
-- ON DELETE SET NULL, so no user rows are lost.
DELETE FROM players WHERE nba_player_id IS NULL;
