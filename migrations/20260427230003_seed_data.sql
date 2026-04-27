-- Seed NBA players
INSERT INTO players (name, position, archetype) VALUES
    ('Anthony Edwards', 'SG', 'Physical Slasher'),
    ('Giannis Antetokounmpo', 'PF', 'Interior Force'),
    ('Shai Gilgeous-Alexander', 'PG', 'Two-Way Shot Creator');

-- Seed 2023-24 advanced stats
INSERT INTO stats (player_id, season, points_per_game, true_shooting_pct, usage_rate) VALUES
    (1, '2023-24', 25.9, 0.574, 31.8),
    (2, '2023-24', 30.4, 0.614, 34.6),
    (3, '2023-24', 30.1, 0.618, 33.0);
