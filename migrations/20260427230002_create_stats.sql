CREATE TABLE stats (
    id SERIAL PRIMARY KEY,
    player_id INT NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    season TEXT NOT NULL,
    points_per_game REAL NOT NULL,
    true_shooting_pct REAL NOT NULL,
    usage_rate REAL NOT NULL
);
