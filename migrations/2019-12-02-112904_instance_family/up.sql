-- Your SQL goes here
CREATE TABLE instance_family (
    id SERIAL PRIMARY KEY,
    family_name TEXT NOT NULL UNIQUE,
    family_type TEXT NOT NULL
)
