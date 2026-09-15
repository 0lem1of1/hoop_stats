-- Player names come from the NBA feed with their real diacritics (Dončić,
-- Jokić, Šengün), but nobody types those. Name lookups fold accents on both
-- sides so "doncic" finds "Dončić".
--
-- unaccent() is not IMMUTABLE, so it cannot back an index. At a few hundred
-- players a sequential scan is not worth optimising; revisit if the roster
-- grows by orders of magnitude.
CREATE EXTENSION IF NOT EXISTS unaccent;
