Antigravity IDE Project Prompt: Hoops Stats API
===============================================

1\. Project Overview
--------------------

You are tasked with building a high-performance, memory-safe REST API for browsing basketball player profiles and their advanced quantitative statistics. This system should act as a foundational data pipeline capable of serving downstream sports analytics dashboards or agentic orchestrators.

2\. Technology Stack
--------------------

You must strictly adhere to the following stack:

-   **Language:** Rust (Stable)

-   **Web Framework:** Axum

-   **Async Runtime:** Tokio

-   **Database:** PostgreSQL (Run via a local Docker container)

-   **Database Connector:** SQLx (You MUST utilize SQLx's compile-time query verification features, e.g., `query_as!`)

-   **Serialization:** Serde & Serde JSON

-   **Environment Management:** `dotenvy`

3\. Database Schema Requirements
--------------------------------

Design and write migrations for a relational PostgreSQL database with the following structure. Use `sqlx-cli` to handle the migrations.

-   **Table: `players`**

    -   `id`: Primary Key (Auto-incrementing)

    -   `name`: String (Required)

    -   `position`: String (Required)

    -   `archetype`: String (Optional, e.g., "Physical Slasher", "Two-Way Wing")

-   **Table: `stats`**

    -   `id`: Primary Key

    -   `player_id`: Foreign Key referencing `players.id` (Cascade on delete)

    -   `season`: String (Required, e.g., "2023-24")

    -   `points_per_game`: Float/Real (Required)

    -   `true_shooting_pct`: Float/Real (Required)

    -   `usage_rate`: Float/Real (Required)

**Seeding Requirement:** Write a migration or script to seed the database with at least 3 notable modern NBA players (e.g., Anthony Edwards, Giannis Antetokounmpo, Shai Gilgeous-Alexander) and realistic advanced metrics for the 2023-24 season.

4\. Application Architecture & API Endpoints
--------------------------------------------

Write the application logic (primarily in `src/main.rs`). You must set up a connection pool to Postgres and share it as Application State across Axum routes.

Implement the following endpoints:

1.  `GET /players`

    -   **Behavior:** Fetches a list of all players from the database.

    -   **Response:** A JSON array of player objects.

2.  `GET /players/:id/stats`

    -   **Behavior:** Extracts the `id` from the URL path, performs a SQL `JOIN` between the `stats` and `players` tables, and retrieves the advanced stats alongside the player's name.

    -   **Response:** A JSON array of stat objects for that specific player.

5\. Execution Instructions for Antigravity
------------------------------------------

1.  Initialize the Cargo project workspace.

2.  Add all necessary crates to `Cargo.toml`.

3.  Spin up an isolated Postgres Docker container.

4.  Generate and run the SQLx migrations.

5.  Write the Rust server code, ensuring strict type safety and leveraging SQLx compile-time checks.

6.  Ensure a `.env` file is appropriately created and read by the application.

7.  Compile, run, and test the endpoints to ensure there are no compilation errors or runtime panics.